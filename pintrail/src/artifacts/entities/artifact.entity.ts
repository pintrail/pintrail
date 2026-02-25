import {
  Column,
  CreateDateColumn,
  Entity,
  JoinColumn,
  ManyToOne,
  OneToMany,
  PrimaryGeneratedColumn,
  RelationId,
  UpdateDateColumn,
} from 'typeorm';

export enum ArtifactKind {
  ITEM = 'item',
  ROOM = 'room',
  BUILDING = 'building',
  COLLECTION = 'collection',
  OTHER = 'other',
}

export enum ArtifactAssetType {
  TEXT = 'text',
  IMAGE = 'image',
  VIDEO = 'video',
  AUDIO = 'audio',
  ARTIFACT_LINK = 'artifact_link',
}

// Embedded physical location details for an artifact.
export class ArtifactLocation {
  @Column({ name: 'name', type: 'varchar', length: 255, nullable: true })
  name?: string | null;

  @Column({
    name: 'address_line_1',
    type: 'varchar',
    length: 255,
    nullable: true,
  })
  addressLine1?: string | null;

  @Column({
    name: 'address_line_2',
    type: 'varchar',
    length: 255,
    nullable: true,
  })
  addressLine2?: string | null;

  @Column({ name: 'city', type: 'varchar', length: 120, nullable: true })
  city?: string | null;

  @Column({
    name: 'state_province',
    type: 'varchar',
    length: 120,
    nullable: true,
  })
  stateProvince?: string | null;

  @Column({ name: 'postal_code', type: 'varchar', length: 30, nullable: true })
  postalCode?: string | null;

  @Column({ name: 'country', type: 'varchar', length: 120, nullable: true })
  country?: string | null;

  @Column({ name: 'building', type: 'varchar', length: 120, nullable: true })
  building?: string | null;

  @Column({ name: 'floor', type: 'varchar', length: 60, nullable: true })
  floor?: string | null;

  @Column({ name: 'room', type: 'varchar', length: 120, nullable: true })
  room?: string | null;

  @Column({ name: 'shelf', type: 'varchar', length: 120, nullable: true })
  shelf?: string | null;

  @Column({
    name: 'latitude',
    type: 'decimal',
    precision: 10,
    scale: 7,
    nullable: true,
  })
  latitude?: number | null;

  @Column({
    name: 'longitude',
    type: 'decimal',
    precision: 10,
    scale: 7,
    nullable: true,
  })
  longitude?: number | null;

  @Column({ name: 'notes', type: 'text', nullable: true })
  notes?: string | null;
}

@Entity({ name: 'artifacts' })
export class ArtifactEntity {
  @PrimaryGeneratedColumn('uuid')
  id: string;

  @Column({
    type: 'enum',
    enum: ArtifactKind,
    default: ArtifactKind.ITEM,
  })
  kind: ArtifactKind;

  @Column({ type: 'varchar', length: 255 })
  name: string;

  @Column({ type: 'text', nullable: true })
  description?: string | null;

  // For a starting model this keeps tags simple and database-agnostic.
  @Column({ type: 'simple-array', default: '' })
  tags: string[];

  @Column(() => ArtifactLocation, { prefix: 'location' })
  location: ArtifactLocation;

  @ManyToOne(() => ArtifactEntity, (artifact) => artifact.children, {
    nullable: true,
    onDelete: 'SET NULL',
  })
  @JoinColumn({ name: 'parent_artifact_id' })
  parent?: ArtifactEntity | null;

  @RelationId((artifact: ArtifactEntity) => artifact.parent)
  parentArtifactId?: string | null;

  @OneToMany(() => ArtifactEntity, (artifact) => artifact.parent)
  children: ArtifactEntity[];

  @OneToMany(() => ArtifactAssetEntity, (asset) => asset.artifact, {
    cascade: true,
  })
  assets: ArtifactAssetEntity[];

  @Column({ name: 'is_active', type: 'boolean', default: true })
  isActive: boolean;

  @Column({ type: 'simple-json', nullable: true })
  metadata?: Record<string, unknown> | null;

  @CreateDateColumn({ name: 'created_at' })
  createdAt: Date;

  @UpdateDateColumn({ name: 'updated_at' })
  updatedAt: Date;
}

@Entity({ name: 'artifact_assets' })
export class ArtifactAssetEntity {
  @PrimaryGeneratedColumn('uuid')
  id: string;

  @ManyToOne(() => ArtifactEntity, (artifact) => artifact.assets, {
    nullable: false,
    onDelete: 'CASCADE',
  })
  @JoinColumn({ name: 'artifact_id' })
  artifact: ArtifactEntity;

  @RelationId((asset: ArtifactAssetEntity) => asset.artifact)
  artifactId: string;

  @Column({
    type: 'enum',
    enum: ArtifactAssetType,
  })
  type: ArtifactAssetType;

  @Column({ type: 'varchar', length: 255, nullable: true })
  title?: string | null;

  @Column({ type: 'text', nullable: true })
  description?: string | null;

  @Column({ name: 'sort_order', type: 'int', default: 0 })
  sortOrder: number;

  // TEXT asset fields
  @Column({ name: 'text_content', type: 'text', nullable: true })
  textContent?: string | null;

  @Column({ name: 'text_format', type: 'varchar', length: 20, nullable: true })
  textFormat?: 'plain' | 'markdown' | 'html' | null;

  // IMAGE / VIDEO / AUDIO asset fields
  @Column({ type: 'varchar', length: 2048, nullable: true })
  url?: string | null;

  @Column({ name: 'mime_type', type: 'varchar', length: 255, nullable: true })
  mimeType?: string | null;

  @Column({
    name: 'duration_seconds',
    type: 'decimal',
    precision: 10,
    scale: 3,
    nullable: true,
  })
  durationSeconds?: number | null;

  @Column({ type: 'int', nullable: true })
  width?: number | null;

  @Column({ type: 'int', nullable: true })
  height?: number | null;

  @Column({ name: 'size_bytes', type: 'bigint', nullable: true })
  sizeBytes?: string | null;

  // Link to another artifact (for cross-reference relationships)
  @ManyToOne(() => ArtifactEntity, {
    nullable: true,
    onDelete: 'SET NULL',
  })
  @JoinColumn({ name: 'linked_artifact_id' })
  linkedArtifact?: ArtifactEntity | null;

  @RelationId((asset: ArtifactAssetEntity) => asset.linkedArtifact)
  linkedArtifactId?: string | null;

  @Column({ type: 'varchar', length: 120, nullable: true })
  relationship?: string | null;

  @Column({ type: 'simple-json', nullable: true })
  metadata?: Record<string, unknown> | null;

  @CreateDateColumn({ name: 'created_at' })
  createdAt: Date;

  @UpdateDateColumn({ name: 'updated_at' })
  updatedAt: Date;
}
