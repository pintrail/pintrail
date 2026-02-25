import {
  BadRequestException,
  Body,
  Controller,
  Get,
  HttpCode,
  Param,
  Post,
} from '@nestjs/common';
import {
  ArtifactAssetType,
  ArtifactEntity,
  ArtifactKind,
} from './entities/artifact.entity';
import { ArtifactsService } from './artifacts.service';
import {
  CreateArtifactAssetDto,
  CreateArtifactDto,
} from './dto/create-artifact.dto';
import { UpdateArtifactDto } from './dto/update-artifact.dto';

type FormBody = Record<string, string | undefined>;

@Controller('artifacts-admin')
export class ArtifactsUiController {
  constructor(private readonly artifactsService: ArtifactsService) {}

  @Get()
  async page(): Promise<string> {
    const artifacts = await this.artifactsService.findAll();
    const selected = artifacts[0] ?? null;

    return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Artifact Admin</title>
  <script src="https://unpkg.com/htmx.org@1.9.12"></script>
  <style>
    :root { --bg:#f5f4ef; --panel:#ffffff; --ink:#1f2937; --muted:#6b7280; --line:#d1d5db; --accent:#0f766e; --danger:#b91c1c; }
    * { box-sizing:border-box; }
    body { margin:0; font-family: ui-sans-serif, system-ui, sans-serif; background:linear-gradient(180deg,#f8fafc,#f3efe2); color:var(--ink); }
    .shell { display:grid; grid-template-columns: 360px 1fr; gap:16px; padding:16px; min-height:100vh; }
    .panel { background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:14px; box-shadow:0 4px 18px rgba(15,23,42,0.04); }
    h1,h2,h3 { margin:0 0 8px; }
    h1 { font-size:1.1rem; }
    h2 { font-size:1rem; }
    h3 { font-size:.95rem; }
    .muted { color:var(--muted); font-size:.85rem; }
    .stack { display:grid; gap:10px; }
    .row { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
    .row3 { display:grid; grid-template-columns:1fr 1fr 1fr; gap:8px; }
    label { display:grid; gap:4px; font-size:.85rem; }
    input, select, textarea, button { width:100%; border:1px solid var(--line); border-radius:8px; padding:8px 10px; font:inherit; }
    textarea { min-height:84px; resize:vertical; }
    button { background:#fff; cursor:pointer; }
    button.primary { background:var(--accent); border-color:var(--accent); color:#fff; }
    button.danger { color:#fff; background:var(--danger); border-color:var(--danger); }
    .list { display:grid; gap:8px; max-height:36vh; overflow:auto; }
    .item { border:1px solid var(--line); border-radius:10px; padding:10px; background:#fafafa; }
    .item a { color:inherit; text-decoration:none; display:block; }
    .pill { display:inline-block; border:1px solid var(--line); border-radius:999px; padding:2px 8px; font-size:.75rem; margin-right:4px; }
    .sep { height:1px; background:var(--line); margin:8px 0; }
    .assets { display:grid; gap:8px; }
    .asset { border:1px solid #e5e7eb; border-radius:8px; padding:8px; background:#fcfcfc; }
    @media (max-width: 900px) { .shell { grid-template-columns:1fr; } }
  </style>
</head>
<body>
  <div class="shell">
    <aside class="stack">
      <section class="panel">
        <h1>Artifacts Admin</h1>
        <p class="muted">Create, edit, delete artifacts. Manage assets and parent/child links.</p>
      </section>
      <section class="panel">
        <h2>Create Artifact</h2>
        ${this.renderCreateForm()}
      </section>
      <section class="panel">
        <h2>Artifacts</h2>
        <div id="artifact-list">${this.renderArtifactList(artifacts, selected?.id ?? null)}</div>
      </section>
    </aside>
    <main class="stack">
      <section id="artifact-detail" class="panel">
        ${selected ? this.renderArtifactDetail(selected, artifacts) : this.renderEmptyDetail()}
      </section>
    </main>
  </div>
</body>
</html>`;
  }

  @Get(':id')
  async detail(@Param('id') id: string): Promise<string> {
    const [artifact, artifacts] = await Promise.all([
      this.artifactsService.findOne(id),
      this.artifactsService.findAll(),
    ]);
    return this.renderArtifactDetail(artifact, artifacts);
  }

  @Post('create')
  async create(@Body() body: FormBody): Promise<string> {
    const dto = this.parseCreateArtifactForm(body);
    const created = await this.artifactsService.create(dto);
    return this.renderMutationResponse(created.id);
  }

  @Post(':id/update')
  async update(
    @Param('id') id: string,
    @Body() body: FormBody,
  ): Promise<string> {
    const dto = this.parseUpdateArtifactForm(body);
    await this.artifactsService.update(id, dto);
    return this.renderMutationResponse(id);
  }

  @Post(':id/assets')
  async addAsset(
    @Param('id') id: string,
    @Body() body: FormBody,
  ): Promise<string> {
    const dto = this.parseAssetForm(body);
    await this.artifactsService.addAsset(id, dto);
    return this.renderMutationResponse(id);
  }

  @Post(':id/parent')
  async setParent(
    @Param('id') id: string,
    @Body() body: FormBody,
  ): Promise<string> {
    const parentId = this.readString(body.parentArtifactId);
    await this.artifactsService.setParent(id, parentId || null);
    return this.renderMutationResponse(id);
  }

  @Post(':id/children')
  async addChild(
    @Param('id') id: string,
    @Body() body: FormBody,
  ): Promise<string> {
    const childId = this.readRequiredString(
      body.childArtifactId,
      'childArtifactId',
    );
    await this.artifactsService.addChild(id, childId);
    return this.renderMutationResponse(id);
  }

  @Post(':id/delete')
  @HttpCode(200)
  async remove(@Param('id') id: string): Promise<string> {
    await this.artifactsService.remove(id);
    return this.renderMutationResponse(null);
  }

  private async renderMutationResponse(
    selectedId: string | null,
  ): Promise<string> {
    const artifacts = await this.artifactsService.findAll();
    const nextSelectedId =
      selectedId && artifacts.some((artifact) => artifact.id === selectedId)
        ? selectedId
        : (artifacts[0]?.id ?? null);

    const selected = nextSelectedId
      ? (artifacts.find((artifact) => artifact.id === nextSelectedId) ?? null)
      : null;

    return `
<div id="artifact-list" hx-swap-oob="innerHTML">${this.renderArtifactList(artifacts, nextSelectedId)}</div>
<section id="artifact-detail" class="panel">${selected ? this.renderArtifactDetail(selected, artifacts) : this.renderEmptyDetail()}</section>`;
  }

  private renderCreateForm(): string {
    return `<form class="stack" hx-post="/artifacts-admin/create" hx-target="#artifact-detail" hx-swap="outerHTML">
      ${this.renderArtifactCoreFields()}
      <button class="primary" type="submit">Create Artifact</button>
    </form>`;
  }

  private renderArtifactList(
    artifacts: ArtifactEntity[],
    selectedId: string | null,
  ): string {
    if (artifacts.length === 0) {
      return '<p class="muted">No artifacts yet.</p>';
    }

    return `<div class="list">
      ${artifacts
        .map((artifact) => {
          const isSelected = artifact.id === selectedId;
          const parentBadge = artifact.parentArtifactId
            ? `<span class="pill">child</span>`
            : '';
          return `<div class="item" style="${
            isSelected ? 'border-color: var(--accent); background:#f0fdfa;' : ''
          }">
            <a hx-get="/artifacts-admin/${this.escapeAttr(artifact.id)}" hx-target="#artifact-detail" hx-swap="outerHTML">
              <strong>${this.escapeHtml(artifact.name)}</strong>
              <div class="muted">${this.escapeHtml(artifact.kind)} ${parentBadge}</div>
              <div class="muted">Geo: ${artifact.geolocation?.latitude ?? '?'} , ${artifact.geolocation?.longitude ?? '?'}</div>
            </a>
          </div>`;
        })
        .join('')}
    </div>`;
  }

  private renderEmptyDetail(): string {
    return `<section id="artifact-detail" class="panel">
      <h2>No artifact selected</h2>
      <p class="muted">Create an artifact or choose one from the list.</p>
    </section>`;
  }

  private renderArtifactDetail(
    artifact: ArtifactEntity,
    allArtifacts: ArtifactEntity[],
  ): string {
    const parent = artifact.parentArtifactId
      ? (allArtifacts.find(
          (candidate) => candidate.id === artifact.parentArtifactId,
        ) ?? null)
      : null;
    const children = allArtifacts.filter(
      (candidate) => candidate.parentArtifactId === artifact.id,
    );
    const otherArtifacts = allArtifacts.filter(
      (candidate) => candidate.id !== artifact.id,
    );

    return `<section id="artifact-detail" class="panel stack">
      <div>
        <h2>${this.escapeHtml(artifact.name)}</h2>
        <p class="muted">ID: ${this.escapeHtml(artifact.id)}</p>
      </div>

      <div class="sep"></div>
      <div class="stack">
        <h3>Edit Artifact</h3>
        <form class="stack" hx-post="/artifacts-admin/${this.escapeAttr(artifact.id)}/update" hx-target="#artifact-detail" hx-swap="outerHTML">
          ${this.renderArtifactCoreFields(artifact)}
          <div class="row">
            <button class="primary" type="submit">Save Changes</button>
            <button class="danger" type="submit" formaction="/artifacts-admin/${this.escapeAttr(artifact.id)}/delete" hx-post="/artifacts-admin/${this.escapeAttr(artifact.id)}/delete" hx-target="#artifact-detail" hx-swap="outerHTML">Delete Artifact</button>
          </div>
        </form>
      </div>

      <div class="sep"></div>
      <div class="stack">
        <h3>Hierarchy</h3>
        <div class="muted">Parent: ${parent ? this.escapeHtml(parent.name) : 'None'}</div>
        <div class="muted">Children: ${children.length > 0 ? children.map((child) => this.escapeHtml(child.name)).join(', ') : 'None'}</div>
        <form class="stack" hx-post="/artifacts-admin/${this.escapeAttr(artifact.id)}/parent" hx-target="#artifact-detail" hx-swap="outerHTML">
          <label>Set Parent
            <select name="parentArtifactId">
              <option value="">No parent</option>
              ${otherArtifacts
                .map(
                  (candidate) =>
                    `<option value="${this.escapeAttr(candidate.id)}" ${
                      artifact.parentArtifactId === candidate.id
                        ? 'selected'
                        : ''
                    }>${this.escapeHtml(candidate.name)}</option>`,
                )
                .join('')}
            </select>
          </label>
          <button type="submit">Save Parent</button>
        </form>
        <form class="stack" hx-post="/artifacts-admin/${this.escapeAttr(artifact.id)}/children" hx-target="#artifact-detail" hx-swap="outerHTML">
          <label>Add Child
            <select name="childArtifactId">
              <option value="">Select artifact</option>
              ${otherArtifacts
                .map(
                  (candidate) =>
                    `<option value="${this.escapeAttr(candidate.id)}">${this.escapeHtml(candidate.name)}</option>`,
                )
                .join('')}
            </select>
          </label>
          <button type="submit">Attach Child</button>
        </form>
      </div>

      <div class="sep"></div>
      <div class="stack">
        <h3>Assets (${artifact.assets?.length ?? 0})</h3>
        <div class="assets">
          ${
            (artifact.assets ?? [])
              .map(
                (asset) => `<div class="asset">
                <div><strong>${this.escapeHtml(asset.type)}</strong> ${asset.title ? `- ${this.escapeHtml(asset.title)}` : ''}</div>
                ${asset.description ? `<div class="muted">${this.escapeHtml(asset.description)}</div>` : ''}
                ${asset.textContent ? `<div class="muted">Text: ${this.escapeHtml(asset.textContent.slice(0, 120))}</div>` : ''}
                ${asset.url ? `<div class="muted">URL: ${this.escapeHtml(asset.url)}</div>` : ''}
                ${asset.linkedArtifactId ? `<div class="muted">Linked artifact: ${this.escapeHtml(asset.linkedArtifactId)}</div>` : ''}
              </div>`,
              )
              .join('') || '<p class="muted">No assets yet.</p>'
          }
        </div>
        <form class="stack" hx-post="/artifacts-admin/${this.escapeAttr(artifact.id)}/assets" hx-target="#artifact-detail" hx-swap="outerHTML">
          <div class="row3">
            <label>Type
              <select name="type">
                ${Object.values(ArtifactAssetType)
                  .map(
                    (value) =>
                      `<option value="${this.escapeAttr(value)}">${this.escapeHtml(value)}</option>`,
                  )
                  .join('')}
              </select>
            </label>
            <label>Title <input name="title" /></label>
            <label>Sort Order <input name="sortOrder" type="number" /></label>
          </div>
          <label>Description <input name="description" /></label>
          <label>Text Content <textarea name="textContent"></textarea></label>
          <div class="row3">
            <label>Text Format
              <select name="textFormat">
                <option value="">(none)</option>
                <option value="plain">plain</option>
                <option value="markdown">markdown</option>
                <option value="html">html</option>
              </select>
            </label>
            <label>URL <input name="url" /></label>
            <label>MIME Type <input name="mimeType" /></label>
          </div>
          <div class="row3">
            <label>Duration (s) <input name="durationSeconds" type="number" step="0.001" /></label>
            <label>Width <input name="width" type="number" /></label>
            <label>Height <input name="height" type="number" /></label>
          </div>
          <div class="row3">
            <label>Size Bytes <input name="sizeBytes" /></label>
            <label>Linked Artifact ID <input name="linkedArtifactId" /></label>
            <label>Relationship <input name="relationship" /></label>
          </div>
          <button type="submit">Add Asset</button>
        </form>
      </div>
    </section>`;
  }

  private renderArtifactCoreFields(artifact?: ArtifactEntity): string {
    const tags = (artifact?.tags ?? []).join(', ');
    return `
      <div class="row">
        <label>Name
          <input name="name" value="${this.escapeAttr(artifact?.name ?? '')}" required />
        </label>
        <label>Kind
          <select name="kind">
            ${Object.values(ArtifactKind)
              .map(
                (kind) =>
                  `<option value="${this.escapeAttr(kind)}" ${
                    (artifact?.kind ?? ArtifactKind.ITEM) === kind
                      ? 'selected'
                      : ''
                  }>${this.escapeHtml(kind)}</option>`,
              )
              .join('')}
          </select>
        </label>
      </div>
      <label>Description
        <textarea name="description">${this.escapeHtml(artifact?.description ?? '')}</textarea>
      </label>
      <label>Tags (comma separated)
        <input name="tagsCsv" value="${this.escapeAttr(tags)}" />
      </label>
      <div class="row3">
        <label>Latitude
          <input name="geoLatitude" type="number" step="0.0000001" value="${this.escapeAttr(String(artifact?.geolocation?.latitude ?? ''))}" required />
        </label>
        <label>Longitude
          <input name="geoLongitude" type="number" step="0.0000001" value="${this.escapeAttr(String(artifact?.geolocation?.longitude ?? ''))}" required />
        </label>
        <label>Proximity Radius (m)
          <input name="geoRadius" type="number" step="1" value="${this.escapeAttr(String(artifact?.geolocation?.proximityRadiusMeters ?? 25))}" />
        </label>
      </div>
      <div class="row3">
        <label>Building <input name="locationBuilding" value="${this.escapeAttr(artifact?.location?.building ?? '')}" /></label>
        <label>Floor <input name="locationFloor" value="${this.escapeAttr(artifact?.location?.floor ?? '')}" /></label>
        <label>Room <input name="locationRoom" value="${this.escapeAttr(artifact?.location?.room ?? '')}" /></label>
      </div>
      <div class="row">
        <label>Location Name <input name="locationName" value="${this.escapeAttr(artifact?.location?.name ?? '')}" /></label>
        <label>Country <input name="locationCountry" value="${this.escapeAttr(artifact?.location?.country ?? '')}" /></label>
      </div>
      <div class="row">
        <label>City <input name="locationCity" value="${this.escapeAttr(artifact?.location?.city ?? '')}" /></label>
        <label>State/Province <input name="locationStateProvince" value="${this.escapeAttr(artifact?.location?.stateProvince ?? '')}" /></label>
      </div>
      <label>Address Line 1 <input name="locationAddressLine1" value="${this.escapeAttr(artifact?.location?.addressLine1 ?? '')}" /></label>
      <label>Address Line 2 <input name="locationAddressLine2" value="${this.escapeAttr(artifact?.location?.addressLine2 ?? '')}" /></label>
      <div class="row3">
        <label>Postal Code <input name="locationPostalCode" value="${this.escapeAttr(artifact?.location?.postalCode ?? '')}" /></label>
        <label>Shelf <input name="locationShelf" value="${this.escapeAttr(artifact?.location?.shelf ?? '')}" /></label>
        <label>Active
          <select name="isActive">
            <option value="true" ${artifact?.isActive === false ? '' : 'selected'}>true</option>
            <option value="false" ${artifact?.isActive === false ? 'selected' : ''}>false</option>
          </select>
        </label>
      </div>
      <label>Location Notes
        <textarea name="locationNotes">${this.escapeHtml(artifact?.location?.notes ?? '')}</textarea>
      </label>`;
  }

  private parseCreateArtifactForm(body: FormBody): CreateArtifactDto {
    return {
      name: this.readRequiredString(body.name, 'name'),
      kind: this.parseArtifactKind(body.kind),
      description: this.readString(body.description) ?? undefined,
      tags: this.parseTags(body.tagsCsv),
      geolocation: {
        latitude: this.readRequiredNumber(body.geoLatitude, 'geoLatitude'),
        longitude: this.readRequiredNumber(body.geoLongitude, 'geoLongitude'),
        proximityRadiusMeters: this.readNumber(body.geoRadius) ?? 25,
      },
      location: this.parseLocation(body),
      isActive: this.readBoolean(body.isActive, true),
    };
  }

  private parseUpdateArtifactForm(body: FormBody): UpdateArtifactDto {
    return {
      name: this.readRequiredString(body.name, 'name'),
      kind: this.parseArtifactKind(body.kind),
      description: this.readString(body.description) ?? null,
      tags: this.parseTags(body.tagsCsv),
      geolocation: {
        latitude: this.readRequiredNumber(body.geoLatitude, 'geoLatitude'),
        longitude: this.readRequiredNumber(body.geoLongitude, 'geoLongitude'),
        proximityRadiusMeters: this.readNumber(body.geoRadius) ?? 25,
      },
      location: this.parseLocation(body),
      isActive: this.readBoolean(body.isActive, true),
    };
  }

  private parseAssetForm(body: FormBody): CreateArtifactAssetDto {
    return {
      type: this.parseAssetType(body.type),
      title: this.readString(body.title) ?? undefined,
      description: this.readString(body.description) ?? undefined,
      sortOrder: this.readNumber(body.sortOrder) ?? undefined,
      textContent: this.readString(body.textContent) ?? undefined,
      textFormat:
        (this.readString(body.textFormat) as
          | 'plain'
          | 'markdown'
          | 'html'
          | null) ?? undefined,
      url: this.readString(body.url) ?? undefined,
      mimeType: this.readString(body.mimeType) ?? undefined,
      durationSeconds: this.readNumber(body.durationSeconds) ?? undefined,
      width: this.readInt(body.width) ?? undefined,
      height: this.readInt(body.height) ?? undefined,
      sizeBytes: this.readString(body.sizeBytes) ?? undefined,
      linkedArtifactId: this.readString(body.linkedArtifactId) ?? undefined,
      relationship: this.readString(body.relationship) ?? undefined,
    };
  }

  private parseLocation(body: FormBody): CreateArtifactDto['location'] {
    return {
      name: this.readString(body.locationName) ?? undefined,
      addressLine1: this.readString(body.locationAddressLine1) ?? undefined,
      addressLine2: this.readString(body.locationAddressLine2) ?? undefined,
      city: this.readString(body.locationCity) ?? undefined,
      stateProvince: this.readString(body.locationStateProvince) ?? undefined,
      postalCode: this.readString(body.locationPostalCode) ?? undefined,
      country: this.readString(body.locationCountry) ?? undefined,
      building: this.readString(body.locationBuilding) ?? undefined,
      floor: this.readString(body.locationFloor) ?? undefined,
      room: this.readString(body.locationRoom) ?? undefined,
      shelf: this.readString(body.locationShelf) ?? undefined,
      notes: this.readString(body.locationNotes) ?? undefined,
    };
  }

  private parseTags(value: string | undefined): string[] {
    const raw = this.readString(value);
    if (!raw) {
      return [];
    }
    return raw
      .split(',')
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0);
  }

  private parseArtifactKind(value: string | undefined): ArtifactKind {
    const candidate = this.readString(value);
    if (!candidate) {
      return ArtifactKind.ITEM;
    }
    if (Object.values(ArtifactKind).includes(candidate as ArtifactKind)) {
      return candidate as ArtifactKind;
    }
    return ArtifactKind.ITEM;
  }

  private parseAssetType(value: string | undefined): ArtifactAssetType {
    const candidate = this.readString(value);
    if (
      !candidate ||
      !Object.values(ArtifactAssetType).includes(candidate as ArtifactAssetType)
    ) {
      return ArtifactAssetType.TEXT;
    }
    return candidate as ArtifactAssetType;
  }

  private readString(value: string | undefined): string | null {
    if (value === undefined) {
      return null;
    }
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  private readRequiredString(value: string | undefined, field: string): string {
    const normalized = this.readString(value);
    if (!normalized) {
      throw new BadRequestException(`${field} is required`);
    }
    return normalized;
  }

  private readNumber(value: string | undefined): number | null {
    const normalized = this.readString(value);
    if (!normalized) {
      return null;
    }
    const parsed = Number(normalized);
    return Number.isFinite(parsed) ? parsed : null;
  }

  private readRequiredNumber(value: string | undefined, field: string): number {
    const parsed = this.readNumber(value);
    if (parsed === null) {
      throw new BadRequestException(`${field} must be a number`);
    }
    return parsed;
  }

  private readInt(value: string | undefined): number | null {
    const parsed = this.readNumber(value);
    return parsed === null ? null : Math.trunc(parsed);
  }

  private readBoolean(value: string | undefined, fallback: boolean): boolean {
    const normalized = this.readString(value);
    if (normalized === null) {
      return fallback;
    }
    return normalized === 'true';
  }

  private escapeHtml(value: string): string {
    return value
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }

  private escapeAttr(value: string): string {
    return this.escapeHtml(value);
  }
}
