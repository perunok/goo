import { APPEARANCE_KEY, DEFAULT_APPEARANCE, loadAppearance, sanitizeAppearance, saveAppearance, type KeyValueStore } from './appearance'

const memory = (init: Record<string, string> = {}): KeyValueStore & { data: Record<string, string> } => {
  const data = { ...init }
  return { data, get: (k) => data[k] ?? null, set: (k, v) => void (data[k] = v) }
}

describe('appearance', () => {
  it('defaults when nothing or garbage is stored', () => {
    expect(loadAppearance(memory())).toEqual(DEFAULT_APPEARANCE)
    expect(loadAppearance(memory({ [APPEARANCE_KEY]: '{nope' }))).toEqual(DEFAULT_APPEARANCE)
  })

  it('round-trips', () => {
    const store = memory()
    const a = { theme: 'dracula', accent: 'teal', systemLight: 'paper', systemDark: 'nord', fontSize: 16 }
    saveAppearance(store, a)
    expect(loadAppearance(store)).toEqual(a)
  })

  it('drops unknown themes, accents and out-of-range sizes', () => {
    expect(sanitizeAppearance({ theme: 'nope', accent: 'plaid', systemLight: 'dracula', systemDark: 'paper', fontSize: 99 })).toEqual(DEFAULT_APPEARANCE)
  })
})
