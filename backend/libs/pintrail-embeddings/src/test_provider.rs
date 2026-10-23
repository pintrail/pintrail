//! Predictable plumbing substitute. These vectors do NOT represent meaning.
use crate::{validate_inputs, EmbeddingError, EmbeddingProvider, InputKind, ModelInfo};

pub struct DeterministicProvider {
    info: ModelInfo,
}

impl Default for DeterministicProvider {
    fn default() -> Self {
        Self {
            info: ModelInfo {
                name: "test/deterministic".into(),
                repository: "none".into(),
                revision: Some("test-v1".into()),
                dimension: 3,
                max_tokens: usize::MAX,
                pipeline_version: "test-v1".into(),
            },
        }
    }
}

impl EmbeddingProvider for DeterministicProvider {
    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn embed(&mut self, texts: &[&str], _kind: InputKind) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        validate_inputs(texts)?;
        Ok(texts
            .iter()
            .map(|text| {
                let mut vector = vec![
                    1.0,
                    text.len() as f32,
                    text.split_whitespace().count() as f32,
                ];
                let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
                for value in &mut vector {
                    *value /= norm;
                }
                vector
            })
            .collect())
    }
}
