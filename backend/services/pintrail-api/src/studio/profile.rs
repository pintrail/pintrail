//! Author profiles: name, pronouns, affiliation, bio, and a profile photo.
//!
//! The Studio names people by these rather than by email address. Photos are
//! uploaded straight to this server (they are small and few, so the
//! presigned-upload path the media pipeline uses would be overkill), cropped
//! to a square, resized, re-encoded as WebP, and stored in the media bucket.
//! Re-encoding also strips whatever metadata the original carried, location
//! included.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use image::{DynamicImage, ImageDecoder, ImageReader};
use minijinja::{context, Value};
use serde::Deserialize;
use serde_json::{json, Value as Json};
use sqlx::PgPool;
use uuid::Uuid;

use super::routes::{
    base_ctx, environment, render, require_csrf, respond, session_token, CsrfForm,
};
use crate::admin::csrf;
use crate::authors::extractors::AuthenticatedAuthor;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Side length of the stored photo. Shown at most ~96px, so 320 covers
/// high-density screens with room to spare.
const AVATAR_PX: u32 = 320;
/// Phones produce 5-15 MB photos; anything past this is not a photo.
const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;
/// Refuse images whose decoded size would be unreasonable (decompression
/// bombs), before allocating for them.
const MAX_PIXELS: u64 = 60_000_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio/profile", get(profile_form).post(profile_submit))
        .route(
            "/studio/profile/photo",
            post(upload_photo).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route("/studio/profile/photo/remove", post(remove_photo))
        .route("/studio/profile/header-avatar", get(header_avatar))
        .route("/studio/avatars/{id}", get(avatar))
}

// --- how people appear -----------------------------------------------------

/// Where an author's photo is served. The version is the stored key's own
/// random part, so a new photo gets a new URL and the old one can be cached
/// forever.
pub fn avatar_url(id: Uuid, key: Option<&str>) -> Option<String> {
    key.map(|k| {
        let v = k.rsplit('/').next().unwrap_or(k).trim_end_matches(".webp");
        format!("/studio/avatars/{id}?v={v}")
    })
}

/// Up to two initials: first and last word of the name.
pub fn initials(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name);
    let words: Vec<&str> = base.split(|c: char| c.is_whitespace() || c == '.' || c == '_' || c == '-')
        .filter(|w| !w.is_empty())
        .collect();
    let first = |w: &str| w.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    match words.as_slice() {
        [] => "?".into(),
        [one] => first(one),
        [a, .., b] => format!("{}{}", first(a), first(b)),
    }
}

/// A stable colour for the initials badge, so each person keeps theirs.
fn hue(id: Uuid) -> u16 {
    (id.as_u128() % 360) as u16
}

/// Everything a template needs to show a person: name, photo or initials.
pub fn person(id: Uuid, label: &str, avatar_key: Option<&str>) -> Json {
    json!({
        "id": id.to_string(),
        "name": label,
        "avatar": avatar_url(id, avatar_key),
        "initials": initials(label),
        "hue": hue(id),
    })
}

/// Loads [`person`] values for a set of author ids. Unknown ids (a removed
/// account) are simply absent.
pub async fn load_people(db: &PgPool, ids: &[Uuid]) -> Result<HashMap<Uuid, Json>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT id, author_label(id), avatar_key FROM authors WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, label, key)| (id, person(id, &label, key.as_deref())))
        .collect())
}

// --- the profile page ----------------------------------------------------

#[derive(sqlx::FromRow)]
struct ProfileRow {
    email: String,
    full_name: String,
    display_name: String,
    pronouns: String,
    affiliation: String,
    bio: String,
    avatar_key: Option<String>,
}

async fn load_profile(db: &PgPool, id: Uuid) -> AppResult<ProfileRow> {
    Ok(sqlx::query_as::<_, ProfileRow>(
        "SELECT email, full_name, display_name, pronouns, affiliation, bio, avatar_key \
         FROM authors WHERE id = $1",
    )
    .bind(id)
    .fetch_one(db)
    .await?)
}

#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    /// Set when the Studio sent a new author here to fill in their name.
    welcome: Option<String>,
    /// Set after a successful save.
    saved: Option<String>,
}

async fn profile_form(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Query<ProfileQuery>,
) -> AppResult<Html<String>> {
    let p = load_profile(&state.db, author.0.id).await?;
    let form = context! {
        full_name => p.full_name, display_name => p.display_name, pronouns => p.pronouns,
        affiliation => p.affiliation, bio => p.bio,
    };
    render_profile(&author, &jar, &headers, &p.email, p.avatar_key.as_deref(), form, q.welcome.is_some(),
        q.saved.is_some().then_some("Profile saved."), None)
}

