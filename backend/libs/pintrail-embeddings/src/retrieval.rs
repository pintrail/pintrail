//! PostgreSQL full-text candidates and conservative semantic-first rank fusion.
//! No model is used for keyword-only retrieval; fusion does not run inference.
use std::collections::HashMap;

use sqlx::PgConnection;
use uuid::Uuid;

use crate::{storage, ModelInfo};

const CANDIDATE_LIMIT: i64 = 50;
const RRF_OFFSET: f64 = 60.0;
const MAX_LOOKUP_WORDS: usize = 3;
const MAX_SELECTIVE_MATCHES: usize = 3;
pub const HYBRID_RANKING_VERSION: &str = "semantic-first-v1";

#[derive(Debug)]
pub struct RetrievedArtifact {
    pub artifact_id: Uuid,
    pub name: String,
    pub is_topic: bool,
    pub snippet: String,
    pub score: f64,
    pub semantic_score: Option<f64>,
    pub keyword_score: Option<f64>,
    exact_name: bool,
}

#[derive(sqlx::FromRow)]
struct KeywordHit {
    artifact_id: Uuid,
    name: String,
    is_topic: bool,
    snippet: String,
    keyword_score: f64,
    exact_name: bool,
}

pub fn from_semantic(hits: Vec<storage::Hit>) -> Vec<RetrievedArtifact> {
    hits.into_iter()
        .map(|hit| RetrievedArtifact {
            artifact_id: hit.artifact_id,
            name: hit.name,
            is_topic: hit.is_topic,
            snippet: hit.snippet,
            score: hit.similarity,
            semantic_score: Some(hit.similarity),
            keyword_score: None,
            exact_name: false,
        })
        .collect()
}

fn normalized_name(text: &str) -> String {
    let trimmed = text.trim();
    let text = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed);
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn has_constraints(text: &str) -> bool {
    text.contains('"')
        || text
            .split_whitespace()
            .any(|word| word.starts_with('-') && word.len() > 1)
}

fn allow_keyword_boost(text: &str, matching_artifacts: usize, semantic_empty: bool) -> bool {
    if semantic_empty || has_constraints(text) {
        return true;
    }
    let words: Vec<_> = text.split_whitespace().collect();
    let first = words
        .first()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .unwrap_or_default();
    let question_start = matches!(
        first.as_str(),
        "what"
            | "where"
            | "which"
            | "who"
            | "when"
            | "why"
            | "how"
            | "can"
            | "could"
            | "would"
            | "should"
            | "do"
            | "does"
            | "did"
            | "is"
            | "are"
            | "was"
            | "were"
            | "i"
            | "we"
    );
    // Selectivity counts visible artifacts, not private rows or calibrated
    // confidence. These limits are provisional English query heuristics.
    !question_start
        && !words.is_empty()
        && words.len() <= MAX_LOOKUP_WORDS
        && (1..=MAX_SELECTIVE_MATCHES).contains(&matching_artifacts)
}

async fn keywords(
    connection: &mut PgConnection,
    text: &str,
    limit: i64,
) -> Result<Vec<KeywordHit>, storage::StorageError> {
    Ok(sqlx::query_as(
        r#"
        WITH input AS (
            SELECT websearch_to_tsquery('english', $1) AS query
        ), candidate_documents AS (
            -- Keep the GIN-searchable condition separate from the joined
            -- name fallback; a cross-table OR would prevent that index path.
            SELECT d.id FROM search_documents d CROSS JOIN input i
            WHERE d.search_terms @@ i.query
            UNION
            SELECT d.id FROM search_documents d JOIN artifacts a ON a.id=d.artifact_id
            WHERE $4 AND lower(regexp_replace(btrim(a.name),'[[:space:]]+',' ','g'))=$2
        ), documents AS (
            SELECT d.artifact_id, d.id AS document_id, a.name, a.is_topic,
                   left(a.description,280) AS snippet,
                   ($4 AND lower(regexp_replace(btrim(a.name),'[[:space:]]+',' ','g'))=$2) AS exact_name,
                   (ts_rank_cd(d.search_terms,i.query,32)
                    + 2 * ts_rank_cd(to_tsvector('english',a.name),i.query,32))::float8 AS keyword_score
            FROM candidate_documents c JOIN search_documents d ON d.id=c.id
            JOIN artifacts a ON a.id=d.artifact_id CROSS JOIN input i
            WHERE artifact_is_published(a.id)
        ), ranked_artifacts AS (
            SELECT DISTINCT ON (artifact_id) artifact_id, name, is_topic, snippet, keyword_score, exact_name
            FROM documents ORDER BY artifact_id, exact_name DESC, keyword_score DESC, document_id
        )
        SELECT * FROM ranked_artifacts ORDER BY exact_name DESC, keyword_score DESC, artifact_id LIMIT $3
        "#,
    ).bind(text).bind(normalized_name(text)).bind(limit.clamp(1,100))
        .bind(!text.split_whitespace().any(|word|word.starts_with('-')) && !text.contains(" OR "))
        .fetch_all(connection).await?)
}

