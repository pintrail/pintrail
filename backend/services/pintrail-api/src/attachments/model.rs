use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "attachment_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
    Pdf,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "processing_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    /// A presigned URL has been issued; the bytes are not in the bucket yet.
    /// The worker must not see this state (see the upload-lifecycle migration).
    PendingUpload,
    Queued,
    Processing,
    Processed,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Attachment {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub kind: AttachmentKind,
    pub position: i32,
    pub caption: Option<String>,
    pub original_filename: String,
    pub original_mime_type: String,
    pub original_storage_key: String,
    pub status: ProcessingStatus,
    pub processed_storage_key: Option<String>,
    pub processed_mime_type: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_seconds: Option<f64>,
    pub size_bytes: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What a client receives for one attachment.
///
/// `url` is a presigned, time-limited link to the processed output when it is
/// ready, falling back to the original while processing is still pending —
/// so a freshly uploaded photo displays immediately rather than as a gap.
/// Storage keys themselves are never exposed; they are internal layout.
#[derive(Debug, Serialize)]
pub struct AttachmentView {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub kind: AttachmentKind,
    pub position: i32,
    pub caption: Option<String>,
    pub original_filename: String,
    pub status: ProcessingStatus,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_seconds: Option<f64>,
    pub size_bytes: Option<i64>,
    pub error_message: Option<String>,
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UploadIntentRequest {
    pub filename: String,
    pub mime_type: String,
    pub caption: Option<String>,
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAttachmentRequest {
    /// Double-Option: absent leaves the caption alone, explicit null clears it.
    #[serde(default, deserialize_with = "crate::serde_util::present")]
    pub caption: Option<Option<String>>,
    pub position: Option<i32>,
}
