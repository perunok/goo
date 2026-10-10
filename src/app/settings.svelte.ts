/**
 * Appearance (theme, accent, font size; lib/appearance.ts) and layout preferences, applied to <html> as
 * `data-theme` / `data-accent` / `--font-size`. `paletteVersion` bumps on every change so views that bake colours
 * in (Mermaid diagrams) know to redraw.
 */
import {
  DEFAULT_APPEARANCE,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  browserStore,
  loadAppearance,
  saveAppearance,
  type Appearance,
} from '../lib/appearance'
import { THEME_DEFAULT_ACCENT, findTheme, resolveTheme } from '../lib/themes'

export type ViewMode = 'edit' | 'split' | 'preview'
const VIEW_MODES: ViewMode[] = ['edit', 'split', 'preview']
const UI_KEY = 'goo.ui'

interface UiPrefs {
  viewMode: ViewMode
  sidebar: boolean
  wrap: boolean
}

const DEFAULT_UI: UiPrefs = { viewMode: 'split', sidebar: true, wrap: true }

function loadUi(): UiPrefs {
  try {
    const raw = JSON.parse(browserStore.get(UI_KEY) ?? '{}') as Partial<UiPrefs>
    return {
      viewMode: VIEW_MODES.includes(raw.viewMode as ViewMode) ? (raw.viewMode as ViewMode) : DEFAULT_UI.viewMode,
      sidebar: typeof raw.sidebar === 'boolean' ? raw.sidebar : DEFAULT_UI.sidebar,
      wrap: typeof raw.wrap === 'boolean' ? raw.wrap : DEFAULT_UI.wrap,
    }
  } catch {
    return { ...DEFAULT_UI }
  }
}

class Settings {
  appearance = $state<Appearance>({ ...DEFAULT_APPEARANCE })
  ui = $state<UiPrefs>({ ...DEFAULT_UI })
  /** The palette actually shown (resolves 'system'). */
  resolvedTheme = $state('dark')
  paletteVersion = $state(0)
  private media: MediaQueryList | null = null

  init() {
    this.appearance = loadAppearance(browserStore)
    this.ui = loadUi()
    this.media = matchMedia('(prefers-color-scheme: light)')
    this.media.addEventListener('change', () => this.apply())
    this.apply()
  }

  get resolvedScheme() {
    return findTheme(this.resolvedTheme)?.scheme ?? 'dark'
  }

  setAppearance(patch: Partial<Appearance>) {
    this.appearance = { ...this.appearance, ...patch }
    saveAppearance(browserStore, this.appearance)
    this.apply()
  }

  bumpFont(delta: number) {
    const size = Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, this.appearance.fontSize + delta))
    if (size !== this.appearance.fontSize) this.setAppearance({ fontSize: size })
  }

  setUi(patch: Partial<UiPrefs>) {
    this.ui = { ...this.ui, ...patch }
    browserStore.set(UI_KEY, JSON.stringify(this.ui))
  }

  cycleViewMode() {
    const i = VIEW_MODES.indexOf(this.ui.viewMode)
    this.setUi({ viewMode: VIEW_MODES[(i + 1) % VIEW_MODES.length]! })
  }

  private apply() {
    const a = this.appearance
    const d = document.documentElement
    this.resolvedTheme = resolveTheme(a.theme, this.media?.matches ?? false, { light: a.systemLight, dark: a.systemDark })
    d.dataset.theme = this.resolvedTheme
    if (a.accent === THEME_DEFAULT_ACCENT) delete d.dataset.accent
    else d.dataset.accent = a.accent
    d.style.setProperty('--font-size', `${a.fontSize}px`)
    this.paletteVersion++
  }
}

export const settings = new Settings()
