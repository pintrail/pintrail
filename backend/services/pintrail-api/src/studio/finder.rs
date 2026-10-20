//! The artifact finder: browse artifacts one level at a time.
//!
//! Replaces the Studio's old always-expanded tree. The sidebar shows the top
//! level (places, plus a Topics folder); clicking an artifact that holds
//! others opens it and steps inside, with a breadcrumb back out. Typing in
//! the search box searches every level by name and tag, and the filter chips
//! (Mine, Drafts, In review, Topics) list matches from every level, each
//! with its path.
//!
//! Each request returns one level, not the whole collection, so it stays
//! fast as the collection grows. The fragment re-renders itself in place:
//! its own URL is on it, and it reloads on `refresh-tree` (fired after a
//! create, edit or delete) at whatever level the author was looking at.
//!
//! `mode=pick` renders the same widget for choosing an artifact rather than
//! opening it: a click fires an `artifact-picked` event on the finder with
//! the artifact's id and name (see studio.js). That is how other views reuse
//! it (choosing a parent, adding a trail stop, linking a topic).

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use minijinja::{context, Value};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::routes::{base_ctx, environment, render, session_token};
use crate::authors::extractors::AuthenticatedAuthor;
use crate::error::AppResult;
use crate::state::AppState;

/// Search and filter results are capped; the hint says when there are more.
const LIST_LIMIT: i64 = 100;

pub fn router() -> Router<AppState> {
    Router::new().route("/studio/finder", get(finder))
}

#[derive(Debug, Default, Deserialize)]
pub struct FinderQuery {
    /// Show what is inside this artifact. Absent: the top level.
    #[serde(rename = "in")]
    pub inside: Option<Uuid>,
    /// Search every level by name and tag.
    pub q: Option<String>,
    /// all (default), mine, drafts, review, topics.
    pub filter: Option<String>,
    /// Highlight this artifact. On first load without `in`, the finder opens
    /// at the level that shows it, so a deep link lands in the right place.
    pub selected: Option<Uuid>,
    /// navigate (default) or pick.
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Mine,
    Drafts,
    Review,
    Topics,
}

