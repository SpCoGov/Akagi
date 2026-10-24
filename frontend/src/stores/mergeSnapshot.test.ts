import { describe, expect, it } from 'vitest'
import { mergeSnapshot } from './mergeSnapshot'

describe('mergeSnapshot', () => {
  it('keeps live arrivals without duplicating entries already read from disk', () => {
    const first = { ts_ms: 1, message: 'first' }
    const overlap = { ts_ms: 2, message: 'overlap' }
    const arrivedDuringRead = { ts_ms: 3, message: 'live' }

    expect(mergeSnapshot([first, overlap], [overlap, arrivedDuringRead], 10)).toEqual([
      first,
      overlap,
      arrivedDuringRead,
    ])
  })
})
