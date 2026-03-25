'use strict'

// In-Memory implementation of the artifact repository.
import { Ok, Err, Some, None } from '#root/utility/utility.js'
import { ArtifactValidationError } from './errors.js'
import { ArtifactSchema } from './schema.js'

const uuid = () => crypto.randomUUID()

export const InMemoryArtifactRepository = logger => {
  console.assert(logger !== undefined)
  const log = logger.child({ module: 'InMemoryArtifactRepository' })
  const repo = {}

  return {
    add: artifact => {
      log.info('Adding artifact to repository')
      const id = uuid()
      const res = ArtifactSchema.safeParse(artifact)
      if (res.success) {
        const art = { id, ...res.data }
        repo[id] = art
        log.info(`Artifact ${id} added`)
        return Ok(art)
      }
      log.error(`Failed to create artifact: ${res.error.message}`)
      return Err(ArtifactValidationError(res.error.message, artifact))
    },

    get: id => {
      const art = repo[id]
      if (!art) return None()
      return Some(art)
    },
  }
}
