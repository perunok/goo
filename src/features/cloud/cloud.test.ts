import { ago } from './cloud.svelte'

describe('ago', () => {
  const now = 1_800_000_000_000
  it('reads naturally', () => {
    expect(ago(null, now)).toBe('not yet')
    expect(ago(now / 1000 - 10, now)).toBe('just now')
    expect(ago(now / 1000 - 300, now)).toBe('5 min ago')
    expect(ago(now / 1000 - 7200, now)).toBe('2 h ago')
  })
})
