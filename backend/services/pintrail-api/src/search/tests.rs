use super::*;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use pintrail_embeddings::{
    storage, test_provider::DeterministicProvider, EmbeddingProvider, InputKind,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

async fn state(db: &PgPool, search: Option<SearchService>) -> AppState {
    let settings = crate::config::Settings {
        database_url: "unused".into(),
        bind_addr: "127.0.0.1:0".into(),
        db_max_connections: 5,
        db_acquire_timeout: Duration::from_secs(2),
        cookie_secure: false,
        public_base_url: "http://localhost".into(),
        mailer: "log".into(),
        trust_proxy_headers: false,
        search_enabled: search.is_some(),
        s3_endpoint: Some("http://127.0.0.1:9".into()),
        s3_public_endpoint: None,
        s3_region: "us-east-1".into(),
        s3_bucket: "unused".into(),
        s3_access_key_id: "unused".into(),
        s3_secret_access_key: "unused".into(),
        presign_ttl: Duration::from_secs(60),
        max_upload_bytes: 1024,
    };
    // Client construction is local; these tests never check or call S3.
    let storage = pintrail_storage::Storage::connect(&pintrail_storage::StorageConfig {
        endpoint: settings.s3_endpoint.clone(),
        public_endpoint: None,
        region: settings.s3_region.clone(),
        bucket: settings.s3_bucket.clone(),
        access_key_id: settings.s3_access_key_id.clone(),
        secret_access_key: settings.s3_secret_access_key.clone(),
        presign_ttl: settings.presign_ttl,
    })
    .await
    .unwrap();
    AppState {
        db: db.clone(),
        settings,
        storage,
        search,
        mailer: Arc::new(crate::mail::LogMailer),
        limiter: crate::rate_limit::RateLimiter::new(),
    }
}

async fn reader(db: &PgPool) -> (Uuid, String) {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO readers(email,password_hash) VALUES ($1,'unused') RETURNING id",
    )
    .bind(format!("{}@example.test", Uuid::new_v4()))
    .fetch_one(db)
    .await
    .unwrap();
    let token = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO reader_sessions(reader_id,token_hash,expires_at) VALUES ($1,$2,now()+interval '1 hour')")
        .bind(id).bind(crate::crypto::hash_token(&token)).execute(db).await.unwrap();
    (id, format!("Bearer {token}"))
}

