//! Opt-in real-model smoke test, not run by `cargo test`.
use std::{collections::HashMap, error::Error, time::Instant};

use pintrail_embeddings::{Config, EmbeddingError, EmbeddingProvider, InputKind, LocalProvider};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Seed {
    artifact: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    key: String,
    name: String,
    description: String,
    parent: Option<String>,
    #[serde(default)]
    topic: bool,
}

#[derive(Deserialize)]
struct Case {
    question: String,
    expected: Vec<String>,
}

fn similarity(left: &[f32], right: &[f32]) -> f64 {
    let dot: f64 = left
        .iter()
        .zip(right)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum();
    let norm = |v: &[f32]| v.iter().map(|n| f64::from(*n).powi(2)).sum::<f64>().sqrt();
    dot / (norm(left) * norm(right))
}

fn embed_batches(
    provider: &mut impl EmbeddingProvider,
    texts: &[String],
    kind: InputKind,
) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let mut vectors = Vec::new();
    for batch in texts.chunks(32) {
        let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
        vectors.extend(provider.embed(&refs, kind)?);
    }
    Ok(vectors)
}

fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();
    let seed: Seed = toml::from_str(include_str!("../../../seeds/umass/artifacts.toml"))?;
    let cases: Vec<Case> = serde_json::from_str(include_str!("umass_queries.json"))?;
    let lookup: HashMap<&str, &Artifact> =
        seed.artifact.iter().map(|a| (a.key.as_str(), a)).collect();
    for case in &cases {
        if case.expected.is_empty()
            || case
                .expected
                .iter()
                .any(|key| !lookup.contains_key(key.as_str()))
        {
            return Err("query fixture refers to a missing expected artifact".into());
        }
    }

    let config = Config::from_env()?;
    let threads = config.threads;
    eprintln!(
        "Loading {} on CPU ({} threads); a cold cache downloads public model files.",
        config.model.info().name,
        threads
    );
    let load_start = Instant::now();
    let mut provider = LocalProvider::new(config)?;
    let load_ms = load_start.elapsed().as_millis();

    // Paragraph passages keep each source below both models' limits, retain its
    // artifact identity, and avoid silently cutting off the end of descriptions.
    // This is demo-only passage preparation, not the production chunking pipeline.
    let mut passages = Vec::new();
    let mut owners = Vec::new();
    for (index, artifact) in seed.artifact.iter().enumerate() {
        let mut ancestors = Vec::new();
        let mut parent = artifact.parent.as_deref();
        while let Some(key) = parent {
            if ancestors.len() >= seed.artifact.len() {
                return Err("cyclic seed hierarchy".into());
            }
            let ancestor = lookup.get(key).ok_or("missing seed parent")?;
            ancestors.push(ancestor.name.as_str());
            parent = ancestor.parent.as_deref();
        }
        ancestors.reverse();
        let description = artifact.description.replace("\r\n", "\n");
        for paragraph in description.split("\n\n").filter(|p| !p.trim().is_empty()) {
            passages.push(format!(
                "Name: {}\nKind: other\nParents: {}\nDescription: {}",
                artifact.name,
                ancestors.join(" > "),
                paragraph.trim()
            ));
            owners.push(index);
        }
    }
    let index_start = Instant::now();
    let documents = embed_batches(&mut provider, &passages, InputKind::Document)?;
    let document_ms = index_start.elapsed().as_millis();
    let questions: Vec<String> = cases.iter().map(|c| c.question.clone()).collect();
    let query_start = Instant::now();
    let queries = embed_batches(&mut provider, &questions, InputKind::Query)?;
    let query_ms = query_start.elapsed().as_millis();

    // Verify actual token-limit enforcement with the loaded model. A caller must
    // split a long passage instead of receiving an embedding for silently lost text.
    let oversized = "water ".repeat(600);
    if !matches!(
        provider.embed(&[&oversized], InputKind::Document),
        Err(EmbeddingError::InputTooLong { .. })
    ) {
        return Err("oversized input was not rejected".into());
    }

    let mut results = Vec::new();
    let mut top_one = 0;
    let mut top_three = 0;
    for (case, query) in cases.iter().zip(&queries) {
        let mut scores = vec![f64::NEG_INFINITY; seed.artifact.len()];
        for (vector, owner) in documents.iter().zip(&owners) {
            scores[*owner] = scores[*owner].max(similarity(query, vector));
        }
        let mut ranked: Vec<usize> = (0..seed.artifact.len()).collect();
        ranked.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]).then_with(|| a.cmp(b)));
        let found_one = case.expected.contains(&seed.artifact[ranked[0]].key);
        let found_three = ranked[..3]
            .iter()
            .any(|i| case.expected.contains(&seed.artifact[*i].key));
        top_one += usize::from(found_one);
        top_three += usize::from(found_three);
        let matches: Vec<_> = ranked[..3]
            .iter()
            .map(|i| {
                let artifact = &seed.artifact[*i];
                json!({"key": artifact.key, "name": artifact.name,
                "is_topic": artifact.topic, "similarity": scores[*i]})
            })
            .collect();
        results.push(json!({"question":case.question,"expected":case.expected,
            "expected_in_top_1":found_one,"expected_in_top_3":found_three,"matches":matches}));
    }
    let peak_rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u64>().ok())
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "model":provider.model_info(),"threads":threads,"device":"CPU",
            "artifact_count":seed.artifact.len(),"passage_count":passages.len(),
            "query_count":cases.len(),"model_load_ms":load_ms,
            "document_embedding_ms":document_ms,"query_embedding_ms":query_ms,
            "process_peak_rss_kib":peak_rss_kib,"expected_in_top_1":top_one,
            "expected_in_top_3":top_three,"oversized_input_rejected":true,"results":results
        }))?
    );
    Ok(())
}
