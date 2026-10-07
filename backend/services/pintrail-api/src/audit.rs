//! Who did what to an artifact: the actor for the change history, and the
//! ownership rules for editing and deleting.
//!
//! The history itself is written by database triggers (migration
//! 20261007000001), which covers every write path. They learn who is acting
//! from a transaction-local setting, so every authored write goes through
//! [`begin_as`]. A write that skips it still lands in the history, with no
//! author, which the Studio shows as "system".
//!
//! Ownership: an editor may change only artifacts they created; an admin may
//! change anything. Anyone with editor rights may add an artifact *inside*
//! someone else's, which is how a group builds out one building together.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::authors::model::{Author, AuthorRole};
use crate::error::{AppError, AppResult};

/// Opens a transaction whose writes are attributed to `actor`.
///
/// `set_config(..., true)` scopes the setting to this transaction, so it
/// cannot leak to the next request that borrows the pooled connection.
pub async fn begin_as(db: &PgPool, actor: Uuid) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('pintrail.actor_id', $1, true)")
        .bind(actor.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

pub fn is_admin(author: &Author) -> bool {
    author.role == AuthorRole::Admin
}

#[derive(Debug, sqlx::FromRow)]
pub struct Ownership {
    pub created_by: Option<Uuid>,
}

pub fn may_modify(author: &Author, own: &Ownership) -> bool {
    is_admin(author) || (author.role >= AuthorRole::Editor && own.created_by == Some(author.id))
}

pub async fn ownership<'e, E: sqlx::PgExecutor<'e>>(ex: E, id: Uuid) -> AppResult<Ownership> {
    sqlx::query_as::<_, Ownership>(
        "SELECT created_by FROM artifacts WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(ex)
    .await?
    .ok_or(AppError::NotFound("artifact"))
}

/// Fails with 403 unless `author` may change artifact `id`.
pub async fn ensure_can_modify<'e, E: sqlx::PgExecutor<'e>>(
    ex: E,
    author: &Author,
    id: Uuid,
) -> AppResult<Ownership> {
    let own = ownership(ex, id).await?;
    if may_modify(author, &own) {
        Ok(own)
    } else {
        Err(AppError::Forbidden)
    }
}

/// Whether `author` may delete artifact `id`, which also deletes everything
/// nested inside it. An admin may delete anything. An editor may delete only
/// when every artifact in the subtree is theirs and none of it is approved,
/// so one mis-tap can't take a classmate's work or reviewed work with it.
pub async fn can_delete<'e, E: sqlx::PgExecutor<'e>>(ex: E, author: &Author, id: Uuid) -> AppResult<bool> {
    if is_admin(author) {
        return Ok(true);
    }
    if author.role < AuthorRole::Editor {
        return Ok(false);
    }
    let ok: Option<bool> = sqlx::query_scalar(
        r#"
        WITH RECURSIVE subtree AS (
            SELECT id, created_by, status FROM artifacts WHERE id = $1 AND deleted_at IS NULL
            UNION ALL
            SELECT a.id, a.created_by, a.status
            FROM artifacts a JOIN subtree s ON a.parent_id = s.id
            WHERE a.deleted_at IS NULL
        )
        SELECT bool_and(created_by IS NOT DISTINCT FROM $2 AND status <> 'approved') FROM subtree
        "#,
    )
    .bind(id)
    .bind(author.id)
    .fetch_one(ex)
    .await?;
    Ok(ok.unwrap_or(false))
}

/// After a non-admin changes an approved artifact, it goes back into the
/// review queue: what was approved is no longer what's there.
pub async fn reopen_if_approved(
    tx: &mut Transaction<'_, Postgres>,
    author: &Author,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    if !is_admin(author) {
        sqlx::query(
            "UPDATE artifacts SET status = 'ready', submitted_at = now(), review_note = '' \
             WHERE id = $1 AND status = 'approved'",
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Records that `author` changed something hanging off the artifact (a link,
/// a photo), so "last edited by" and the review status reflect it.
pub async fn touch(tx: &mut Transaction<'_, Postgres>, author: &Author, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE artifacts SET updated_at = now(), updated_by = $2 WHERE id = $1")
        .bind(id)
        .bind(author.id)
        .execute(&mut **tx)
        .await?;
    reopen_if_approved(tx, author, id).await
}
