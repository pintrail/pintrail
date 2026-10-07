use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "artifact_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Building,
    Room,
    Artwork,
    Installation,
    Rooftop,
    Other,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Artifact {
    pub id: Uuid,
    pub kind: ArtifactKind,
    pub name: String,
    pub description: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub parent_id: Option<Uuid>,
    pub beacon_id: Option<String>,
    pub sync_version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Full detail for one artifact.
///
/// Reports both the artifact's own coordinates and its effective ones. The
/// client needs the distinction: an author editing a room must see that its
/// position is inherited rather than set, or "clearing" a coordinate that was
/// never there looks like a broken form.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ArtifactDetail {
    pub id: Uuid,
    pub kind: ArtifactKind,
    pub name: String,
    pub description: String,
    /// The artifact's own coordinates. NULL means inherited.
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// Resolved by walking up `parent_id`; equals lat/lng when set directly.
    pub effective_lat: Option<f64>,
    pub effective_lng: Option<f64>,
    /// Which ancestor supplied the effective coordinates, if inherited.
    pub location_source_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub beacon_id: Option<String>,
    /// A shared page many artifacts link to (LEED certification, a bike
    /// share system), not a place. Never has coordinates or a parent.
    pub is_topic: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One end of an artifact-topic link, as listed on the other end's detail.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TopicLink {
    pub id: Uuid,
    pub name: String,
    /// How the topic applies to this artifact, e.g. "Gold, 2019".
    pub note: String,
}

/// One entry in the `/artifacts/sync` manifest.
///
/// Deliberately minimal — this is downloaded for the whole campus and held on
/// device. Media and descriptions are fetched lazily per artifact
/// (docs/DESIGN.md §2.4).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SyncEntry {
    pub id: Uuid,
    pub kind: ArtifactKind,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub beacon_id: Option<String>,
    pub is_topic: bool,
    /// The topics this artifact links to (empty for a topic). The topic's
    /// own entry carries its name; its description comes with its detail.
    pub topic_ids: Vec<Uuid>,
    pub sync_version: i64,
    /// A tombstone. The client evicts this id from its local cache; without
    /// it, a deleted artifact would keep geofencing forever.
    pub deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateArtifact {
    #[serde(default = "default_kind")]
    pub kind: ArtifactKind,
    /// Create a topic rather than a place. A topic takes no coordinates,
    /// parent, or beacon.
    #[serde(default)]
    pub is_topic: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub parent_id: Option<Uuid>,
    pub beacon_id: Option<String>,
}

/// Partial update.
///
/// Every field is `Option`, and coordinates are doubly so: `None` means "leave
/// alone" while `Some(None)` means "clear this, inherit from the parent
/// instead". Collapsing those two would make it impossible to ever un-set a
/// coordinate.
#[derive(Debug, Deserialize)]
pub struct UpdateArtifact {
    pub kind: Option<ArtifactKind>,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "crate::serde_util::present")]
    pub lat: Option<Option<f64>>,
    #[serde(default, deserialize_with = "crate::serde_util::present")]
    pub lng: Option<Option<f64>>,
    #[serde(default, deserialize_with = "crate::serde_util::present")]
    pub parent_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "crate::serde_util::present")]
    pub beacon_id: Option<Option<String>>,
}

fn default_kind() -> ArtifactKind {
    ArtifactKind::Other
}
