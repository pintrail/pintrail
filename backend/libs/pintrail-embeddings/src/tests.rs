use super::*;
use std::{path::PathBuf, str::FromStr};
use test_provider::DeterministicProvider;

#[test]
fn test_provider_is_repeatable_and_preserves_input_order() {
    let mut provider = DeterministicProvider::default();
    let texts = ["solar", "collects stormwater runoff"];
    let batch = provider.embed(&texts, InputKind::Document).unwrap();
    assert_eq!(batch, provider.embed(&texts, InputKind::Document).unwrap());
    for (index, text) in texts.iter().enumerate() {
        assert_eq!(
            batch[index],
            provider.embed(&[*text], InputKind::Document).unwrap()[0]
        );
    }
    validate_vectors(&batch, 2, provider.model_info().dimension).unwrap();
}

#[test]
fn invalid_inputs_fail_without_loading_a_model() {
    let mut provider = DeterministicProvider::default();
    assert!(matches!(
        provider.embed(&["valid", "  "], InputKind::Document),
        Err(EmbeddingError::EmptyInput { index: 1 })
    ));
    assert!(matches!(
        provider.embed(&vec!["text"; MAX_BATCH_SIZE + 1], InputKind::Query),
        Err(EmbeddingError::BatchTooLarge { .. })
    ));
    assert!(provider.embed(&[], InputKind::Query).unwrap().is_empty());
}

#[test]
fn model_identity_includes_export_dimension_and_pipeline() {
    let bge = Model::BgeSmall.info();
    let mini = Model::MiniLm.info();
    assert_eq!((bge.dimension, mini.dimension), (384, 384));
    assert_ne!(
        bge.name, mini.name,
        "equal dimensions do not make models compatible"
    );
    assert!(!bge.repository.is_empty());
    assert!(!bge.pipeline_version.is_empty());
    assert_eq!((bge.max_tokens, mini.max_tokens), (512, 256));
    assert!(Model::from_str("not-a-model").is_err());
}

#[test]
fn bad_configuration_is_rejected_before_any_model_download() {
    let config = Config {
        model: Model::BgeSmall,
        cache_dir: PathBuf::from("unused"),
        threads: 0,
    };
    assert!(matches!(
        LocalProvider::new(config),
        Err(EmbeddingError::Configuration(_))
    ));
    let config = Config {
        model: Model::BgeSmall,
        cache_dir: PathBuf::new(),
        threads: 2,
    };
    assert!(config.validate().is_err());
}

#[test]
fn output_validation_rejects_bad_counts_dimensions_and_values() {
    for vectors in [
        vec![],
        vec![vec![1.0]],
        vec![vec![f32::NAN, 1.0]],
        vec![vec![f32::INFINITY, 1.0]],
        vec![vec![0.0, 0.0]],
    ] {
        assert!(validate_vectors(&vectors, 1, 2).is_err());
    }
    validate_vectors(&[vec![0.6, 0.8]], 1, 2).unwrap();
}
