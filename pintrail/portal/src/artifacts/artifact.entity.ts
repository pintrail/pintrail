import {
  Column,
  CreateDateColumn,
  Entity,
  JoinColumn,
  ManyToOne,
  OneToMany,
  PrimaryGeneratedColumn,
  Relation,
  UpdateDateColumn,
} from 'typeorm';
import { ArtifactImageEntity } from './artifact-image.entity';

@Entity({ name: 'artifacts' })
export class ArtifactEntity {
  @PrimaryGeneratedColumn('uuid')
  id!: string;

  @Column({ default: '' })
  name!: string;

  @Column({ default: '' })
  desc!: string;

  @Column({ type: 'double precision', nullable: true })
  lat!: number | null;

  @Column({ type: 'double precision', nullable: true })
  lng!: number | null;

  @Column({ nullable: true })
  parentId!: string | null;

  @ManyToOne(() => ArtifactEntity, artifact => artifact.children, {
    nullable: true,
    onDelete: 'CASCADE',
  })
  @JoinColumn({ name: 'parentId' })
  parent!: Relation<ArtifactEntity> | null;

  @OneToMany(() => ArtifactEntity, artifact => artifact.parent)
  children!: Relation<ArtifactEntity[]>;

  @OneToMany(() => ArtifactImageEntity, image => image.artifact)
  images!: Relation<ArtifactImageEntity[]>;

  @CreateDateColumn({ type: 'timestamptz' })
  createdAt!: Date;

  @UpdateDateColumn({ type: 'timestamptz' })
  updatedAt!: Date;
}
