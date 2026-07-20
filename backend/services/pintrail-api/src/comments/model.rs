use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "comment_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CommentStatus {
    Visible,
    Hidden,
    Flagged,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CommentRow {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub reader_id: Uuid,
    pub body: String,
    pub status: CommentStatus,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A comment as other users see it.
///
/// Carries `author` — never the commenter's email address. Publishing emails
/// to every reader would be a privacy breach, and readers who have not chosen
/// a display name get a pseudonym derived from their id (see
/// [`display_name_for`]).
#[derive(Debug, Serialize)]
pub struct CommentView {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub author: String,
    pub body: String,
    /// Whether the caller wrote this, so a client can offer "delete".
    pub is_mine: bool,
    pub created_at: DateTime<Utc>,
}

/// The name shown beside a comment.
///
/// Derived from the reader's id rather than their email: an email local part
/// is often a real name or a university username, and neither belongs on a
/// public comment thread just because someone never set a display name.
/// Stable, so the same person reads as the same author across a thread.
pub fn display_name_for(reader_id: Uuid, display_name: Option<&str>) -> String {
    match display_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => format!("explorer-{}", &reader_id.simple().to_string()[..6]),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateComment {
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_a_chosen_display_name() {
        let id = Uuid::nil();
        assert_eq!(display_name_for(id, Some("jstudent42")), "jstudent42");
        assert_eq!(display_name_for(id, Some("  spaced  ")), "spaced");
    }

    #[test]
    fn falls_back_to_a_pseudonym_never_an_email() {
        let id = Uuid::parse_str("a3a39501-beca-4916-8077-07cd16f04cf7").unwrap();
        let name = display_name_for(id, None);

        assert_eq!(name, "explorer-a3a395");
        assert!(!name.contains('@'), "must never look like an email");
    }

    #[test]
    fn blank_display_name_is_treated_as_unset() {
        let id = Uuid::parse_str("a3a39501-beca-4916-8077-07cd16f04cf7").unwrap();
        assert_eq!(display_name_for(id, Some("   ")), "explorer-a3a395");
    }

    #[test]
    fn pseudonym_is_stable_for_the_same_reader() {
        let id = Uuid::new_v4();
        assert_eq!(display_name_for(id, None), display_name_for(id, None));
    }
}
