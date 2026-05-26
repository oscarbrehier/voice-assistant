use std::path::{Path, PathBuf};

use ort::session::Session;

use crate::audio::utils::resample_to_16khz;
use mel_spec::fbank::{Fbank, FbankConfig};

pub struct EnrolmentState {
    pub current_step: usize,
    pub max_steps: usize,
    pub accumulated_embedding: Vec<Vec<f32>>,
    pub negative_embeddings: Vec<Vec<f32>>,
}

impl EnrolmentState {
    pub fn new() -> Self {
        Self {
            current_step: 0,
            max_steps: 8,
            accumulated_embedding: Vec::new(),
            negative_embeddings: Vec::new(),
        }
    }
}

pub struct SpeakerID {
    pub session: Session,
    pub profile_path: Option<PathBuf>,
    pub reference_embedding: Option<Vec<f32>>,
    pub negative_embeddings: Vec<Vec<f32>>,
    pub similarity_threshold: f32,
    pub recent_scores: Vec<f32>,
    pub enrolment_state: Option<EnrolmentState>,
}

impl SpeakerID {
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(
        model_path: P1,
        profile_path: Option<P2>,
        threshold: f32,
    ) -> anyhow::Result<Self> {
        let mut session_builder =
            Session::builder().map_err(|e| anyhow::anyhow!("Failed to create builder: {}", e))?;

        session_builder = session_builder
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("Failed to set optimization: {}", e))?;

        let session = session_builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {}", e))?;

        let owned_profile_path = profile_path.as_ref().map(|p| p.as_ref().to_path_buf());

        let (embedding, negative_embeddings) = match &owned_profile_path {
            Some(p) if p.exists() => {
                let bytes = std::fs::read(p)?;

                if let Ok((pos, neg)) = bincode::deserialize::<(Vec<f32>, Vec<Vec<f32>>)>(&bytes) {
                    (Some(pos), neg)
                } else {
                    let decoded: Vec<f32> = bincode::deserialize(&bytes)?;
                    (Some(decoded), Vec::new())
                }
            }
            _ => (None, Vec::new()),
        };

        let enrolment_state = if embedding.is_some() {
            None
        } else {
            Some(EnrolmentState::new())
        };

