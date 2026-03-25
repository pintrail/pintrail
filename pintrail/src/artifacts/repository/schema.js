'use strict'

// Defines the schema and data transfer objects (dto) for the repository layer
import { z } from 'zod'

const WebpImagePathSchema = z
  .string()
  .regex(/^(\/[\w.-]+)+\.webp$/, 'Must be an absolute path to a .webp file')

const VideoPathSchema = z
  .string()
  .regex(/^(\/[\w.-]+)+\.mp4$/, 'Must be an absolute path to a .mp4 file')

const GeoCoordinatesSchema = z.object({
  lat: z.number().min(-90).max(90),
  lng: z.number().min(-180).max(180),
})

export const ArtifactSchema = z.object({
  id: z.string().optional(),
  name: z.string(),
  desc: z.string(),
  loc: GeoCoordinatesSchema,
  createdAt: z.date(),
  updatedAt: z.date(),
  pid: z.string().default('0'),
})