pub async fn keyword_on(
    connection: &mut PgConnection,
    text: &str,
    limit: i64,
) -> Result<Vec<RetrievedArtifact>, storage::StorageError> {
    Ok(keywords(connection, text, limit)
        .await?
        .into_iter()
        .map(|hit| RetrievedArtifact {
            artifact_id: hit.artifact_id,
            name: hit.name,
            is_topic: hit.is_topic,
            snippet: hit.snippet,
            score: hit.keyword_score,
            semantic_score: None,
            keyword_score: Some(hit.keyword_score),
            exact_name: hit.exact_name,
        })
        .collect())
}

pub async fn hybrid_on(
    connection: &mut PgConnection,
    model: &ModelInfo,
    vector: &[f32],
    text: &str,
    limit: i64,
) -> Result<Vec<RetrievedArtifact>, storage::StorageError> {
    let constraint = has_constraints(text).then_some(text);
    let semantic =
        storage::search_constrained_on(connection, model, vector, CANDIDATE_LIMIT, constraint)
            .await?;
    let lexical = keywords(connection, text, CANDIDATE_LIMIT).await?;
    let boost = allow_keyword_boost(text, lexical.len(), semantic.is_empty());
    Ok(fuse(semantic, lexical, limit.clamp(1, 100) as usize, boost))
}

fn fuse(
    semantic: Vec<storage::Hit>,
    lexical: Vec<KeywordHit>,
    limit: usize,
    boost: bool,
) -> Vec<RetrievedArtifact> {
    let mut tail_rank = semantic.len();
    let mut results: HashMap<Uuid, RetrievedArtifact> = HashMap::new();
    for (rank, hit) in semantic.into_iter().enumerate() {
        // Start with one result per artifact in semantic order. Lexical
        // agreement contributes only when the query policy permits a boost.
        results.insert(
            hit.artifact_id,
            RetrievedArtifact {
                artifact_id: hit.artifact_id,
                name: hit.name,
                is_topic: hit.is_topic,
                snippet: hit.snippet,
                score: 1.0 / (RRF_OFFSET + (rank + 1) as f64),
                semantic_score: Some(hit.similarity),
                keyword_score: None,
                exact_name: false,
            },
        );
    }
    for (rank, hit) in lexical.into_iter().enumerate() {
        let score = 1.0 / (RRF_OFFSET + (rank + 1) as f64);
        if let Some(existing) = results.get_mut(&hit.artifact_id) {
            if boost {
                existing.score += score;
            }
            existing.keyword_score = Some(hit.keyword_score);
            existing.exact_name = hit.exact_name;
        } else {
            tail_rank += 1;
            results.insert(
                hit.artifact_id,
                RetrievedArtifact {
                    artifact_id: hit.artifact_id,
                    name: hit.name,
                    is_topic: hit.is_topic,
                    snippet: hit.snippet,
                    // Weak lexical-only hits follow the semantic list, in
                    // lexical order. Exact names still sort first below.
                    score: if boost {
                        score
                    } else {
                        1.0 / (RRF_OFFSET + tail_rank as f64)
                    },
                    semantic_score: None,
                    keyword_score: Some(hit.keyword_score),
                    exact_name: hit.exact_name,
                },
            );
        }
    }
    let mut results: Vec<_> = results.into_values().collect();
    results.sort_by(|a, b| {
        b.exact_name
            .cmp(&a.exact_name)
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.artifact_id.cmp(&b.artifact_id))
    });
    results.truncate(limit);
    results
}