async fn request(
    state: &AppState,
    uri: &str,
    authorization: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri);
    if let Some(token) = authorization {
        builder = builder.header("Authorization", token);
    }
    let response = crate::router(state.clone())
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn document(
    db: &PgPool,
    name: &str,
    status: &str,
    description: &str,
    is_topic: bool,
) -> storage::Document {
    let id: Uuid = sqlx::query_scalar("INSERT INTO artifacts(name,status,description,is_topic) VALUES ($1,$2::artifact_status,$3,$4) RETURNING id")
        .bind(name).bind(status).bind(description).bind(is_topic).fetch_one(db).await.unwrap();
    sqlx::query_as("INSERT INTO search_documents(artifact_id,content,content_hash) VALUES ($1,$2,$3) RETURNING id,artifact_id,content,content_hash")
        .bind(id).bind(description).bind("a".repeat(64)).fetch_one(db).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn route_requires_login_validates_input_and_reports_disabled_search(db: PgPool) {
    let state = state(&db, None).await;
    let (_, auth) = reader(&db).await;
    assert_eq!(
        request(&state, "/search?q=solar", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&state, "/search?q=solar", Some("Bearer invalid"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for uri in [
        "/search",
        "/search?q=%20%20",
        "/search?q=solar&limit=0",
        "/search?q=solar&limit=21",
        "/search?q=solar&limit=no",
        "/search?q=solar&mode=unknown",
    ] {
        let (status, body) = request(&state, uri, Some(&auth)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert!(body["error"].is_string());
    }
    assert_eq!(
        request(
            &state,
            &format!("/search?q={}", "x".repeat(4097)),
            Some(&auth)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&state, "/search?q=solar", Some(&auth)).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn route_filters_before_limit_and_returns_public_snippets_without_account_data(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let mut provider = DeterministicProvider::default();
    let model = provider.model_info().clone();
    let vector = provider
        .embed(&["solar"], InputKind::Query)
        .unwrap()
        .remove(0);
    let visible = document(&db, "Visible", "approved", &"🌿".repeat(300), false).await;
    let draft = document(&db, "HIDDEN DRAFT", "draft", "secret", false).await;
    let deleted = document(&db, "HIDDEN DELETED", "approved", "secret", false).await;
    let child = document(&db, "HIDDEN CHILD", "approved", "secret", false).await;
    let wrong = document(&db, "HIDDEN MODEL", "approved", "secret", false).await;
    let stale = document(&db, "HIDDEN STALE", "approved", "secret", false).await;
    let topic = document(
        &db,
        "Published topic",
        "approved",
        "Shared explanation",
        true,
    )
    .await;
    for doc in [&visible, &draft, &deleted, &child, &stale] {
        storage::save(&db, doc, &model, &vector).await.unwrap();
    }
    let mut other = model.clone();
    other.revision = Some("other-version".into());
    storage::save(&db, &wrong, &other, &vector).await.unwrap();
    storage::save(&db, &topic, &model, &[0.0, 0.0, 1.0])
        .await
        .unwrap();
    sqlx::query("UPDATE artifacts SET deleted_at=now() WHERE id=$1")
        .bind(deleted.artifact_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE artifacts SET parent_id=$1 WHERE id=$2")
        .bind(draft.artifact_id)
        .bind(child.artifact_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE search_embeddings SET source_content_hash=$1 WHERE document_id=$2")
        .bind("b".repeat(64))
        .bind(stale.id)
        .execute(&db)
        .await
        .unwrap();
    let state = state(&db, Some(SearchService::from_provider(provider).unwrap())).await;
    let (_, auth) = reader(&db).await;
    let (status, body) = request(&state, "/search?q=solar&limit=1", Some(&auth)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(
        body["results"][0]["artifact_id"],
        visible.artifact_id.to_string()
    );
    assert_eq!(
        body["results"][0]["snippet"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        280
    );
    assert!((body["results"][0]["score"].as_f64().unwrap() - 1.0).abs() < 0.00001);
    assert!(body["results"][0].get("document_id").is_none());
    let (_, body) = request(&state, "/search?q=solar&limit=20", Some(&auth)).await;
    assert_eq!(body["count"], 2);
    assert_eq!(body["results"][1]["is_topic"], true);
    assert!(!body.to_string().contains("HIDDEN"));
    assert!(!body.to_string().contains("example.test"));
    sqlx::query("UPDATE artifacts SET status='draft' WHERE id=$1")
        .bind(visible.artifact_id)
        .execute(&db)
        .await
        .unwrap();
    let (_, body) = request(&state, "/search?q=solar", Some(&auth)).await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["results"][0]["name"], "Published topic");
}

#[sqlx::test(migrations = "../../migrations")]
async fn route_accepts_author_cookie_but_still_hides_drafts(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let state = state(
        &db,
        Some(SearchService::from_provider(DeterministicProvider::default()).unwrap()),
    )
    .await;
    let author: Uuid = sqlx::query_scalar("INSERT INTO authors(email,password_hash,role) VALUES ('author@example.test','unused','admin') RETURNING id")
        .fetch_one(&db).await.unwrap();
    sqlx::query("UPDATE authors SET must_change_password=false WHERE id=$1")
        .bind(author)
        .execute(&db)
        .await
        .unwrap();
    let token = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO author_sessions(author_id,token_hash,expires_at) VALUES ($1,$2,now()+interval '1 hour')")
        .bind(author).bind(crate::crypto::hash_token(&token)).execute(&db).await.unwrap();
    let draft = document(&db, "Hidden draft", "draft", "secret", false).await;
    let mut p = DeterministicProvider::default();
    let vector = p.embed(&["solar"], InputKind::Query).unwrap().remove(0);
    storage::save(&db, &draft, p.model_info(), &vector)
        .await
        .unwrap();
    let response = crate::router(state)
        .oneshot(
            Request::builder()
                .uri("/search?q=solar")
                .header(
                    "Cookie",
                    format!("{}={token}", crate::authors::auth::SESSION_COOKIE),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 10000).await.unwrap()).unwrap();
    assert_eq!(body["count"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn route_rate_limit_is_per_account(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let state = state(
        &db,
        Some(SearchService::from_provider(DeterministicProvider::default()).unwrap()),
    )
    .await;
    let (id, auth) = reader(&db).await;
    for _ in 0..60 {
        state
            .limiter
            .check(&format!("search:Reader:{id}"), SEARCH_PER_ACCOUNT)
            .unwrap();
    }
    assert_eq!(
        request(&state, "/search?q=solar", Some(&auth)).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let (_, other) = reader(&db).await;
    assert_eq!(
        request(&state, "/search?q=solar", Some(&other)).await.0,
        StatusCode::OK
    );
}

struct ErrorProvider {
    inner: DeterministicProvider,
    too_long: bool,
}
impl EmbeddingProvider for ErrorProvider {
    fn model_info(&self) -> &pintrail_embeddings::ModelInfo {
        self.inner.model_info()
    }
    fn embed(
        &mut self,
        _: &[&str],
        _: InputKind,
    ) -> Result<Vec<Vec<f32>>, pintrail_embeddings::EmbeddingError> {
        if self.too_long {
            Err(pintrail_embeddings::EmbeddingError::InputTooLong {
                index: 0,
                tokens: 513,
                limit: 512,
            })
        } else {
            Err(pintrail_embeddings::EmbeddingError::Inference(
                "PRIVATE MODEL FAILURE".into(),
            ))
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn route_maps_input_errors_and_redacts_internal_failures(db: PgPool) {
    let (_, auth) = reader(&db).await;
    for (too_long, expected) in [
        (true, StatusCode::BAD_REQUEST),
        (false, StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let provider = ErrorProvider {
            inner: DeterministicProvider::default(),
            too_long,
        };
        let state = state(&db, Some(SearchService::from_provider(provider).unwrap())).await;
        let (status, body) = request(&state, "/search?q=solar", Some(&auth)).await;
        assert_eq!(status, expected);
        assert!(!body.to_string().contains("PRIVATE"));
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn keyword_mode_skips_inference_and_model_token_limit(db: PgPool) {
    // This provider fails every inference call; keyword requests must still work.
    let provider = ErrorProvider {
        inner: DeterministicProvider::default(),
        too_long: false,
    };
    let state = state(&db, Some(SearchService::from_provider(provider).unwrap())).await;
    let (_, auth) = reader(&db).await;
    document(
        &db,
        "Rare Name",
        "approved",
        "A rare scientific name",
        false,
    )
    .await;
    let (status, body) = request(&state, "/search?q=Rare%20Name&mode=keyword", Some(&auth)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"][0]["name"], "Rare Name");
    assert_eq!(body["mode"], "keyword");
    assert!(body["model"].is_null());
    assert_eq!(body["score_kind"], "full_text_rank");
    assert!(body["results"][0].get("semantic_score").is_none());
    assert_eq!(
        request(
            &state,
            &format!("/search?mode=keyword&q={}", "water+".repeat(600)),
            Some(&auth)
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn mode_selection_and_hybrid_exact_names_are_exposed_over_http(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let mut provider = DeterministicProvider::default();
    let vector = provider
        .embed(&["Rare Name"], InputKind::Query)
        .unwrap()
        .remove(0);
    let exact = document(
        &db,
        "Rare Name",
        "approved",
        "A scientific laboratory",
        false,
    )
    .await;
    let meaning = document(&db, "Meaning result", "approved", "Other research", false).await;
    storage::save(&db, &meaning, provider.model_info(), &vector)
        .await
        .unwrap();
    let state = state(&db, Some(SearchService::from_provider(provider).unwrap())).await;
    let (_, auth) = reader(&db).await;
    for (mode, expected) in [
        ("keyword", exact.artifact_id),
        ("semantic", meaning.artifact_id),
        ("hybrid", exact.artifact_id),
    ] {
        let (status, body) = request(
            &state,
            &format!("/search?q=Rare%20Name&limit=1&mode={mode}"),
            Some(&auth),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{mode}: {body}");
        assert_eq!(body["mode"], mode);
        assert_eq!(body["results"][0]["artifact_id"], expected.to_string());
        if mode == "hybrid" {
            assert_eq!(body["score_kind"], "semantic_first_fusion");
        }
    }
}
