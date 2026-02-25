# Artifacts

This document describes the current `Artifact` model and the HTMX admin interface in the NestJS app.

## Overview

An `Artifact` is the core model used to represent a physical thing or place in the system.

Examples:
- A single object (painting, vase, tool)
- A room that contains objects
- A building that contains rooms
- A collection grouping related artifacts

Artifacts support:
- Physical/descriptive location metadata
- Required geolocation for mobile proximity detection
- Rich assets (text, image, video, audio, links)
- Parent/child containment relationships

## Current Entity Model

Primary entity file:
- `src/artifacts/entities/artifact.entity.ts`

Entities:
- `ArtifactEntity` (main record)
- `ArtifactAssetEntity` (assets attached to an artifact)
- Embedded value objects:
  - `ArtifactLocation`
  - `ArtifactGeolocation`

## `ArtifactEntity` Fields

### Identity and classification

- `id: string`
  - UUID primary key.
  - System-generated.

- `kind: ArtifactKind`
  - Type/category of artifact.
  - Enum values:
    - `item`
    - `room`
    - `building`
    - `collection`
    - `other`
  - Used to distinguish physical items vs containers/locations.

- `name: string`
  - Human-readable name.
  - Required.
  - Example: `"North Gallery"` or `"Bronze Vase #12"`.

- `description?: string | null`
  - Optional long-form description.
  - Can store notes describing the artifact itself.

### Tagging and metadata

- `tags: string[]`
  - Free-form tags.
  - Stored as a TypeORM `simple-array`.
  - Useful for filtering, grouping, and lightweight labels.

- `metadata?: Record<string, unknown> | null`
  - Arbitrary JSON metadata.
  - Use for fields not yet modeled explicitly.
  - Good for experimental attributes while the schema evolves.

- `isActive: boolean`
  - Indicates whether the artifact is active/usable in the app.
  - Can be used to hide retired/archived artifacts without deleting them.

### Physical location (descriptive)

- `location: ArtifactLocation`
  - Embedded object for descriptive or human-readable location details.
  - These fields can coexist with geolocation.

`ArtifactLocation` fields:
- `name?: string | null`
  - Display name for a place/location label.
- `addressLine1?: string | null`
  - Street address line 1.
- `addressLine2?: string | null`
  - Street address line 2.
- `city?: string | null`
  - City/locality.
- `stateProvince?: string | null`
  - State/province/region.
- `postalCode?: string | null`
  - Postal/ZIP code.
- `country?: string | null`
  - Country.
- `building?: string | null`
  - Building label/name.
- `floor?: string | null`
  - Floor level.
- `room?: string | null`
  - Room identifier/name.
- `shelf?: string | null`
  - Shelf/cabinet/bin position.
- `notes?: string | null`
  - Free-form location notes.

Important:
- `location` is descriptive and organizational.
- It is not the primary proximity signal for mobile detection.

### Geolocation (critical for mobile proximity)

- `geolocation: ArtifactGeolocation`
  - Embedded object for proximity detection and map-like features.
  - This is the critical location field for the mobile app.

`ArtifactGeolocation` fields:
- `latitude: number`
  - Required.
  - Decimal latitude (`-90` to `90`).
- `longitude: number`
  - Required.
  - Decimal longitude (`-180` to `180`).
- `proximityRadiusMeters: number`
  - Radius used to determine if a mobile device is “near” the artifact.
  - Defaults to `25`.
  - Intended for geofencing/proximity checks.

Service validation (current behavior):
- Create requires `geolocation`
- Latitude must be between `-90` and `90`
- Longitude must be between `-180` and `180`
- `proximityRadiusMeters` must be > `0`

### Hierarchy / containment

- `parent?: ArtifactEntity | null`
  - TypeORM relation to the parent artifact (optional).
  - Example:
    - an `item` may have a parent `room`
    - a `room` may have a parent `building`

- `parentArtifactId?: string | null`
  - Relation ID for the parent artifact.
  - Practical field for APIs/UI without loading the full parent relation.

- `children: ArtifactEntity[]`
  - TypeORM inverse relation for contained artifacts.
  - Current in-memory implementation computes/uses parent IDs and returns children list placeholders.

### Assets

- `assets: ArtifactAssetEntity[]`
  - One-to-many relation of assets associated with the artifact.
  - Supports descriptive content and media.

### Timestamps

- `createdAt: Date`
  - Auto-generated creation timestamp.

- `updatedAt: Date`
  - Auto-generated update timestamp.

## `ArtifactAssetEntity` Fields

Asset entity stores different asset types in one table using nullable fields.

### Identity and relation

- `id: string`
  - UUID primary key for the asset.

- `artifact: ArtifactEntity`
  - Owning artifact relation.

- `artifactId: string`
  - Relation ID for the owning artifact.

### Type and labeling

- `type: ArtifactAssetType`
  - Asset kind enum.
  - Values:
    - `text`
    - `image`
    - `video`
    - `audio`
    - `artifact_link`

- `title?: string | null`
  - Optional asset title.

- `description?: string | null`
  - Optional asset description.

- `sortOrder: number`
  - Sort/display order within an artifact’s assets.

### Text asset fields (used when `type = text`)

- `textContent?: string | null`
  - Text body content.

- `textFormat?: 'plain' | 'markdown' | 'html' | null`
  - Indicates text format.

### Media asset fields (used when `type = image | video | audio`)

- `url?: string | null`
  - URL to the media file/resource.

