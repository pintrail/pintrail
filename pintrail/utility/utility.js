'use strict'

// Result Type
export const Ok = v => ({ ok: true, v })
export const Err = v => ({ ok: false, v })

/**
 * Represents the absence of a value.
 *
 * @returns {{
 *   isSome:  () => false,
 *   isNone:  () => true,
 *   getOrElse: (defaultValue: any) => any,
 *   map:     (fn: Function) => None,
 *   flatMap: (fn: Function) => None,
 *   filter:  (fn: Function) => None,
 *   toString: () => string,
 * }}
 */
export const None = () => ({
  isSome: () => false,
  isNone: () => true,
  getOrElse: defaultValue => defaultValue,
  map: _fn => None(),
  flatMap: _fn => None(),
  filter: _fn => None(),
  toString: () => 'None',
})

/**
 * Represents the presence of a value.
 *
 * @param {*} value - The wrapped value
 *
 * @returns {{
 *   isSome:    () => true,
 *   isNone:    () => false,
 *   get:       () => any,
 *   getOrElse: (defaultValue: any) => any,
 *   map:       (fn: Function) => Some|None,
 *   flatMap:   (fn: Function) => Some|None,
 *   filter:    (fn: Function) => Some|None,
 *   toString:  () => string,
 * }}
 */
export const Some = value => ({
  isSome: () => true,
  isNone: () => false,
  get: () => value,
  getOrElse: _defaultValue => value,
  map: fn => Some(fn(value)),
  flatMap: fn => fn(value),
  filter: fn => (fn(value) ? Some(value) : None()),
  toString: () => `Some(${value})`,
})

/**
 * Wraps a throwing function in a Result.
 * @param {Function} fn - A function that may throw
 * @returns {Ok|Err}
 */
export const tryCatch = fn => {
  try {
    return Ok(fn())
  } catch (e) {
    return Err(e.message)
  }
}
