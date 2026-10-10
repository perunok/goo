import { READING_WIDTH_MAX, READING_WIDTH_MIN, sanitizeReadingWidth } from './settings.svelte'

describe('reading width', () => {
  it('keeps 0 (full width) and clamps everything else into range', () => {
    expect(sanitizeReadingWidth(0)).toBe(0)
    expect(sanitizeReadingWidth(900.4)).toBe(900)
    expect(sanitizeReadingWidth(10)).toBe(READING_WIDTH_MIN)
    expect(sanitizeReadingWidth(99999)).toBe(READING_WIDTH_MAX)
  })

  it('falls back to the default for garbage', () => {
    expect(sanitizeReadingWidth('wide')).toBe(820)
    expect(sanitizeReadingWidth(NaN)).toBe(820)
    expect(sanitizeReadingWidth(undefined)).toBe(820)
  })
})
