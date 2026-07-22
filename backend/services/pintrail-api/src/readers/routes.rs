use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::auth::{
    generate_session_token, issue_verification_token, looks_like_email, validate_password,
};
use super::extractors::AuthenticatedReader;
use super::model::{Reader, ReaderTokenPurpose, ReaderView};
use crate::crypto::{hash_password, hash_token, verify_dummy_password, verify_password};
use crate::error::{AppError, AppResult};
use crate::client_ip::ClientIp;
use crate::mail;
use crate::rate_limit::{EMAIL_TRIGGER_PER_IP, LOGIN_PER_ACCOUNT, LOGIN_PER_IP};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/readers/register", post(register))
        .route("/readers/verify-email", post(verify_email))
        .route("/readers/resend-verification", post(resend_verification))
        .route("/readers/login", post(login))
        .route("/readers/logout", post(logout))
        .route("/readers/me", get(me))
        .route("/readers/password-reset", post(request_password_reset))
        .route("/readers/password-reset/confirm", post(confirm_password_reset))
}

/// The response every registration and password-reset request returns,
/// whether or not the address has an account. See [`register`].
fn accepted() -> (StatusCode, Json<Value>) {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "message": "If that address can receive mail, a message is on its way."
        })),
    )
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    /// Optional. Without one, comments show a pseudonym rather than anything
    /// derived from the email address.
    pub display_name: Option<String>,
}

/// Self-service registration.
///
/// Returns the identical 202 whether the address is new or already
/// registered. The reader tier is public and potentially tens of thousands of
/// accounts, so a 409 on duplicate would turn this endpoint into an oracle for
/// "does this person have an account" — answerable by anyone, for any address.
///
/// The real owner is not left in the dark: an existing-account address gets a
/// "someone tried to register" message instead of a verification link.
async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    // Every call here can send mail, so the limit protects other people's
    // inboxes as much as this service.
    state.limiter.check(&format!("register:{ip}"), EMAIL_TRIGGER_PER_IP)?;

    let email = body.email.trim();

    if !looks_like_email(email) {
        return Err(AppError::BadRequest(
            "a valid email address is required".into(),
        ));
    }
    validate_password(&body.password)?;

    let hash = hash_password(&body.password)?;
    let mut tx = state.db.begin().await?;

    let existing = sqlx::query_as::<_, Reader>(
        r#"
        SELECT id, email, email_verified, password_hash, is_active, display_name, created_at, updated_at
        FROM readers WHERE lower(email) = lower($1)
        "#,
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;

    match existing {
        Some(reader) if reader.email_verified => {
            // Established account. Do not touch it -- an unauthenticated
            // caller must not be able to alter an existing password.
            tx.commit().await?;
            state
                .mailer
                .send(mail::duplicate_registration_email(&reader.email))?;
            tracing::info!(reader_id = %reader.id, "registration attempt on existing account");
        }
        Some(reader) => {
            // Registered but never verified. Re-send the link, but do NOT
            // touch the stored password.
            //
            // Refreshing it here would be account takeover: an attacker who
            // knows an unverified address could re-register it with their own
            // password, and the fresh verification link would go to the real
            // owner's inbox. The owner clicks it in good faith, and the
            // account ends up verified under the attacker's password. No
            // unauthenticated endpoint may change an existing credential.
            //
            // Someone who genuinely mistyped their password recovers through
            // the password-reset flow, which proves mailbox control first.
            let token =
                issue_verification_token(&mut *tx, reader.id, ReaderTokenPurpose::VerifyEmail)
                    .await?;
            tx.commit().await?;

            state.mailer.send(mail::verification_email(
                &reader.email,
                &state.settings.verify_link(&token.raw),
            ))?;
            tracing::info!(reader_id = %reader.id, "re-sent verification for unverified account");
        }
        None => {
            let display_name = body
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty());

            let reader_id: Uuid = sqlx::query_scalar(
                "INSERT INTO readers (email, password_hash, display_name) \
                 VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(email)
            .bind(&hash)
            .bind(display_name)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| match &e {
                // Only display_name can collide here; the email case was
                // handled above.
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    AppError::Conflict("that display name is taken".into())
                }
                _ => AppError::from(e),
            })?;

            let token =
                issue_verification_token(&mut *tx, reader_id, ReaderTokenPurpose::VerifyEmail)
                    .await?;
            tx.commit().await?;

            state.mailer.send(mail::verification_email(
                email,
                &state.settings.verify_link(&token.raw),
            ))?;
            tracing::info!(reader_id = %reader_id, "reader registered");
        }
    }

    Ok(accepted())
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub token: String,
}

