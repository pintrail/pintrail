// In-Memory implementation of the artifact repository.
import { Ok, Err, Some, None } from '#root/src/utility/utility.js'
import { ArtifactValidationError } from './errors.js'
import { ArtifactSchema } from './schema.js'

const uuid = () => crypto.randomUUID()

export const InMemoryArtifactRepository = () => {
  const repo = {}

  return {
    add: artifact => {
      const id = uuid()
      const res = ArtifactSchema.safeParse(artifact)
      if (res.success) {
        const art = { id, ...res.data }
        repo[id] = art
        return Ok(art)
      }
      return Err(ArtifactValidationError(res.error.message, artifact))
    },

    get: id => {
      const art = repo[id]
      if (!art) return None()
      return Some(art)
    },
  }
}
