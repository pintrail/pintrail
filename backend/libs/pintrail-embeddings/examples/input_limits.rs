//! Direct token-limit probe; synthetic repetition measures capacity, not quality.
use pintrail_embeddings::{Config, EmbeddingError, EmbeddingProvider, InputKind, LocalProvider};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut provider = LocalProvider::new(Config::from_env()?)?;
    let mut rows = Vec::new();
    for words in [20, 240, 254, 255, 256, 300, 500, 510, 511, 512, 600] {
        let text = "water ".repeat(words);
        for kind in [InputKind::Query, InputKind::Document] {
            let result = match provider.embed(&[&text], kind) {
                Ok(_) => json!({"accepted":true}),
                Err(EmbeddingError::InputTooLong { tokens, limit, .. }) => {
                    json!({"accepted":false,"tokens":tokens,"limit":limit})
                }
                Err(e) => return Err(e.into()),
            };
            rows.push(json!({"repeated_words":words,"kind":format!("{kind:?}"),"result":result}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"model":provider.model_info(),"note":"Special tokens count; BGE queries also include its retrieval instruction. Word counts are not general token counts.","results":rows})
        )?
    );
    Ok(())
}
