/**
 * Persisted appearance settings: theme, accent, which themes 'System' uses, and the base font size.
 * Stored as JSON under `goo.appearance`; public/theme-init.js reads the same key before first paint, so keep the two
 * in step.
 */
import { DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, THEME_DEFAULT_ACCENT, findTheme, isAccent } from './themes'

export const APPEARANCE_KEY = 'goo.appearance'

export const FONT_SIZE_MIN = 11
export const FONT_SIZE_MAX = 22

export interface Appearance {
  /** 'system' or a theme id. */
  theme: string
  /** An accent id, or 'theme' for the theme's own accent. */
  accent: string
  systemLight: string
  systemDark: string
  /** Base UI and editor font size in px. */
  fontSize: number
}

export interface KeyValueStore {
  get(key: string): string | null
  set(key: string, value: string): void
}

export const DEFAULT_APPEARANCE: Appearance = {
  theme: 'system',
  accent: THEME_DEFAULT_ACCENT,
  systemLight: DEFAULT_LIGHT_THEME,
  systemDark: DEFAULT_DARK_THEME,
  fontSize: 14,
}

/** Drops anything unknown (e.g. a theme from a newer version) back to its default. */
export function sanitizeAppearance(raw: Partial<Record<keyof Appearance, unknown>>): Appearance {
  const str = (v: unknown) => (typeof v === 'string' ? v : '')
  const theme = str(raw.theme)
  const accent = str(raw.accent)
  const light = str(raw.systemLight)
  const dark = str(raw.systemDark)
  const size = typeof raw.fontSize === 'number' && Number.isFinite(raw.fontSize) ? Math.round(raw.fontSize) : NaN
  return {
    theme: theme === 'system' || findTheme(theme) ? theme : DEFAULT_APPEARANCE.theme,
    accent: isAccent(accent) ? accent : DEFAULT_APPEARANCE.accent,
    systemLight: findTheme(light)?.scheme === 'light' ? light : DEFAULT_LIGHT_THEME,
    systemDark: findTheme(dark)?.scheme === 'dark' ? dark : DEFAULT_DARK_THEME,
    fontSize: size >= FONT_SIZE_MIN && size <= FONT_SIZE_MAX ? size : DEFAULT_APPEARANCE.fontSize,
  }
}

export function loadAppearance(store: KeyValueStore): Appearance {
  const json = store.get(APPEARANCE_KEY)
  if (json === null) return { ...DEFAULT_APPEARANCE }
  try {
    const parsed: unknown = JSON.parse(json)
    if (parsed && typeof parsed === 'object') return sanitizeAppearance(parsed as Record<string, unknown>)
  } catch {
    /* corrupt: defaults */
  }
  return { ...DEFAULT_APPEARANCE }
}

export function saveAppearance(store: KeyValueStore, a: Appearance): void {
  store.set(APPEARANCE_KEY, JSON.stringify({ v: 1, ...a }))
}

/** localStorage, but never throwing (private windows, blocked storage). */
export const browserStore: KeyValueStore = {
  get(key) {
    try {
      return localStorage.getItem(key)
    } catch {
      return null
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value)
    } catch {
      /* not persisted; the app still works */
    }
  },
}