#[allow(clippy::too_many_arguments)]
fn render_profile(
    author: &AuthenticatedAuthor,
    jar: &CookieJar,
    headers: &HeaderMap,
    email: &str,
    avatar_key: Option<&str>,
    form: Value,
    welcome: bool,
    saved: Option<&str>,
    error: Option<&str>,
) -> AppResult<Html<String>> {
    let a = &author.0;
    let label = [form.get_attr("display_name").ok(), form.get_attr("full_name").ok()]
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| email.to_string());
    let ctx = context! {
        title => "Your profile",
        form => form,
        email => email,
        me => Value::from_serialize(person(a.id, &label, avatar_key)),
        welcome => welcome,
        saved => saved,
        error => error,
        ..base_ctx(a, &session_token(jar))
    };
    respond(&environment(), headers, "studio_profile.html", ctx)
}

#[derive(Debug, Deserialize)]
pub struct ProfileForm {
    csrf_token: String,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    pronouns: String,
    #[serde(default)]
    affiliation: String,
    #[serde(default)]
    bio: String,
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn profile_submit(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Form(f): Form<ProfileForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    let full_name = clean(&f.full_name);
    let display_name = clean(&f.display_name);
    let pronouns = clean(&f.pronouns);
    let affiliation = clean(&f.affiliation);
    // Keep the author's paragraph breaks in the bio; trim the ends only.
    let bio = f.bio.trim().replace("\r\n", "\n");

    let form = context! {
        full_name => &full_name, display_name => &display_name, pronouns => &pronouns,
        affiliation => &affiliation, bio => &bio,
    };
    let p = load_profile(&state.db, author.0.id).await?;
    let problem = if full_name.is_empty() {
        Some("Enter your full name.")
    } else if full_name.chars().count() > 120 {
        Some("Full name can be at most 120 characters.")
    } else if display_name.chars().count() > 60 {
        Some("Display name can be at most 60 characters.")
    } else if pronouns.chars().count() > 40 {
        Some("Pronouns can be at most 40 characters.")
    } else if affiliation.chars().count() > 120 {
        Some("Affiliation can be at most 120 characters.")
    } else if bio.chars().count() > 600 {
        Some("The bio can be at most 600 characters.")
    } else {
        None
    };
    if let Some(msg) = problem {
        return Ok(render_profile(&author, &jar, &headers, &p.email, p.avatar_key.as_deref(), form, false, None, Some(msg))?
            .into_response());
    }

    sqlx::query(
        "UPDATE authors SET full_name = $2, display_name = $3, pronouns = $4, affiliation = $5, bio = $6 \
         WHERE id = $1",
    )
    .bind(author.0.id)
    .bind(&full_name)
    .bind(&display_name)
    .bind(&pronouns)
    .bind(&affiliation)
    .bind(&bio)
    .execute(&state.db)
    .await?;

    // A full page load, so the header picks up the new name.
    if headers.get("HX-Request").is_some() {
        return Ok(([("HX-Redirect", "/studio/profile?saved=1")], "").into_response());
    }
    Ok(Redirect::to("/studio/profile?saved=1").into_response())
}

// --- the photo ------------------------------------------------------------

/// Takes the image as the raw request body (sent by studio.js), with the
/// CSRF token in a header.
async fn upload_photo(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let token = headers.get("X-CSRF-Token").and_then(|v| v.to_str().ok()).unwrap_or("");
    require_csrf(&jar, token)?;
    if body.is_empty() {
        return Err(AppError::BadRequest("choose a photo".into()));
    }

    let bytes = body.to_vec();
    let webp = tokio::task::spawn_blocking(move || process_photo(&bytes))
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .map_err(AppError::BadRequest)?;

    let key = format!("avatars/{}/{}.webp", author.0.id, Uuid::new_v4().simple());
    state.storage.put(&key, webp, "image/webp").await.map_err(AppError::Internal)?;
    let old: Option<String> = sqlx::query_scalar(
        "UPDATE authors a SET avatar_key = $2 FROM authors o WHERE a.id = $1 AND o.id = a.id \
         RETURNING o.avatar_key",
    )
    .bind(author.0.id)
    .bind(&key)
    .fetch_one(&state.db)
    .await?;
    if let Some(old) = old {
        if let Err(e) = state.storage.delete(&old).await {
            tracing::warn!(error = %e, key = %old, "could not delete replaced profile photo");
        }
    }
    tracing::info!(author = %author.0.id, "profile photo updated");
    photo_fragment(&state, &author, &jar).await
}

async fn remove_photo(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &form.csrf_token)?;
    let old: Option<String> = sqlx::query_scalar(
        "UPDATE authors a SET avatar_key = NULL FROM authors o WHERE a.id = $1 AND o.id = a.id \
         RETURNING o.avatar_key",
    )
    .bind(author.0.id)
    .fetch_one(&state.db)
    .await?;
    if let Some(old) = old {
        if let Err(e) = state.storage.delete(&old).await {
            tracing::warn!(error = %e, key = %old, "could not delete removed profile photo");
        }
    }
    photo_fragment(&state, &author, &jar).await
}

