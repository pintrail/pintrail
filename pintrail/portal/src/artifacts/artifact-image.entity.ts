import {
  Column,
  CreateDateColumn,
  Entity,
  JoinColumn,
  ManyToOne,
  PrimaryGeneratedColumn,
  Relation,
  UpdateDateColumn,
} from 'typeorm';
import { ArtifactEntity } from './artifact.entity';

export type ArtifactImageStatus = 'queued' | 'processing' | 'processed' | 'failed';

@Entity({ name: 'artifact_images' })
export class ArtifactImageEntity {
  @PrimaryGeneratedColumn('uuid')
  id!: string;

  @Column()
  artifactId!: string;

  @ManyToOne(() => ArtifactEntity, artifact => artifact.images, { onDelete: 'CASCADE' })
  @JoinColumn({ name: 'artifactId' })
  artifact!: Relation<ArtifactEntity>;

  @Column()
  originalFilename!: string;

  @Column()
  originalMimeType!: string;

  @Column()
  originalStoragePath!: string;

  @Column({ type: 'varchar', length: 32, default: 'queued' })
  status!: ArtifactImageStatus;

  @Column({ type: 'varchar', nullable: true })
  processedFilename!: string | null;

  @Column({ type: 'varchar', nullable: true })
  processedMimeType!: string | null;

  @Column({ type: 'integer', nullable: true })
  width!: number | null;

  @Column({ type: 'integer', nullable: true })
  height!: number | null;

  @Column({ type: 'text', nullable: true })
  errorMessage!: string | null;

  @CreateDateColumn({ type: 'timestamptz' })
  createdAt!: Date;

  @UpdateDateColumn({ type: 'timestamptz' })
  updatedAt!: Date;
}
