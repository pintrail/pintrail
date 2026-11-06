//! Authenticated keyword, semantic and hybrid retrieval of approved content.
mod service;
pub use service::SearchService;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::identity::Identity;
use crate::rate_limit::SEARCH_PER_ACCOUNT;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/search", get(search))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    q: String,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    mode: SearchMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Keyword,
    #[default]
    Semantic,
    Hybrid,
}

impl SearchMode {
    #[cfg(feature = "semantic-search")]
    fn score_kind(self) -> &'static str {
        match self {
            Self::Keyword => "full_text_rank",
            Self::Semantic => "cosine_similarity",
            Self::Hybrid => "semantic_first_fusion",
        }
    }
}

fn default_limit() -> u32 {
    10
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub artifact_id: Uuid,
    pub name: String,
    pub is_topic: bool,
    pub snippet: String,
    /// Mode-specific ranking value; never a confidence probability.
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword_score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub mode: SearchMode,
    pub model: Option<String>,
    pub score_kind: &'static str,
    pub count: usize,
    pub results: Vec<SearchResult>,
}

async fn search(
    identity: Identity,
    State(state): State<AppState>,
    params: Result<Query<SearchParams>, QueryRejection>,
) -> AppResult<Json<SearchResponse>> {
    let Query(params) = params.map_err(|_| {
        AppError::BadRequest(
            "provide q, optional limit from 1 to 20, and mode keyword, semantic or hybrid".into(),
        )
    })?;
    let query = params.q.trim();
    if query.is_empty() || query.len() > 4096 {
        return Err(AppError::BadRequest(
            "q must contain text and be at most 4096 UTF-8 bytes".into(),
        ));
    }
    if !(1..=20).contains(&params.limit) {
        return Err(AppError::BadRequest("limit must be from 1 to 20".into()));
    }
    let service = state
        .search
        .as_ref()
        .ok_or(AppError::ServiceUnavailable("search is not enabled"))?;
    state.limiter.check(
        &format!("search:{:?}:{}", identity.owner_type(), identity.id()),
        SEARCH_PER_ACCOUNT,
    )?;
    Ok(Json(
        service
            .query(
                &state.db,
                query.to_owned(),
                i64::from(params.limit),
                params.mode,
            )
            .await?,
    ))
}

#[cfg(all(test, feature = "semantic-search"))]
mod tests;
