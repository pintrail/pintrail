use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Author privilege levels, ordered viewer < editor < admin.
///
/// The `Ord` derive is what `RequireAuthorRole` compares against, so variant
/// order here is load-bearing: reordering these silently changes who can do
/// what. New levels must be inserted at the correct privilege position, not
/// appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "author_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AuthorRole {
    Viewer,
    Editor,
    Admin,
}

impl AuthorRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthorRole::Viewer => "viewer",
            AuthorRole::Editor => "editor",
            AuthorRole::Admin => "admin",
        }
    }
}

impl std::str::FromStr for AuthorRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "viewer" => Ok(AuthorRole::Viewer),
            "editor" => Ok(AuthorRole::Editor),
            "admin" => Ok(AuthorRole::Admin),
            other => Err(format!(
                "unknown role {other:?} (expected viewer, editor, or admin)"
            )),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Author {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: AuthorRole,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The wire representation. Distinct from [`Author`] so `password_hash` cannot
/// reach a response body by accident — serializing the row type directly is
/// the usual way that leak happens.
#[derive(Debug, Serialize)]
pub struct AuthorView {
    pub id: Uuid,
    pub email: String,
    pub role: AuthorRole,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

impl From<Author> for AuthorView {
    fn from(a: Author) -> Self {
        Self {
            id: a.id,
            email: a.email,
            role: a.role,
            is_active: a.is_active,
            created_at: a.created_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthorSession {
    pub id: Uuid,
    pub author_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering_is_privilege_ordering() {
        assert!(AuthorRole::Viewer < AuthorRole::Editor);
        assert!(AuthorRole::Editor < AuthorRole::Admin);
        assert!(AuthorRole::Admin >= AuthorRole::Admin);
    }

    #[test]
    fn role_parses_case_insensitively() {
        assert_eq!("ADMIN".parse::<AuthorRole>().unwrap(), AuthorRole::Admin);
        assert_eq!(" editor ".parse::<AuthorRole>().unwrap(), AuthorRole::Editor);
        assert!("root".parse::<AuthorRole>().is_err());
    }
}
