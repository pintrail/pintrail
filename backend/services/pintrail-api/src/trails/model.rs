use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::artifacts::model::ArtifactKind;
use crate::identity::OwnerType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "trail_visibility", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TrailVisibility {
    /// Only the owner.
    Private,
    /// Anyone holding the share link.
    Unlisted,
    /// Discoverable by any signed-in user.
    Public,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Trail {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_type: OwnerType,
    pub owner_id: Uuid,
    pub visibility: TrailVisibility,
    pub share_token: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Trail {
    pub fn is_owned_by(&self, owner_type: OwnerType, owner_id: Uuid) -> bool {
        self.owner_type == owner_type && self.owner_id == owner_id
    }
}

/// A trail in a listing. `share_token` is deliberately absent — it appears
/// only in the owner's own detail view, since anyone holding it can read the
/// trail.
#[derive(Debug, Serialize)]
pub struct TrailSummary {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_type: OwnerType,
    pub visibility: TrailVisibility,
    pub stop_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct TrailDetail {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub owner_type: OwnerType,
    pub visibility: TrailVisibility,
    /// Only populated for the owner.
    pub share_token: Option<String>,
    /// Whether the caller may edit this trail.
    pub is_owner: bool,
    pub stops: Vec<TrailStopView>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrailStopRow {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub position: i32,
    pub note: Option<String>,
    pub artifact_name: Option<String>,
    pub artifact_kind: Option<ArtifactKind>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub artifact_deleted: bool,
}

/// A stop as the app sees it.
///
/// When the underlying artifact has been deleted, the stop is still returned
/// with `available: false` rather than being dropped. Silently removing it
/// would renumber someone's trail behind their back and leave them wondering
/// what happened to stop 3; this way the client can show "no longer
/// available" and the owner can decide.
#[derive(Debug, Serialize)]
pub struct TrailStopView {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub position: i32,
    pub note: Option<String>,
    pub available: bool,
    pub artifact_name: Option<String>,
    pub artifact_kind: Option<ArtifactKind>,
    /// Effective coordinates, resolved through the parent chain.
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

impl From<TrailStopRow> for TrailStopView {
    fn from(row: TrailStopRow) -> Self {
        let available = !row.artifact_deleted;
        Self {
            id: row.id,
            artifact_id: row.artifact_id,
            position: row.position,
            note: row.note,
            available,
            // Nothing about a deleted artifact is worth showing.
            artifact_name: available.then_some(row.artifact_name).flatten(),
            artifact_kind: available.then_some(row.artifact_kind).flatten(),
            lat: available.then_some(row.lat).flatten(),
            lng: available.then_some(row.lng).flatten(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTrail {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_visibility")]
    pub visibility: TrailVisibility,
    /// Optional initial stops, in order.
    #[serde(default)]
    pub stops: Vec<StopInput>,
}

fn default_visibility() -> TrailVisibility {
    // A trail someone is still building should not be public by accident.
    TrailVisibility::Private
}

#[derive(Debug, Deserialize)]
pub struct UpdateTrail {
    pub title: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<TrailVisibility>,
}

#[derive(Debug, Deserialize)]
pub struct StopInput {
    pub artifact_id: Uuid,
    pub note: Option<String>,
}

/// The whole ordered list, replacing whatever was there.
#[derive(Debug, Deserialize)]
pub struct ReplaceStops {
    pub stops: Vec<StopInput>,
}