/// The photo card, re-rendered after a change.
async fn photo_fragment(state: &AppState, author: &AuthenticatedAuthor, jar: &CookieJar) -> AppResult<Response> {
    let p = load_profile(&state.db, author.0.id).await?;
    let label = [p.display_name.as_str(), p.full_name.as_str()]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .unwrap_or(&p.email)
        .to_string();
    let html = render(
        &environment(),
        "studio_profile_photo.html",
        context! {
            me => Value::from_serialize(person(author.0.id, &label, p.avatar_key.as_deref())),
            csrf_token => csrf::token_for_session(&session_token(jar)),
        },
    )?;
    // The header shows the photo too.
    Ok(([("HX-Trigger", "profile-photo-changed")], html).into_response())
}

/// The small photo in the page header, refreshed after the photo changes.
async fn header_avatar(author: AuthenticatedAuthor, State(state): State<AppState>) -> AppResult<Html<String>> {
    let key: Option<String> = sqlx::query_scalar("SELECT avatar_key FROM authors WHERE id = $1")
        .bind(author.0.id)
        .fetch_one(&state.db)
        .await?;
    render(
        &environment(),
        "studio_avatar.html",
        context! { me => Value::from_serialize(person(author.0.id, author.0.label(), key.as_deref())) },
    )
}

/// Decodes a JPEG, PNG, WebP, or GIF, applies the camera's rotation, crops
/// the centre square, and returns it as a WebP.
fn process_photo(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let unreadable = || "That file isn't a photo we can read. Use a JPEG, PNG, or WebP.".to_string();
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unreadable())?;
    let mut decoder = reader.into_decoder().map_err(|_| unreadable())?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err("That photo is too large. Try one under 60 megapixels.".into());
    }
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|_| unreadable())?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }

    let side = img.width().min(img.height());
    let x = (img.width() - side) / 2;
    let y = (img.height() - side) / 2;
    let square = img.crop_imm(x, y, side, side);
    let sized = if side > AVATAR_PX {
        square.resize_exact(AVATAR_PX, AVATAR_PX, image::imageops::FilterType::Lanczos3)
    } else {
        square
    };
    let rgba = sized.to_rgba8();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height()).encode(82.0);
    Ok(encoded.to_vec())
}

/// Serves an author's photo to signed-in authors. Long-cached: a changed
/// photo has a different URL (see [`avatar_url`]).
async fn avatar(
    _author: AuthenticatedAuthor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    let key: Option<String> = sqlx::query_scalar("SELECT avatar_key FROM authors WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .flatten();
    let key = key.ok_or(AppError::NotFound("profile photo"))?;
    let bytes = state.storage.get(&key).await.map_err(AppError::Internal)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, "private, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_first_and_last_word() {
        assert_eq!(initials("Tim Richards"), "TR");
        assert_eq!(initials("Mary Ann van der Berg"), "MB");
        assert_eq!(initials("Cher"), "C");
        assert_eq!(initials("ptdunn@example.edu"), "P");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn photos_come_out_square_and_small() {
        let img = image::RgbImage::from_pixel(1200, 800, image::Rgb([200, 30, 30]));
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let webp = process_photo(&png).unwrap();
        let out = image::load_from_memory(&webp).unwrap();
        assert_eq!((out.width(), out.height()), (AVATAR_PX, AVATAR_PX));
    }

    #[test]
    fn rejects_files_that_are_not_images() {
        assert!(process_photo(b"%PDF-1.7 not an image").is_err());
    }
}
