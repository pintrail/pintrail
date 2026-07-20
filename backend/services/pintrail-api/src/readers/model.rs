use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "reader_token_purpose", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ReaderTokenPurpose {
    VerifyEmail,
    ResetPassword,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Reader {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub password_hash: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Wire representation. Separate from [`Reader`] so `password_hash` cannot
/// reach a response body by accident.
#[derive(Debug, Serialize)]
pub struct ReaderView {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
}

impl From<Reader> for ReaderView {
    fn from(r: Reader) -> Self {
        Self {
            id: r.id,
            email: r.email,
            email_verified: r.email_verified,
            created_at: r.created_at,
        }
    }
}
