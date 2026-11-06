#![cfg(feature = "postgres")]
use pintrail_embeddings::{retrieval, storage, ModelInfo};
use sqlx::PgPool;
use uuid::Uuid;

fn test_model() -> ModelInfo {
    ModelInfo {
        name: "test/model".into(),
        repository: "none".into(),
        revision: Some("v1".into()),
        dimension: 3,
        max_tokens: 512,
        pipeline_version: "test-v1".into(),
    }
}

async fn document(db: &PgPool, name: &str, body: &str, status: &str) -> storage::Document {
    let id:Uuid=sqlx::query_scalar("INSERT INTO artifacts(name,description,status) VALUES ($1,$2,$3::artifact_status) RETURNING id")
        .bind(name).bind(body).bind(status).fetch_one(db).await.unwrap();
    sqlx::query_as("INSERT INTO search_documents(artifact_id,content,content_hash) VALUES ($1,$2,$3) RETURNING id,artifact_id,content,content_hash")
        .bind(id).bind(format!("Name: {name}\nDescription: {body}")).bind("a".repeat(64)).fetch_one(db).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn keywords_rank_exact_names_and_follow_text_updates_without_vectors(db: PgPool) {
    let exact = document(
        &db,
        "Durfee Conservatory",
        "Warm tropical plants",
        "approved",
    )
    .await;
    document(
        &db,
        "Other greenhouse",
        "Durfee Conservatory Durfee Conservatory research",
        "approved",
    )
    .await;
    document(&db, "Hidden", "Durfee Conservatory", "draft").await;
    let mut connection = db.acquire().await.unwrap();
    let hits = retrieval::keyword_on(&mut connection, "  DURFEE   CONSERVATORY  ", 1)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].artifact_id, exact.artifact_id);
    assert!(hits[0].semantic_score.is_none());
    // The stored generated tsvector changes with prepared text; no worker or
    // model is required to recalculate the lexical representation itself.
    sqlx::query("UPDATE search_documents SET content='Name: Durfee Conservatory\nDescription: zyzzyva cactus',content_hash=$1 WHERE id=$2")
        .bind("b".repeat(64)).bind(exact.id).execute(&mut *connection).await.unwrap();
    assert_eq!(
        retrieval::keyword_on(&mut connection, "zyzzyva", 3)
            .await
            .unwrap()[0]
            .artifact_id,
        exact.artifact_id
    );
    assert!(retrieval::keyword_on(&mut connection, "tropical", 3)
        .await
        .unwrap()
        .is_empty());
    assert!(retrieval::keyword_on(&mut connection, "the and", 3)
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn phrases_and_exclusions_constrain_hybrid_before_candidate_limit(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let phrase = document(
        &db,
        "Ground array",
        "solar panels beside a meadow",
        "approved",
    )
    .await;
    let separate = document(
        &db,
        "Different order",
        "panels beside a solar installation",
        "approved",
    )
    .await;
    let excluded = document(
        &db,
        "Parking array",
        "solar panels over parking",
        "approved",
    )
    .await;
    for doc in [&phrase, &separate, &excluded] {
        storage::save(&db, doc, &model, &[1.0, 0.0, 0.0])
            .await
            .unwrap();
    }
    let mut connection = db.acquire().await.unwrap();
    let text = "\"solar panels\" -parking";
    for hit in retrieval::keyword_on(&mut connection, text, 20)
        .await
        .unwrap()
    {
        assert_eq!(hit.artifact_id, phrase.artifact_id);
    }
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], text, 1)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].artifact_id, phrase.artifact_id);
    let hits = retrieval::keyword_on(&mut connection, "solar OR meadow", 20)
        .await
        .unwrap();
    assert_eq!(hits.len(), 3);
    // Punctuation is parsed as data, never interpolated as SQL/query syntax.
    assert!(
        retrieval::keyword_on(&mut connection, "' ; DROP TABLE artifacts; --", 20)
            .await
            .is_ok()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn hybrid_deduplicates_and_filters_private_and_stale_vector_sources(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let visible = document(&db, "Rareterm station", "Rareterm solar", "approved").await;
    let draft = document(&db, "Hidden draft", "Rareterm solar", "draft").await;
    let child = document(&db, "Hidden child", "Rareterm solar", "approved").await;
    let deleted = document(&db, "Hidden deleted", "Rareterm solar", "approved").await;
    let vector_only = document(&db, "Meaning result", "Different vocabulary", "approved").await;
    for doc in [&visible, &draft, &child, &deleted] {
        storage::save(&db, doc, &model, &[1.0, 0.0, 0.0])
            .await
            .unwrap();
    }
    storage::save(&db, &vector_only, &model, &[0.0, 1.0, 0.0])
        .await
        .unwrap();
    sqlx::query("UPDATE artifacts SET parent_id=$1 WHERE id=$2")
        .bind(draft.artifact_id)
        .bind(child.artifact_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE artifacts SET deleted_at=now() WHERE id=$1")
        .bind(deleted.artifact_id)
        .execute(&db)
        .await
        .unwrap();
    let mut connection = db.acquire().await.unwrap();
    let lexical = retrieval::keyword_on(&mut connection, "rareterm", 1)
        .await
        .unwrap();
    assert_eq!(lexical[0].artifact_id, visible.artifact_id);
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], "rareterm", 20)
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].artifact_id, visible.artifact_id);
    assert!(hits[0].semantic_score.is_some() && hits[0].keyword_score.is_some());
    assert!((hits[0].score - (1.0 / 61.0 + 1.0 / 61.0)).abs() < 1e-9);
    // Simulate an inconsistent stored vector. Lexical matching remains valid
    // independently, but this vector must not contribute to hybrid ranking.
    sqlx::query("UPDATE search_embeddings SET source_content_hash=$1 WHERE document_id=$2")
        .bind("b".repeat(64))
        .bind(visible.id)
        .execute(&mut *connection)
        .await
        .unwrap();
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], "rareterm", 20)
        .await
        .unwrap();
    let visible = hits
        .iter()
        .find(|hit| hit.artifact_id == visible.artifact_id)
        .unwrap();
    assert!(visible.semantic_score.is_none() && visible.keyword_score.is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn exact_names_beat_semantic_distractors_and_keyword_only_artifacts_are_retrievable(
    db: PgPool,
) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let exact = document(&db, "Zyzzyva Lab", "A rare named laboratory", "approved").await;
    let distractor = document(&db, "Other laboratory", "Unrelated research", "approved").await;
    storage::save(&db, &distractor, &model, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    let mut connection = db.acquire().await.unwrap();
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], "Zyzzyva Lab", 1)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, exact.artifact_id);
    assert!(hits[0].semantic_score.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn questions_keep_semantic_order_despite_weak_shared_keyword_votes(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let specific = document(
        &db,
        "Garage charger",
        "A station in a particular building",
        "approved",
    )
    .await;
    let topic = document(
        &db,
        "General charging topic",
        "charge electric car campus center parking garage",
        "approved",
    )
    .await;
    storage::save(&db, &specific, &model, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    storage::save(&db, &topic, &model, &[0.8, 0.6, 0.0])
        .await
        .unwrap();
    let mut connection = db.acquire().await.unwrap();
    let query = "Where can I charge an electric car in the Campus Center Parking Garage?";
    let keywords = retrieval::keyword_on(&mut connection, query, 20)
        .await
        .unwrap();
    assert_eq!(keywords.len(), 1);
    assert_eq!(keywords[0].artifact_id, topic.artifact_id);
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], query, 20)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, specific.artifact_id);
    assert_eq!(hits[1].artifact_id, topic.artifact_id);
    assert!(
        hits[1].keyword_score.is_some(),
        "keyword match is visible without overriding semantic order"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn conversational_negation_is_not_treated_as_a_keyword_request(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let wood = document(
        &db,
        "Wood teaching building",
        "Architecture with laminated timber",
        "approved",
    )
    .await;
    let unrelated = document(&db, "Solar distractor", "solar panels", "approved").await;
    storage::save(&db, &wood, &model, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    storage::save(&db, &unrelated, &model, &[0.8, 0.6, 0.0])
        .await
        .unwrap();
    let query = "I want a teaching building made from layers of wood, not gardens or solar panels";
    let mut connection = db.acquire().await.unwrap();
    let keywords = retrieval::keyword_on(&mut connection, query, 20)
        .await
        .unwrap();
    assert_eq!(keywords[0].artifact_id, unrelated.artifact_id);
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], query, 1)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, wood.artifact_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn selective_lookups_get_support_but_broad_terms_do_not(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let meaning = document(&db, "Meaning result", "different language", "approved").await;
    let term = document(&db, "Rare lookup", "zyzzyva solar", "approved").await;
    storage::save(&db, &meaning, &model, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    storage::save(&db, &term, &model, &[0.8, 0.6, 0.0])
        .await
        .unwrap();
    let mut connection = db.acquire().await.unwrap();
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], "zyzzyva", 1)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, term.artifact_id);
    // The same term now has broad support across four approved artifacts.
    for n in 0..3 {
        document(
            &db,
            &format!("Solar place {n}"),
            "solar equipment",
            "approved",
        )
        .await;
    }
    let hits = retrieval::hybrid_on(&mut connection, &model, &[1.0, 0.0, 0.0], "solar", 20)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, meaning.artifact_id);
    assert_eq!(hits[1].artifact_id, term.artifact_id);
    // Short questions are also protected, despite a selective word match.
    let hits = retrieval::hybrid_on(
        &mut connection,
        &model,
        &[1.0, 0.0, 0.0],
        "Where is zyzzyva?",
        1,
    )
    .await
    .unwrap();
    assert_eq!(hits[0].artifact_id, meaning.artifact_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn lexical_fallback_and_long_exact_names_still_work(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let model = test_model();
    let exact = document(
        &db,
        "The Long Name Of This Laboratory",
        "unusual science instrumentation",
        "approved",
    )
    .await;
    let semantic = document(&db, "Different lab", "other science", "approved").await;
    let mut connection = db.acquire().await.unwrap();
    let hits = retrieval::hybrid_on(
        &mut connection,
        &model,
        &[1.0, 0.0, 0.0],
        "unusual science instrumentation",
        1,
    )
    .await
    .unwrap();
    assert_eq!(
        hits[0].artifact_id, exact.artifact_id,
        "keyword fallback works with no vectors"
    );
    storage::save(&db, &semantic, &model, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    let hits = retrieval::hybrid_on(
        &mut connection,
        &model,
        &[1.0, 0.0, 0.0],
        "The Long Name Of This Laboratory",
        1,
    )
    .await
    .unwrap();
    assert_eq!(hits[0].artifact_id, exact.artifact_id);
}
