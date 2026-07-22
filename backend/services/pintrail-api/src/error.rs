use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// The single error type every handler returns.
///
/// The `Internal` variant carries the underlying error for the log but never
/// puts it in the response body — database URLs, row contents, and storage
/// keys have a habit of ending up in error strings, and this tier is public.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("authentication required")]
    Unauthorized,

    /// Deliberately one variant for every login failure -- wrong password,
    /// unknown email, suspended account. Separate messages would let an
    /// attacker enumerate which emails have accounts.
    #[error("invalid email or password")]
    InvalidCredentials,

    #[error("insufficient permissions")]
    Forbidden,

    /// Distinct from `Forbidden` so the mobile app can tell "you need to
    /// confirm your email" from "you may never do this" and offer to resend
    /// the verification message.
    #[error("email address must be verified first")]
    EmailNotVerified,

    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("{0}")]
    Conflict(String),

    #[error("rate limit exceeded")]
    TooManyRequests,

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            // A bare RowNotFound almost always means "the id in the path does
            // not exist", which is a 404 rather than a 500.
            sqlx::Error::RowNotFound => AppError::NotFound("resource"),
            other => AppError::Internal(other.into()),
        }
    }
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::EmailNotVerified => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        let message = match &self {
            AppError::Internal(err) => {
                tracing::error!(error = ?err, "unhandled internal error");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
