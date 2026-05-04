use std::path::Path;

use ort::session::Session;

use mel_spec::fbank::{Fbank, FbankConfig};

use crate::audio::utils::resample_to_16khz;

pub struct EnrolmentState {
    pub current_step: usize,
    pub max_steps: usize,
    pub accumulated_embedding: Vec<Vec<f32>>,
}

impl EnrolmentState {
    pub fn new() -> Self {
        Self {
            current_step: 0,
            max_steps: 5,
            accumulated_embedding: Vec::new(),
        }
    }
}

pub struct SpeakerID {
    pub session: Session,
    pub reference_embedding: Option<Vec<f32>>,
    pub similarity_threshold: f32,
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

        let embedding = match profile_path {
            Some(p) => {
                let bytes = std::fs::read(p.as_ref())?;
                let decoded: Vec<f32> = bincode::deserialize(&bytes)?;
                Some(decoded)
            }
            None => None,
        };

        let enrolment_state = if embedding.is_some() {
            None
        } else {
            Some(EnrolmentState::new())
        };

        Ok(Self {
            session,
            reference_embedding: embedding,
            similarity_threshold: threshold,
            enrolment_state,
        })
    }

    pub fn is_enrolled(&self) -> bool {
        self.reference_embedding.is_some()
    }

    pub fn add_enrollment_sample(&mut self, audio_samples: &[f32]) -> anyhow::Result<bool> {
        let current_embedding = self.extract_embedding(audio_samples)?;

        let is_finished = {
            let state = self
                .enrolment_state
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("No active enrolment session"))?;

            state.accumulated_embedding.push(current_embedding);
            state.current_step += 1;

            state.current_step >= state.max_steps
        };

        if is_finished {
            self.finalize_enrolment()?;
            return Ok(true);
        }

        Ok(false)
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

        let encoded = bincode::serialize(&normalized)?;

        std::fs::create_dir_all("voices")?;
        std::fs::write("voices/voice_1.bin", encoded)?;

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

        Ok(similarity >= self.similarity_threshold)
    }

    fn extract_embedding(&mut self, samples: &[f32]) -> anyhow::Result<Vec<f32>> {
        let config = FbankConfig::default();
        let fbank = Fbank::new(config);

        let features = fbank.compute(samples);
        let (t_dim, mel_dim) = (features.nrows(), features.ncols());

        let raw_data = features
            .as_slice()
            .map(|s| s.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Could not flatten features"))?;

        let array_3d = ndarray::Array3::from_shape_vec((1, t_dim, mel_dim), raw_data)?;

        let input_tensor = ort::value::Value::from_array(array_3d)?;
        let outputs = self.session.run(ort::inputs!["feats" => input_tensor])?;

        let output_value = outputs
            .get("embs")
            .ok_or_else(|| anyhow::anyhow!("Output 'embs' not found"))?;

        let (_, extracted_slice) = output_value.try_extract_tensor::<f32>()?;

        Ok(extracted_slice.to_vec())
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