/// Confirms an email address. The token is single-use and expiring.
async fn verify_email(
    State(state): State<AppState>,
    Json(body): Json<TokenRequest>,
) -> AppResult<Json<Value>> {
    let token_hash = hash_token(body.token.trim());
    let mut tx = state.db.begin().await?;

    // Consume and validate in one statement: a plain SELECT-then-UPDATE would
    // let two concurrent requests both observe the token as unconsumed.
    let reader_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE reader_verification_tokens
        SET consumed_at = now()
        WHERE token_hash = $1
          AND purpose = 'verify_email'
          AND consumed_at IS NULL
          AND expires_at > $2
        RETURNING reader_id
        "#,
    )
    .bind(&token_hash)
    .bind(Utc::now())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(reader_id) = reader_id else {
        return Err(AppError::BadRequest(
            "verification link is invalid or has expired".into(),
        ));
    };

    sqlx::query("UPDATE readers SET email_verified = true WHERE id = $1")
        .bind(reader_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    tracing::info!(reader_id = %reader_id, "email verified");

    Ok(Json(json!({ "status": "verified" })))
}

#[derive(Debug, Deserialize)]
pub struct EmailRequest {
    pub email: String,
}

/// Re-sends a verification link. Same opaque response as registration.
async fn resend_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<EmailRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    state.limiter.check(&format!("resend:{ip}"), EMAIL_TRIGGER_PER_IP)?;

    let email = body.email.trim();
    let mut tx = state.db.begin().await?;

    let reader = sqlx::query_as::<_, Reader>(
        r#"
        SELECT id, email, email_verified, password_hash, is_active, display_name, created_at, updated_at
        FROM readers WHERE lower(email) = lower($1) AND is_active AND NOT email_verified
        "#,
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(reader) = reader {
        let token =
            issue_verification_token(&mut *tx, reader.id, ReaderTokenPurpose::VerifyEmail).await?;
        tx.commit().await?;

        state.mailer.send(mail::verification_email(
            &reader.email,
            &state.settings.verify_link(&token.raw),
        ))?;
    }

    Ok(accepted())
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Issues a bearer token.
///
/// Unverified readers may sign in and browse — verification gates *writing*,
/// not reading (DESIGN.md §2.6). The response reports `email_verified` so the
/// app can show a "confirm your address" prompt rather than discovering the
/// restriction only when a comment fails.
async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<Value>> {
    let email = body.email.trim();

    let account_key = format!("reader-login:{}", email.to_ascii_lowercase());
    let ip_key = format!("reader-login-ip:{ip}");
    state.limiter.check(&ip_key, LOGIN_PER_IP)?;
    state.limiter.check(&account_key, LOGIN_PER_ACCOUNT)?;

    let reader = sqlx::query_as::<_, Reader>(
        r#"
        SELECT id, email, email_verified, password_hash, is_active, display_name, created_at, updated_at
        FROM readers WHERE lower(email) = lower($1)
        "#,
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    let reader = match reader {
        Some(r) => r,
        None => {
            verify_dummy_password(&body.password);
            return Err(AppError::InvalidCredentials);
        }
    };

    if !verify_password(&body.password, &reader.password_hash) || !reader.is_active {
        tracing::warn!(reader_id = %reader.id, "failed reader login");
        return Err(AppError::InvalidCredentials);
    }

    let token = generate_session_token();

    sqlx::query(
        "INSERT INTO reader_sessions (reader_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(reader.id)
    .bind(&token.hash)
    .bind(token.expires_at)
    .execute(&state.db)
    .await?;

    state.limiter.reset(&account_key);
    state.limiter.reset(&ip_key);

    tracing::info!(reader_id = %reader.id, "reader logged in");

    Ok(Json(json!({
        "token": token.raw,
        "expires_at": token.expires_at,
        "reader": ReaderView::from(reader),
    })))
}

/// Revokes the presented token server-side. A bearer token the client merely
/// forgets stays valid for its full 90 days otherwise.
async fn logout(
    State(state): State<AppState>,
    reader: AuthenticatedReader,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    if let Some(raw) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .map(|(_, token)| token.trim())
    {
        sqlx::query("DELETE FROM reader_sessions WHERE token_hash = $1")
            .bind(hash_token(raw))
            .execute(&state.db)
            .await?;
    }

    tracing::info!(reader_id = %reader.0.id, "reader logged out");
    Ok(Json(json!({ "status": "logged out" })))
}

async fn me(reader: AuthenticatedReader) -> Json<Value> {
    Json(json!({ "reader": ReaderView::from(reader.into_inner()) }))
}

/// Starts a password reset. Opaque response, for the same reason as
/// registration.
async fn request_password_reset(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<EmailRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    state.limiter.check(&format!("reset:{ip}"), EMAIL_TRIGGER_PER_IP)?;

    let email = body.email.trim();
    let mut tx = state.db.begin().await?;

    let reader = sqlx::query_as::<_, Reader>(
        r#"
        SELECT id, email, email_verified, password_hash, is_active, display_name, created_at, updated_at
        FROM readers WHERE lower(email) = lower($1) AND is_active
        "#,
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(reader) = reader {
        let token =
            issue_verification_token(&mut *tx, reader.id, ReaderTokenPurpose::ResetPassword)
                .await?;
        tx.commit().await?;

        state.mailer.send(mail::password_reset_email(
            &reader.email,
            &state.settings.reset_link(&token.raw),
        ))?;
        tracing::info!(reader_id = %reader.id, "password reset requested");
    }

    Ok(accepted())
}

#[derive(Debug, Deserialize)]
pub struct ConfirmResetRequest {
    pub token: String,
    pub password: String,
}

async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(body): Json<ConfirmResetRequest>,
) -> AppResult<Json<Value>> {
    validate_password(&body.password)?;

    let token_hash = hash_token(body.token.trim());
    let hash = hash_password(&body.password)?;
    let mut tx = state.db.begin().await?;

    let reader_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE reader_verification_tokens
        SET consumed_at = now()
        WHERE token_hash = $1
          AND purpose = 'reset_password'
          AND consumed_at IS NULL
          AND expires_at > $2
        RETURNING reader_id
        "#,
    )
    .bind(&token_hash)
    .bind(Utc::now())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(reader_id) = reader_id else {
        return Err(AppError::BadRequest(
            "reset link is invalid or has expired".into(),
        ));
    };

    // Completing a reset proves control of the mailbox, which is exactly what
    // verification asks for -- so a reset also verifies the address.
    sqlx::query("UPDATE readers SET password_hash = $1, email_verified = true WHERE id = $2")
        .bind(&hash)
        .bind(reader_id)
        .execute(&mut *tx)
        .await?;

    // The usual reason to reset is a compromised credential, so every
    // existing session dies with it.
    let revoked = sqlx::query("DELETE FROM reader_sessions WHERE reader_id = $1")
        .bind(reader_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;
    tracing::info!(reader_id = %reader_id, revoked, "password reset completed");

    Ok(Json(json!({ "status": "password updated", "sessions_revoked": revoked })))
}