        Ok(Self {
            session,
            profile_path: owned_profile_path,
            reference_embedding: embedding,
            negative_embeddings,
            similarity_threshold: threshold,
            recent_scores: Vec::new(),
            enrolment_state,
        })
    }

    pub fn save_profile(&self) -> anyhow::Result<()> {
        let path = self
            .profile_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No profile path configured for this speaker"))?;

        let reference = self
            .reference_embedding
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No reference embedding to save"))?;

        let data_to_save = (reference.clone(), self.negative_embeddings.clone());

        let encoded = bincode::serialize(&data_to_save)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, encoded)?;

        Ok(())
    }

    pub fn is_enrolled(&self) -> bool {
        self.reference_embedding.is_some()
    }

    pub fn get_adaptive_threshold(&self) -> f32 {
        if self.recent_scores.len() < 10 {
            return self.similarity_threshold;
        }

        let mut sorted = self.recent_scores.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p = sorted[(sorted.len() as f32 * 0.70) as usize];
        p.max(self.similarity_threshold)
            .min(self.similarity_threshold + 0.03)
    }

    pub fn add_enrollment_sample(&mut self, audio_samples: &[f32]) -> anyhow::Result<bool> {
        let embeddings = self.extract_windowed_embedding(audio_samples)?;

        let is_finished = {
            let state = self
                .enrolment_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("No active enrolment session"))?;

            state.accumulated_embedding.extend(embeddings);
            state.current_step += 1;

            state.current_step >= state.max_steps
        };

        if is_finished {
            self.finalize_enrolment()?;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn add_negative_sample(&mut self, audio_samples: &[f32]) -> anyhow::Result<()> {
        let embedding = self.extract_embedding(audio_samples)?;

        if let Some(state) = &mut self.enrolment_state {
            state.negative_embeddings.push(embedding.clone());
        }

        self.negative_embeddings.push(embedding);

        Ok(())
    }

    fn finalize_enrolment(&mut self) -> anyhow::Result<()> {
        let state = self
            .enrolment_state
            .take()
            .ok_or_else(|| anyhow::anyhow!("State missing during finalization"))?;

        let dim = state.accumulated_embedding[0].len();
        let mut averaged = vec![0.0f32; dim];

        for emb in &state.accumulated_embedding {
            for i in 0..dim {
                averaged[i] += emb[i];
            }
        }

        for i in 0..dim {
            averaged[i] /= state.accumulated_embedding.len() as f32;
        }

        let norm = averaged.iter().map(|x| x * x).sum::<f32>().sqrt();
        let normalized: Vec<f32> = averaged.into_iter().map(|x| x / norm).collect();

        self.reference_embedding = Some(normalized.clone());
        self.negative_embeddings = state.negative_embeddings;

        if self.profile_path.is_some() {
            self.save_profile()?;
        }

        Ok(())
    }

    pub fn verify(
        &mut self,
        candidate_samples: &[f32],
        sample_rate: usize,
    ) -> anyhow::Result<bool> {
        let resampled_data = resample_to_16khz(candidate_samples, sample_rate);

        let reference = self
            .reference_embedding
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SpeakerID has no enrolled reference"))?;

        let candidate = self.extract_embedding(&resampled_data)?;
        let similarity = self.cosine_similarity(&reference, &candidate);

        println!("speaker similarity: {}", similarity);

        Ok(similarity >= self.similarity_threshold)
    }

    pub fn verify_with_negative_check(
        &mut self,
        candidate_samples: &[f32],
        sample_rate: usize,
    ) -> anyhow::Result<bool> {
        let resampled_data = resample_to_16khz(candidate_samples, sample_rate);

        let reference = self
            .reference_embedding
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SpeakerID has no enrolled reference"))?;

        let candidate = self.extract_embedding(&resampled_data)?;
        let positive_sim = self.cosine_similarity(&reference, &candidate);

        let max_negative_sim = self
            .negative_embeddings
            .iter()
            .map(|n| self.cosine_similarity(n, &candidate))
            .fold(0.0f32, |a, b| a.max(b));

        let threshold = self.get_adaptive_threshold();

        let is_positive_enough = positive_sim >= (threshold - 0.05);

        let is_distinctive = if self.negative_embeddings.is_empty() {
            true
        } else {
            if positive_sim > 0.70 {
                true
            } else {
                positive_sim >= max_negative_sim + 0.10
            }
        };

        let decision = is_positive_enough && is_distinctive;

        if positive_sim > 0.5 {
            self.recent_scores.push(positive_sim);
            if self.recent_scores.len() > 100 {
                self.recent_scores.remove(0);
            }
        }

        println!(
            "[speaker] pos={:.3} max_neg={:.3} thresh={:.3} pos_ok={} distinct={} -> decision={} (recent_scores={})",
            positive_sim,
            max_negative_sim,
            threshold,
            is_positive_enough,
            is_distinctive,
            decision,
            self.recent_scores.len(),
        );

        Ok(decision)
    }

    fn extract_embedding(&mut self, samples: &[f32]) -> anyhow::Result<Vec<f32>> {
        let config = FbankConfig {
            num_mel_bins: 80,
            apply_cmn: false,
            sample_rate: 16000.0,
            frame_length_ms: 25.0,
            frame_shift_ms: 10.0,
            energy_floor: 1.0,
            ..FbankConfig::default()
        };

        let fbank = Fbank::new(config);
        let features = fbank.compute(samples);

        let (t_dim, mel_dim) = (features.nrows(), features.ncols());

        let mut bin_means = vec![0.0f32; mel_dim];
        for t in 0..t_dim {
            for m in 0..mel_dim {
                bin_means[m] += features[(t, m)];
            }
        }
        for m in 0..mel_dim {
            bin_means[m] /= t_dim.max(1) as f32;
        }

        let mut standardized_data = Vec::with_capacity(t_dim * mel_dim);
        for t in 0..t_dim {
            for m in 0..mel_dim {
                standardized_data.push(features[(t, m)] - bin_means[m]);
            }
        }

        let array_3d = ndarray::Array3::from_shape_vec((1, t_dim, mel_dim), standardized_data)?;
        let input_tensor = ort::value::Value::from_array(array_3d)?;

        let outputs = self.session.run(ort::inputs!["feats" => input_tensor])?;

        let output_value = outputs
            .get("embs")
            .ok_or_else(|| anyhow::anyhow!("No embs"))?;
        let (_, extracted_slice) = output_value.try_extract_tensor::<f32>()?;

        let mut emb = extracted_slice.to_vec();
        let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        emb.iter_mut().for_each(|x| *x /= norm);

        Ok(emb)
    }

    fn extract_windowed_embedding(&mut self, samples: &[f32]) -> anyhow::Result<Vec<Vec<f32>>> {
        const SR: usize = 16000;
        let window = SR * 2;
        let hop = SR;
        let min_len = SR + SR / 2;

        let mut out = Vec::new();

        if samples.len() < window {
            if samples.len() >= min_len {
                out.push(self.extract_embedding(samples)?);
            }
        } else {
            let mut start = 0;

            while start + window <= samples.len() {
                out.push(self.extract_embedding(&samples[start..start + window])?);
                start += hop;
            }

            if start < samples.len() && samples.len() >= window {
                let tail_start = samples.len() - window;

                if tail_start > start.saturating_sub(hop) {
                    out.push(self.extract_embedding(&samples[tail_start..])?);
                }
            }
        }

        if out.is_empty() {
            out.push(self.extract_embedding(samples)?);
        }

        Ok(out)
    }

    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }
}
