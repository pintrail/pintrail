'use strict'

import pino from 'pino'

const defaultOptions = {
  level: 'info',
}

const wrapLogger = logger => {
  return {
    trace: (...args) => logger.trace(...args),
    debug: (...args) => logger.debug(...args),
    info: (...args) => logger.info(...args),
    warn: (...args) => logger.warn(...args),
    error: (...args) => logger.error(...args),
    fatal: (...args) => logger.fatal(...args),
    child: bindings => wrapLogger(logger.child(bindings)),
  }
}

export const Logger = (options = {}) =>
  wrapLogger(
    pino({
      ...defaultOptions,
      ...options,
    }),
  )
