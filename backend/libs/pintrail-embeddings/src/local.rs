use std::{path::PathBuf, str::FromStr};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use crate::{
    validate_inputs, validate_vectors, EmbeddingError, EmbeddingProvider, InputKind, ModelInfo,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    #[default]
    BgeSmall,
    MiniLm,
}

impl FromStr for Model {
    type Err = EmbeddingError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bge-small-en-v1.5" => Ok(Self::BgeSmall),
            "all-MiniLM-L6-v2" => Ok(Self::MiniLm),
            _ => Err(EmbeddingError::Configuration(
                "model must be bge-small-en-v1.5 or all-MiniLM-L6-v2".into(),
            )),
        }
    }
}

impl Model {
    fn engine_model(self) -> EmbeddingModel {
        match self {
            Self::BgeSmall => EmbeddingModel::BGESmallENV15,
            Self::MiniLm => EmbeddingModel::AllMiniLML6V2,
        }
    }

    pub fn info(self) -> ModelInfo {
        let (name, max_tokens) = match self {
            Self::BgeSmall => ("BAAI/bge-small-en-v1.5", 512),
            Self::MiniLm => ("sentence-transformers/all-MiniLM-L6-v2", 256),
        };
        let engine_model = self.engine_model();
        let engine = TextEmbedding::get_model_info(&engine_model)
            .expect("built-in models have FastEmbed metadata");
        ModelInfo {
            name: name.into(),
            repository: engine.model_code.clone(),
            revision: None,
            dimension: engine.dim,
            max_tokens,
            pipeline_version: "fastembed-7.1.5/fp32/retrieval-v1".into(),
        }
    }

    fn prepare(self, text: &str, kind: InputKind) -> String {
        match (self, kind) {
            (Self::BgeSmall, InputKind::Query) => {
                format!("Represent this sentence for searching relevant passages: {text}")
            }
            _ => text.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub model: Model,
    pub cache_dir: PathBuf,
    pub threads: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, EmbeddingError> {
        let model = std::env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "bge-small-en-v1.5".into())
            .parse()?;
        let cache_dir = std::env::var("EMBEDDING_CACHE_DIR")
            .unwrap_or_else(|_| ".cache/pintrail-embeddings".into())
            .into();
        let threads = std::env::var("EMBEDDING_THREADS")
            .unwrap_or_else(|_| "2".into())
            .parse()
            .map_err(|_| {
                EmbeddingError::Configuration("EMBEDDING_THREADS must be a number".into())
            })?;
        let config = Self {
            model,
            cache_dir,
            threads,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), EmbeddingError> {
        if !(1..=32).contains(&self.threads) || self.cache_dir.as_os_str().is_empty() {
            return Err(EmbeddingError::Configuration(
                "threads must be 1..=32 and cache_dir must not be empty".into(),
            ));
        }
        Ok(())
    }
}

pub struct LocalProvider {
    engine: TextEmbedding,
    model: Model,
    info: ModelInfo,
}

impl LocalProvider {
    /// A cold cache downloads public model files. Text is embedded locally;
    /// no artifact text is sent to a model API. The cache must be persistent.
    pub fn new(config: Config) -> Result<Self, EmbeddingError> {
        config.validate()?;
        let mut info = config.model.info();
        let cache = hf_hub::Cache::new(config.cache_dir.clone());
        let options = TextInitOptions::new(config.model.engine_model())
            .with_cache_dir(config.cache_dir)
            .with_max_length(info.max_tokens)
            .with_intra_threads(config.threads)
            .with_show_download_progress(false);
        let mut engine = TextEmbedding::try_new(options)
            .map_err(|error| EmbeddingError::Initialization(error.to_string()))?;
        let engine_model = config.model.engine_model();
        let metadata = TextEmbedding::get_model_info(&engine_model)
            .map_err(|error| EmbeddingError::Initialization(error.to_string()))?;
        let path = cache
            .model(info.repository.clone())
            .get(&metadata.model_file)
            .ok_or_else(|| {
                EmbeddingError::Initialization("loaded model snapshot is missing".into())
            })?;
        let components: Vec<_> = path.components().collect();
        info.revision = components
            .windows(2)
            .find(|pair| pair[0].as_os_str() == "snapshots")
            .map(|pair| pair[1].as_os_str().to_string_lossy().into_owned());
        if info.revision.is_none() {
            return Err(EmbeddingError::Initialization(
                "cannot identify loaded model revision".into(),
            ));
        }
        // Reject oversized text explicitly rather than silently truncating it.
        engine
            .tokenizer
            .with_truncation(None)
            .map_err(|error| EmbeddingError::Initialization(error.to_string()))?;
        Ok(Self {
            engine,
            model: config.model,
            info,
        })
    }
}

impl EmbeddingProvider for LocalProvider {
    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn embed(&mut self, texts: &[&str], kind: InputKind) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        validate_inputs(texts)?;
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prepared: Vec<String> = texts
            .iter()
            .map(|text| self.model.prepare(text, kind))
            .collect();
        for (index, text) in prepared.iter().enumerate() {
            let tokens = self
                .engine
                .tokenizer
                .encode(text.as_str(), true)
                .map_err(|error| EmbeddingError::Inference(error.to_string()))?
                .len();
            if tokens > self.info.max_tokens {
                return Err(EmbeddingError::InputTooLong {
                    index,
                    tokens,
                    limit: self.info.max_tokens,
                });
            }
        }
        let vectors = self
            .engine
            .embed(prepared, Some(8))
            .map_err(|error| EmbeddingError::Inference(error.to_string()))?;
        validate_vectors(&vectors, texts.len(), self.info.dimension)?;
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_instruction_only_applies_to_bge_queries() {
        let text = "Where does runoff soak into soil?";
        assert_eq!(Model::BgeSmall.prepare(text, InputKind::Document), text);
        assert!(Model::BgeSmall
            .prepare(text, InputKind::Query)
            .starts_with("Represent this sentence"));
        assert_eq!(Model::MiniLm.prepare(text, InputKind::Query), text);
    }
}
