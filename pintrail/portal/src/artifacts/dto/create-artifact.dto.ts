import { IsOptional, IsString } from 'class-validator';

export class CreateArtifactDto {
  @IsOptional()
  @IsString()
  parentId?: string;
}
