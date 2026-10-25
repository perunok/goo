import { ago, formatBytes } from './format'

describe('formatBytes', () => {
  it('uses the largest sensible unit', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(532)).toBe('532 B')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(78_900_000)).toBe('75.2 MB')
    expect(formatBytes(5 * 1024 ** 3)).toBe('5.0 GB')
    expect(formatBytes(-1)).toBe('0 B')
    expect(formatBytes(NaN)).toBe('0 B')
  })
})

describe('ago', () => {
  const now = 1_800_000_000_000
  it('reads naturally', () => {
    expect(ago(null, now)).toBe('not yet')
    expect(ago(now / 1000 - 10, now)).toBe('just now')
    expect(ago(now / 1000 - 300, now)).toBe('5 min ago')
    expect(ago(now / 1000 - 7200, now)).toBe('2 h ago')
  })
})
