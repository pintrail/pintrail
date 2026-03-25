'use strict'
import { z } from 'zod'
import { Ok, Err } from '#utility/utility.js'

const GeoCoordinatesSchema = z.object({
  lat: z.number().min(-90).max(90),
  lng: z.number().min(-180).max(180),
})

export const ArtifactIn = z.object({
  name: z.string(),
  desc: z.string().default(''),
  loc: GeoCoordinatesSchema.optional(),
})

// This is the artifact service.
export const ArtifactService = (logger, repository) => {
  const log = logger.child({ module: 'ArtfactService' })

  return {
    add: data => {
      log.info('Adding artifact')
      const parsed = ArtifactIn.safeParse(data)
      if (!parsed.success) {
        log.error(`Unable to validate artifact input: ${parsed.error.message}`)
        return Err(`Validation Error: ${parsed.error.message}`)
      }
      const result = repository.add(parsed.data)
      if (result.ok) return result
      log.error(`Unable to add artifact to repository`)
      return Err(`Unable to add artifact to repository: ${result.v}`)
    },

    findById: id => {
      // get from repository
    },
  }
}
