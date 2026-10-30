import { READING_WIDTH_MAX, READING_WIDTH_MIN, SIDEBAR_WIDTH_DEFAULT, SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN, sanitizeReadingWidth, sanitizeSidebarWidth } from './settings.svelte'

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

describe('sidebar width', () => {
  it('clamps into range and falls back to the default for garbage', () => {
    expect(sanitizeSidebarWidth(300.4)).toBe(300)
    expect(sanitizeSidebarWidth(10)).toBe(SIDEBAR_WIDTH_MIN)
    expect(sanitizeSidebarWidth(99999)).toBe(SIDEBAR_WIDTH_MAX)
    expect(sanitizeSidebarWidth('wide')).toBe(SIDEBAR_WIDTH_DEFAULT)
  })
})
