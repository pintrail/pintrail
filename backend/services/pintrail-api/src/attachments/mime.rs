//! Maps a declared MIME type to an attachment kind and storage extension.
//!
//! The client's filename is never trusted for either. A filename is
//! attacker-controlled and can carry path separators, a misleading extension,
//! or nothing at all; the MIME type is at least validated against this
//! allowlist before anything is stored.

use super::model::AttachmentKind;
use crate::error::{AppError, AppResult};

#[derive(Debug)]
pub struct MediaType {
    pub kind: AttachmentKind,
    pub extension: &'static str,
    pub canonical_mime: &'static str,
}

/// v1 accepts images, PDFs, and plain text. Audio and video are deliberately
/// out of scope until the ffmpeg pipeline lands (docs/DESIGN.md §2.8), and are
/// rejected with a message that says so rather than being stored as files
/// nothing can process.
pub fn classify(mime_type: &str) -> AppResult<MediaType> {
    // Strip any parameters: "text/plain; charset=utf-8" is still text/plain.
    let base = mime_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    let media = match base.as_str() {
        "image/jpeg" | "image/jpg" => MediaType {
            kind: AttachmentKind::Image,
            extension: "jpg",
            canonical_mime: "image/jpeg",
        },
        "image/png" => MediaType {
            kind: AttachmentKind::Image,
            extension: "png",
            canonical_mime: "image/png",
        },
        "image/webp" => MediaType {
            kind: AttachmentKind::Image,
            extension: "webp",
            canonical_mime: "image/webp",
        },
        // iPhones capture HEIC by default, so this is the common case for
        // photos taken on a walk rather than an exotic one.
        "image/heic" => MediaType {
            kind: AttachmentKind::Image,
            extension: "heic",
            canonical_mime: "image/heic",
        },
        "image/heif" => MediaType {
            kind: AttachmentKind::Image,
            extension: "heif",
            canonical_mime: "image/heif",
        },
        "application/pdf" => MediaType {
            kind: AttachmentKind::Pdf,
            extension: "pdf",
            canonical_mime: "application/pdf",
        },
        "text/plain" => MediaType {
            kind: AttachmentKind::Text,
            extension: "txt",
            canonical_mime: "text/plain",
        },

        other if other.starts_with("audio/") || other.starts_with("video/") => {
            return Err(AppError::BadRequest(format!(
                "{other} uploads are not supported yet; audio and video arrive with the \
                 transcoding pipeline in a later release"
            )))
        }

        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported media type {other:?}; accepted: image/jpeg, image/png, \
                 image/webp, image/heic, image/heif, application/pdf, text/plain"
            )))
        }
    };

    Ok(media)
}

/// Trims a client filename down to something safe to store as a label.
///
/// Keeps only the final path component, so `../../etc/passwd` becomes
/// `passwd`. The result is metadata only -- it never contributes to a storage
/// key -- but it is echoed back to clients, so it should not carry surprises.
pub fn sanitize_filename(raw: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('.');

    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();

    if cleaned.is_empty() {
        "upload".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_types() {
        assert_eq!(classify("image/jpeg").unwrap().extension, "jpg");
        assert_eq!(classify("application/pdf").unwrap().kind, AttachmentKind::Pdf);
        assert_eq!(classify("image/heic").unwrap().kind, AttachmentKind::Image);
    }

    #[test]
    fn ignores_parameters_and_case() {
        assert_eq!(classify("TEXT/PLAIN; charset=utf-8").unwrap().extension, "txt");
        assert_eq!(classify("  image/PNG  ").unwrap().extension, "png");
    }

    #[test]
    fn audio_and_video_say_why_they_are_rejected() {
        let err = classify("video/mp4").unwrap_err().to_string();
        assert!(err.contains("not supported yet"), "got {err}");
        assert!(classify("audio/mpeg").is_err());
    }

    #[test]
    fn rejects_unknown_and_dangerous_types() {
        assert!(classify("application/x-sh").is_err());
        assert!(classify("text/html").is_err());
        assert!(classify("").is_err());
    }

    #[test]
    fn filename_sanitizing_strips_paths() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("C:\\temp\\photo.jpg"), "photo.jpg");
        assert_eq!(sanitize_filename("photo.jpg"), "photo.jpg");
    }

    #[test]
    fn filename_sanitizing_handles_empty_and_hidden() {
        assert_eq!(sanitize_filename(""), "upload");
        assert_eq!(sanitize_filename("   "), "upload");
        assert_eq!(sanitize_filename("/"), "upload");
        assert_eq!(sanitize_filename(".hidden"), "hidden");
    }
}
