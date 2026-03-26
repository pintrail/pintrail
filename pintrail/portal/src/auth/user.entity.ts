import {
  Column,
  CreateDateColumn,
  Entity,
  OneToMany,
  PrimaryGeneratedColumn,
  Relation,
  UpdateDateColumn,
} from 'typeorm';
import { UserSessionEntity } from './user-session.entity';

@Entity({ name: 'users' })
export class UserEntity {
  @PrimaryGeneratedColumn('uuid')
  id!: string;

  @Column({ unique: true })
  email!: string;

  @Column()
  passwordHash!: string;

  @Column({ type: 'varchar', length: 16, default: 'viewer' })
  role!: 'admin' | 'editor' | 'viewer';

  @Column({ default: true })
  isActive!: boolean;

  @OneToMany(() => UserSessionEntity, session => session.user)
  sessions!: Relation<UserSessionEntity[]>;

  @CreateDateColumn({ type: 'timestamptz' })
  createdAt!: Date;

  @UpdateDateColumn({ type: 'timestamptz' })
  updatedAt!: Date;
}
