import {
  IsBoolean,
  IsNumber,
  IsOptional,
  IsString,
  Max,
  Min,
  ValidateIf,
} from 'class-validator';

export class UpdateArtifactDto {
  @IsOptional()
  @IsString()
  name?: string;

  @IsOptional()
  @IsString()
  desc?: string;

  @IsOptional()
  @IsString()
  parentId?: string | null;

  @IsOptional()
  @IsBoolean()
  clearLocation?: boolean;

  @ValidateIf(dto => dto.lat !== undefined || dto.lng !== undefined)
  @IsNumber()
  @Min(-90)
  @Max(90)
  lat?: number;

  @ValidateIf(dto => dto.lat !== undefined || dto.lng !== undefined)
  @IsNumber()
  @Min(-180)
  @Max(180)
  lng?: number;
}
