//! Explicit local laboratory; never falls back to DATABASE_URL or production.
use pintrail_embeddings::{storage, Config, EmbeddingProvider, InputKind, LocalProvider};
use sqlx::postgres::PgPoolOptions;

#[derive(serde::Deserialize)]
struct EvaluationSeed {
    artifact: Vec<EvaluationArtifact>,
}
#[derive(serde::Deserialize)]
struct EvaluationArtifact {
    key: String,
    name: String,
}
#[derive(serde::Deserialize)]
struct EvaluationCase {
    question: String,
    expected: Vec<String>,
}

async fn evaluate(pool: &sqlx::PgPool, provider: &mut LocalProvider) -> Result<(), Box<dyn Error>> {
    let seed: EvaluationSeed = toml::from_str(include_str!("../../../seeds/umass/artifacts.toml"))?;
    let mut cases: Vec<EvaluationCase> = serde_json::from_str(include_str!("umass_queries.json"))?;
    cases.extend(serde_json::from_str::<Vec<EvaluationCase>>(include_str!(
        "semantic_cases.json"
    ))?);
    cases.extend(serde_json::from_str::<Vec<EvaluationCase>>(include_str!(
        "harder_cases.json"
    ))?);
    let mut first = 0;
    let mut three = 0;
    let mut supported = 0;
    let mut results = Vec::new();
    for case in cases {
        let mut expected = Vec::new();
        for key in &case.expected {
            let name = &seed
                .artifact
                .iter()
                .find(|a| &a.key == key)
                .ok_or("missing expected artifact")?
                .name;
            let ids: Vec<uuid::Uuid> =
                sqlx::query_scalar("SELECT id FROM artifacts WHERE name=$1 AND deleted_at IS NULL")
                    .bind(name)
                    .fetch_all(pool)
                    .await?;
            if ids.len() != 1 {
                return Err(format!("expected exactly one lab artifact named {name}").into());
            }
            expected.push(ids[0]);
        }
        let query = provider
            .embed(&[&case.question], InputKind::Query)?
            .remove(0);
        let hits = storage::search(pool, provider.model_info(), &query, 3).await?;
        let hit_first = hits
            .first()
            .is_some_and(|h| expected.contains(&h.artifact_id));
        let hit_three = hits.iter().any(|h| expected.contains(&h.artifact_id));
        if !expected.is_empty() {
            supported += 1;
            first += usize::from(hit_first);
            three += usize::from(hit_three);
        }
        results.push(serde_json::json!({"question":case.question,"expected":case.expected,
            "expected_first":hit_first,"expected_in_top_3":hit_three,"unsupported":expected.is_empty(),"matches":hits}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
        "model":provider.model_info(),"supported_queries":supported,"expected_first":first,"expected_in_top_3":three,
        "retrieval":"PostgreSQL pgvector using saved full-document embeddings; only questions were embedded during this evaluation",
        "results":results}))?
    );
    Ok(())
}
use std::error::Error;

fn similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let norm = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    dot / (norm(a) * norm(b))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::var("SEARCH_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    if database != "pintrail_search_lab" {
        return Err("This example only operates on pintrail_search_lab".into());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("init") => {
            sqlx::migrate!("../../migrations")
                .set_ignore_missing(true)
                .run(&pool)
                .await?;
            storage::migrate(&pool).await?;
            sqlx::raw_sql(include_str!("../../../seeds/umass/seed-dev.sql"))
                .execute(&pool)
                .await?;
            println!("Initialized the isolated lab and loaded 58 UMass seed artifacts. Run the text worker next.");
        }
        Some("index") => {
            let mut provider = LocalProvider::new(Config::from_env()?)?;
            let model = provider.model_info().clone();
            let documents = storage::documents(&pool).await?;
            if documents.is_empty() {
                return Err(
                    "No prepared search documents; run index-search-text-once first".into(),
                );
            }
            for batch in documents.chunks(32) {
                let texts: Vec<_> = batch.iter().map(|d| d.content.as_str()).collect();
                // No transaction or locks are held while the model works.
                let vectors = provider.embed(&texts, InputKind::Document)?;
                for (document, vector) in batch.iter().zip(vectors) {
                    storage::save(&pool, document, &model, &vector).await?;
                }
            }
            println!(
                "Saved {} document vectors; model {} revision {}, dimension {}.",
                documents.len(),
                model.name,
                model.revision.as_deref().unwrap_or("unknown"),
                model.dimension
            );
        }
        Some("evaluate") => {
            let mut provider = LocalProvider::new(Config::from_env()?)?;
            evaluate(&pool, &mut provider).await?;
        }
        Some("search") | Some("compare") => {
            let question = args.get(1..).unwrap_or_default().join(" ");
            let mut provider = LocalProvider::new(Config::from_env()?)?;
            let query = provider.embed(&[&question], InputKind::Query)?.remove(0);
            let hits = storage::search(&pool, provider.model_info(), &query, 3).await?;
            if args[0] == "compare" {
                // Same full documents, same query: compare PostgreSQL with Rust.
                let documents = storage::documents(&pool).await?;
                let mut scores = Vec::new();
                for batch in documents.chunks(32) {
                    let texts: Vec<_> = batch.iter().map(|d| d.content.as_str()).collect();
                    let vectors = provider.embed(&texts, InputKind::Document)?;
                    for (d, v) in batch.iter().zip(vectors) {
                        let published: bool =
                            sqlx::query_scalar("SELECT artifact_is_published($1)")
                                .bind(d.artifact_id)
                                .fetch_one(&pool)
                                .await?;
                        if published {
                            scores.push((d.id, similarity(&query, &v)));
                        }
                    }
                }
                scores.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                if hits.len() != scores.len().min(3)
                    || hits
                        .iter()
                        .zip(&scores)
                        .any(|(h, s)| h.document_id != s.0 || (h.similarity - s.1).abs() > 0.00001)
                {
                    return Err("PostgreSQL and in-memory cosine results differ".into());
                }
                println!("PASS: PostgreSQL and Rust return the same top three (score tolerance 0.00001).");
            }
            println!("{}", serde_json::to_string_pretty(&hits)?);
        }
        _ => {
            return Err(
                "Usage: postgres_search init | index | search <question> | compare <question>"
                    .into(),
            )
        }
    }
    Ok(())
}
