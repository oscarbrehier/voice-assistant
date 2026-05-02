use std::path::Path;

use ndarray::Array2;
use ort::{session::Session, value::Tensor};

pub struct SpeakerID {
    pub session: Session,
    pub reference_embedding: Option<Vec<f32>>,
    pub similarity_threshold: f32,
}

impl SpeakerID {
    pub fn new<P: AsRef<Path>>(model_path: P, threshold: f32) -> anyhow::Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(model_path)?;

        Ok(Self {
            session,
            reference_embedding: None,
            similarity_threshold: threshold,
        })
    }

    pub fn enroll(&mut self, audio_samples: &[f32]) -> anyhow::Result<()> {
        let embedding = self.extract_embedding(audio_samples)?;

        self.reference_embedding = Some(embedding);
        Ok(())
    }

    pub fn verify(&mut self, candidate_samples: &[f32]) -> anyhow::Result<bool> {
        let reference = self
            .reference_embedding
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SpeakerID has no enrolled reference"))?;

        let candidate = self.extract_embedding(candidate_samples)?;
        let similarity = self.cosine_similarity(&reference, &candidate);

        Ok(similarity >= self.similarity_threshold)
    }

    fn extract_embedding(&mut self, samples: &[f32]) -> anyhow::Result<Vec<f32>> {
        let array = ndarray::Array2::from_shape_vec((1, samples.len()), samples.to_vec())?
            .into_dyn();

        let input_tensor = Tensor::from_array(array)?;
    
        let outputs = self.session.run(ort::inputs!["input" => input_tensor])?;
    
        let (_, extracted_data) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(extracted_data.to_vec())
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
