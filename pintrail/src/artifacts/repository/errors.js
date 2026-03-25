'use strict'

export const ArtifactValidationError = (msg = '', value = null) => ({
  code: 'Invalid Artifact Object Format',
  msg,
  value,
})