impl Filter {
    pub fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("all") {
            "mine" => Self::Mine,
            "drafts" => Self::Drafts,
            "review" => Self::Review,
            "topics" => Self::Topics,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mine => "mine",
            Self::Drafts => "drafts",
            Self::Review => "review",
            Self::Topics => "topics",
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Row {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub is_topic: bool,
    pub parent_id: Option<Uuid>,
    /// Live artifacts directly inside this one.
    pub children: i64,
}

/// Columns every listing selects, so `Row` can be read from any of them.
const ROW_COLUMNS: &str = "a.id, a.name, a.status::text AS status, a.is_topic, a.parent_id, \
     (SELECT count(*) FROM artifacts c WHERE c.parent_id = a.id AND c.deleted_at IS NULL) AS children";

/// What the finder shows: one level, or a flat list with paths.
#[derive(Debug, Serialize)]
pub struct Listing {
    /// The artifact being looked inside, shown first so it can be opened too.
    pub current: Option<Row>,
    /// Ancestors of `current`, outermost first, for the breadcrumb.
    pub trail: Vec<Crumb>,
    pub rows: Vec<Row>,
    /// For flat lists: each row's path ("Central Heating Plant › …").
    pub paths: HashMap<Uuid, String>,
    /// At the top level: how many topics the Topics folder holds.
    pub topic_count: i64,
    /// A flat list was cut at LIST_LIMIT.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Crumb {
    pub id: Uuid,
    pub name: String,
}

/// The parent of `id`, or None if it is at the top (or gone).
async fn parent_of(db: &PgPool, id: Uuid) -> AppResult<Option<Uuid>> {
    Ok(sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT parent_id FROM artifacts WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .flatten())
}

/// Names and parents of the given artifacts and all their ancestors.
async fn ancestry(db: &PgPool, ids: &[Uuid]) -> AppResult<HashMap<Uuid, (String, Option<Uuid>)>> {
    let mut known: HashMap<Uuid, (String, Option<Uuid>)> = HashMap::new();
    let mut want: Vec<Uuid> = ids.to_vec();
    // Nesting is shallow (building, room, item), but a loop is cheap
    // insurance; the database also refuses cycles.
    for _ in 0..16 {
        want.retain(|id| !known.contains_key(id));
        if want.is_empty() {
            break;
        }
        let rows: Vec<(Uuid, String, Option<Uuid>)> =
            sqlx::query_as("SELECT id, name, parent_id FROM artifacts WHERE id = ANY($1)")
                .bind(&want)
                .fetch_all(db)
                .await?;
        want = rows.iter().filter_map(|r| r.2).collect();
        for (id, name, parent) in rows {
            known.insert(id, (name, parent));
        }
    }
    Ok(known)
}

/// Outermost-first ancestors of `id` (not including it).
fn crumbs(known: &HashMap<Uuid, (String, Option<Uuid>)>, id: Uuid) -> Vec<Crumb> {
    let mut out = Vec::new();
    let mut cur = known.get(&id).and_then(|(_, p)| *p);
    while let Some(p) = cur {
        let Some((name, next)) = known.get(&p) else {
            break;
        };
        out.push(Crumb {
            id: p,
            name: name.clone(),
        });
        cur = *next;
        if out.len() > 16 {
            break;
        }
    }
    out.reverse();
    out
}

/// Percent-encodes a query-string value (RFC 3986 unreserved characters
/// pass through).
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Escapes LIKE wildcards so a search for "50%" means fifty percent.
fn like_pattern(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// One level of the hierarchy: the top, or what is inside `inside`.
pub async fn level(db: &PgPool, inside: Option<Uuid>) -> AppResult<Listing> {
    let (current, rows) = match inside {
        Some(parent) => {
            let current = sqlx::query_as::<_, Row>(&format!(
                "SELECT {ROW_COLUMNS} FROM artifacts a WHERE a.id = $1 AND a.deleted_at IS NULL"
            ))
            .bind(parent)
            .fetch_optional(db)
            .await?;
            let rows = sqlx::query_as::<_, Row>(&format!(
                "SELECT {ROW_COLUMNS} FROM artifacts a \
                 WHERE a.parent_id = $1 AND a.deleted_at IS NULL ORDER BY lower(a.name), a.id"
            ))
            .bind(parent)
            .fetch_all(db)
            .await?;
            (current, rows)
        }
        None => {
            let rows = sqlx::query_as::<_, Row>(&format!(
                "SELECT {ROW_COLUMNS} FROM artifacts a \
                 WHERE a.parent_id IS NULL AND NOT a.is_topic AND a.deleted_at IS NULL \
                 ORDER BY lower(a.name), a.id"
            ))
            .fetch_all(db)
            .await?;
            (None, rows)
        }
    };
    // An artifact that has gone (deleted while someone was looking at it)
    // falls back to the top level rather than an empty page.
    if inside.is_some() && current.is_none() {
        return Box::pin(level(db, None)).await;
    }
    let trail = match &current {
        Some(c) => crumbs(&ancestry(db, &[c.id]).await?, c.id),
        None => Vec::new(),
    };
    let topic_count: i64 = if inside.is_none() {
        sqlx::query_scalar("SELECT count(*) FROM artifacts WHERE is_topic AND deleted_at IS NULL")
            .fetch_one(db)
            .await?
    } else {
        0
    };
    Ok(Listing {
        current,
        trail,
        rows,
        paths: HashMap::new(),
        topic_count,
        truncated: false,
    })
}

/// A flat list across every level: a search, a filter, or both.
pub async fn flat(db: &PgPool, q: Option<&str>, filter: Filter, me: Uuid) -> AppResult<Listing> {
    let mut sql = format!("SELECT {ROW_COLUMNS} FROM artifacts a WHERE a.deleted_at IS NULL");
    match filter {
        Filter::All => {}
        Filter::Mine => sql.push_str(" AND a.created_by = $2"),
        Filter::Drafts => sql.push_str(" AND a.status = 'draft' AND NOT a.is_topic"),
        Filter::Review => sql.push_str(" AND a.status = 'ready'"),
        Filter::Topics => sql.push_str(" AND a.is_topic"),
    }
    let q = q.map(str::trim).filter(|s| !s.is_empty());
    if q.is_some() {
        sql.push_str(
            " AND (a.name ILIKE $1 ESCAPE '\\' OR EXISTS (SELECT 1 FROM artifact_tags t \
             WHERE t.artifact_id = a.id AND t.tag ILIKE $1 ESCAPE '\\'))",
        );
    }
    // Name matches that start with the search come first.
    sql.push_str(&format!(
        " ORDER BY (a.name ILIKE substr($1, 2) ESCAPE '\\') DESC, lower(a.name), a.id LIMIT {}",
        LIST_LIMIT + 1
    ));
    let mut rows = sqlx::query_as::<_, Row>(&sql)
        .bind(like_pattern(q.unwrap_or("")))
        .bind(me)
        .fetch_all(db)
        .await?;
    let truncated = rows.len() as i64 > LIST_LIMIT;
    rows.truncate(LIST_LIMIT as usize);

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let known = ancestry(db, &ids).await?;
    let paths = rows
        .iter()
        .filter_map(|r| {
            let names: Vec<String> = crumbs(&known, r.id).into_iter().map(|c| c.name).collect();
            (!names.is_empty()).then(|| (r.id, names.join(" › ")))
        })
        .collect();
    Ok(Listing {
        current: None,
        trail: Vec::new(),
        rows,
        paths,
        topic_count: 0,
        truncated,
    })
}

async fn finder(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<FinderQuery>,
) -> AppResult<Html<String>> {
    let filter = Filter::parse(query.filter.as_deref());
    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let pick = query.mode.as_deref() == Some("pick");

    // First load of a deep link: open at the level that shows the selection.
    // An artifact that holds others opens inside itself; anything else opens
    // inside its parent.
    let mut inside = query.inside;
    if inside.is_none() && q.is_none() && filter == Filter::All {
        if let Some(sel) = query.selected {
            let holds_others: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM artifacts WHERE parent_id = $1 AND deleted_at IS NULL)",
            )
            .bind(sel)
            .fetch_one(&state.db)
            .await?;
            inside = if holds_others {
                Some(sel)
            } else {
                parent_of(&state.db, sel).await?
            };
        }
    }

    let flat_list = q.is_some() || filter != Filter::All;
    let listing = if flat_list {
        flat(&state.db, q, filter, author.0.id).await?
    } else {
        level(&state.db, inside).await?
    };

    // The URL this exact view reloads from on refresh-tree.
    let mut params: Vec<String> = Vec::new();
    if let Some(i) = listing.current.as_ref().map(|c| c.id) {
        params.push(format!("in={i}"));
    }
    if let Some(q) = q {
        params.push(format!("q={}", encode(q)));
    }
    if filter != Filter::All {
        params.push(format!("filter={}", filter.as_str()));
    }
    if pick {
        params.push("mode=pick".to_string());
    }
    let self_url = format!("/studio/finder?{}", params.join("&"));

    let paths: HashMap<String, String> = listing
        .paths
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();

    render(
        &environment(),
        "studio_finder.html",
        context! {
            listing => Value::from_serialize(&listing),
            paths => paths,
            flat => flat_list,
            filter => filter.as_str(),
            q => q,
            pick => pick,
            selected_id => query.selected.map(|u| u.to_string()),
            self_url => self_url,
            list_limit => LIST_LIMIT,
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_round_trip() {
        for f in ["all", "mine", "drafts", "review", "topics"] {
            assert_eq!(Filter::parse(Some(f)).as_str(), f);
        }
        assert_eq!(Filter::parse(Some("bogus")), Filter::All);
        assert_eq!(Filter::parse(None), Filter::All);
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("solar & wind"), "solar%20%26%20wind");
        assert_eq!(encode("Café"), "Caf%C3%A9");
    }

    #[test]
    fn like_wildcards_are_literal() {
        assert_eq!(like_pattern("50%"), "%50\\%%");
        assert_eq!(like_pattern("a_b"), "%a\\_b%");
    }

    async fn add(
        db: &PgPool,
        name: &str,
        parent: Option<Uuid>,
        topic: bool,
    ) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO artifacts (kind, name, description, parent_id, is_topic) \
             VALUES ('other', $1, 'x', $2, $3) RETURNING id",
        )
        .bind(name)
        .bind(parent)
        .bind(topic)
        .fetch_one(db)
        .await?)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn levels_show_one_layer_with_counts_and_a_breadcrumb(db: PgPool) -> anyhow::Result<()> {
        let plant = add(&db, "Central Heating Plant", None, false).await?;
        let pond = add(&db, "Campus Pond", None, false).await?;
        let battery = add(&db, "Battery", Some(plant), false).await?;
        let cell = add(&db, "Cell 1", Some(battery), false).await?;
        add(&db, "Solar power", None, true).await?;

        let top = level(&db, None).await?;
        let names: Vec<_> = top.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["Campus Pond", "Central Heating Plant"],
            "topics are in their own folder"
        );
        assert_eq!(top.topic_count, 1);
        assert_eq!(top.rows.iter().find(|r| r.id == plant).unwrap().children, 1);
        assert_eq!(top.rows.iter().find(|r| r.id == pond).unwrap().children, 0);

