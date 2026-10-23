//! Local embedding boundary shared by future indexing and query handlers.
//!
//! Inference is synchronous CPU work. Load once and call from a bounded blocking
//! task when integrating with Tokio; never hold database locks during inference.

mod local;
pub use local::{Config, LocalProvider, Model};

#[cfg(feature = "postgres")]
pub mod storage;

#[cfg(feature = "postgres")]
pub mod indexer;

#[cfg(any(test, feature = "test-provider"))]
pub mod test_provider;

use serde::Serialize;
use thiserror::Error;

pub const MAX_BATCH_SIZE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInfo {
    pub name: String,
    /// Identifies the ONNX export selected by FastEmbed, not just the model family.
    pub repository: String,
    /// Actual Hugging Face snapshot commit; populated after loading the model.
    pub revision: Option<String>,
    pub dimension: usize,
    pub max_tokens: usize,
    /// Bump if preprocessing or the selected model artifact changes.
    pub pipeline_version: String,
}

#[derive(Clone, Copy, Debug)]
pub enum InputKind {
    Document,
    Query,
}

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("invalid embedding configuration: {0}")]
    Configuration(String),
    #[error("input {index} is empty")]
    EmptyInput { index: usize },
    #[error("batch contains {count} texts; the limit is {MAX_BATCH_SIZE}")]
    BatchTooLarge { count: usize },
    #[error("input {index} has {tokens} tokens; the limit is {limit}; split it into passages")]
    InputTooLong {
        index: usize,
        tokens: usize,
        limit: usize,
    },
    #[error("could not initialize the local embedding model: {0}")]
    Initialization(String),
    #[error("local embedding inference failed: {0}")]
    Inference(String),
    #[error("invalid embedding output: {0}")]
    InvalidOutput(String),
}

/// The model is loaded once; each call returns vectors in the input order.
/// Documents and queries must use the same model and pipeline version.
pub trait EmbeddingProvider: Send {
    fn model_info(&self) -> &ModelInfo;
    fn embed(&mut self, texts: &[&str], kind: InputKind) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

fn validate_inputs(texts: &[&str]) -> Result<(), EmbeddingError> {
    if texts.len() > MAX_BATCH_SIZE {
        return Err(EmbeddingError::BatchTooLarge { count: texts.len() });
    }
    for (index, text) in texts.iter().enumerate() {
        if text.trim().is_empty() {
            return Err(EmbeddingError::EmptyInput { index });
        }
    }
    Ok(())
}

fn validate_vectors(
    vectors: &[Vec<f32>],
    count: usize,
    dimension: usize,
) -> Result<(), EmbeddingError> {
    if vectors.len() != count {
        return Err(EmbeddingError::InvalidOutput(
            "wrong number of vectors".into(),
        ));
    }
    for vector in vectors {
        if vector.len() != dimension || vector.iter().any(|value| !value.is_finite()) {
            return Err(EmbeddingError::InvalidOutput(
                "wrong dimension or non-finite value".into(),
            ));
        }
        let norm: f64 = vector.iter().map(|v| f64::from(*v).powi(2)).sum();
        if norm == 0.0 {
            return Err(EmbeddingError::InvalidOutput("zero vector".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