- `mimeType?: string | null`
  - MIME type (example: `image/jpeg`, `audio/mpeg`).

- `durationSeconds?: number | null`
  - Duration for audio/video.

- `width?: number | null`
  - Pixel width (image/video).

- `height?: number | null`
  - Pixel height (image/video).

- `sizeBytes?: string | null`
  - File size in bytes (stored as string because DB column type is `bigint`).

### Artifact link asset fields (used when `type = artifact_link`)

- `linkedArtifact?: ArtifactEntity | null`
  - Optional relation to another artifact.

- `linkedArtifactId?: string | null`
  - Linked artifact ID.

- `relationship?: string | null`
  - Free-form relationship label.
  - Examples: `"related_to"`, `"duplicate_of"`, `"contains_reference_to"`.

### Asset metadata and timestamps

- `metadata?: Record<string, unknown> | null`
  - Flexible JSON for asset-specific metadata.

- `createdAt: Date`
  - Asset creation timestamp.

- `updatedAt: Date`
  - Asset update timestamp.

## DTO / API Notes (Current CRUD)

Relevant files:
- `src/artifacts/dto/create-artifact.dto.ts`
- `src/artifacts/dto/update-artifact.dto.ts`
- `src/artifacts/artifacts.controller.ts`
- `src/artifacts/artifacts.service.ts`

### Create (`POST /artifacts`)

Current create payload requires:
- `name`
- `geolocation.latitude`
- `geolocation.longitude`

Optional create fields:
- `kind`
- `description`
- `tags`
- `location`
- `parentArtifactId`
- `assets`
- `isActive`
- `metadata`
- `geolocation.proximityRadiusMeters`

### Update (`PATCH /artifacts/:id`)

All update fields are optional, including:
- `geolocation` (if provided, service validates it)

### Delete (`DELETE /artifacts/:id`)

- Deletes artifact
- Current mock repository is in-memory

## Mock Persistence (Current State)

The app currently uses an in-memory repository implementation:
- `src/artifacts/repositories/in-memory-artifact.repository.ts`

Implications:
- No real database required right now
- Data is lost when the Nest server restarts
- Good for UI and workflow development before TypeORM DB wiring is finalized

## HTMX Admin Interface

Primary UI controller:
- `src/artifacts/artifacts-ui.controller.ts`

Page URL:
- `GET /artifacts-admin`

This page is a server-rendered HTMX interface for managing artifacts.

### What the admin UI can do

- View a list of artifacts
- Select an artifact and load its details panel
- Create an artifact
- Edit an artifact
- Delete an artifact
- Add assets of any supported type
- Set an artifact’s parent
- Attach another artifact as a child (implemented by setting the child’s parent)

### UI sections

#### Left panel

- Create Artifact form
  - Includes required geolocation inputs
  - Includes descriptive location fields
- Artifact list
  - Click an artifact to load detail view via HTMX

#### Right panel (detail view)

- Edit Artifact form
- Delete Artifact action
- Hierarchy management
  - Set Parent
  - Add Child
- Asset list
- Add Asset form (all asset types supported through one form)

### HTMX HTML fragment endpoints (Admin UI)

These endpoints return HTML (full page or fragments), not JSON.

- `GET /artifacts-admin`
  - Returns full admin page HTML.

- `GET /artifacts-admin/:id`
  - Returns artifact detail panel HTML for the selected artifact.

- `POST /artifacts-admin/create`
  - Creates an artifact from form data.
  - Returns updated artifact list + detail panel fragments.

- `POST /artifacts-admin/:id/update`
  - Updates an artifact from form data.
  - Returns updated list/detail fragments.

- `POST /artifacts-admin/:id/delete`
  - Deletes the artifact.
  - Returns updated list/detail fragments.

- `POST /artifacts-admin/:id/assets`
  - Adds an asset to the artifact.
  - Returns updated list/detail fragments.

- `POST /artifacts-admin/:id/parent`
  - Sets or clears the artifact’s parent.
  - `parentArtifactId=""` clears parent.
  - Returns updated list/detail fragments.

- `POST /artifacts-admin/:id/children`
  - Attaches a child artifact by setting that child’s parent to this artifact.
  - Form field: `childArtifactId`
  - Returns updated list/detail fragments.

### Form field notes in the admin UI

Artifact create/edit forms use flat HTML field names that the UI controller parses into DTOs.

Examples:
- `name`
- `kind`
- `description`
- `tagsCsv`
- `geoLatitude`
- `geoLongitude`
- `geoRadius`
- `locationBuilding`
- `locationFloor`
- `locationRoom`
- `isActive`

Asset form examples:
- `type`
- `title`
- `description`
- `textContent`
- `textFormat`
- `url`
- `mimeType`
- `durationSeconds`
- `width`
- `height`
- `sizeBytes`
- `linkedArtifactId`
- `relationship`

## Relationship Semantics (Current)

- Parent/child containment is represented by `parentArtifactId`.
- “Add Child” in the admin UI updates the selected child artifact so its `parentArtifactId` points to the current artifact.
- The service prevents an artifact from being its own parent.
- The service verifies referenced parent exists before assignment.

## Future Enhancements (Likely)

- Real TypeORM repository backed by a DB (`pg`, `mysql`, etc.)
- Migrations for `artifacts` and `artifact_assets`
- Proximity query endpoint (e.g. `/artifacts/near`)
- Asset edit/delete operations in the admin UI
- Validation decorators (`class-validator`) on DTOs
- Loop detection for hierarchy (prevent indirect cycles)