        let inside = level(&db, Some(battery)).await?;
        assert_eq!(inside.current.as_ref().map(|c| c.id), Some(battery));
        assert_eq!(inside.rows.iter().map(|r| r.id).collect::<Vec<_>>(), [cell]);
        assert_eq!(
            inside
                .trail
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["Central Heating Plant"]
        );

        // Looking inside something deleted falls back to the top.
        sqlx::query("UPDATE artifacts SET deleted_at = now() WHERE id = $1")
            .bind(cell)
            .execute(&db)
            .await?;
        sqlx::query("UPDATE artifacts SET deleted_at = now() WHERE id = $1")
            .bind(battery)
            .execute(&db)
            .await?;
        let gone = level(&db, Some(battery)).await?;
        assert!(gone.current.is_none());
        assert_eq!(gone.rows.len(), 2);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn search_matches_names_and_tags_across_levels_with_paths(
        db: PgPool,
    ) -> anyhow::Result<()> {
        let plant = add(&db, "Central Heating Plant", None, false).await?;
        let battery = add(&db, "Battery storage", Some(plant), false).await?;
        let pond = add(&db, "Campus Pond", None, false).await?;
        sqlx::query("INSERT INTO artifact_tags (artifact_id, tag) VALUES ($1, 'stormwater')")
            .bind(pond)
            .execute(&db)
            .await?;
        let me = Uuid::new_v4();

        let hits = flat(&db, Some("batt"), Filter::All, me).await?;
        assert_eq!(
            hits.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [battery]
        );
        assert_eq!(
            hits.paths.get(&battery).map(String::as_str),
            Some("Central Heating Plant")
        );

        let by_tag = flat(&db, Some("STORM"), Filter::All, me).await?;
        assert_eq!(by_tag.rows.iter().map(|r| r.id).collect::<Vec<_>>(), [pond]);

        // A literal % matches nothing here rather than everything.
        assert!(flat(&db, Some("%"), Filter::All, me).await?.rows.is_empty());
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn filters_list_across_levels(db: PgPool) -> anyhow::Result<()> {
        let plant = add(&db, "Plant", None, false).await?;
        let battery = add(&db, "Battery", Some(plant), false).await?;
        let topic = add(&db, "Solar power", None, true).await?;
        sqlx::query("UPDATE artifacts SET status = 'approved' WHERE id = $1")
            .bind(plant)
            .execute(&db)
            .await?;
        sqlx::query("UPDATE artifacts SET status = 'ready' WHERE id = $1")
            .bind(battery)
            .execute(&db)
            .await?;
        let me = Uuid::new_v4();

        let review = flat(&db, None, Filter::Review, me).await?;
        assert_eq!(
            review.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [battery]
        );
        let topics = flat(&db, None, Filter::Topics, me).await?;
        assert_eq!(
            topics.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [topic]
        );
        // Drafts leaves topics out: they are reference pages, not field work.
        assert!(flat(&db, None, Filter::Drafts, me).await?.rows.is_empty());
        // Nothing was created by this author.
        assert!(flat(&db, None, Filter::Mine, me).await?.rows.is_empty());
        Ok(())
    }
}
