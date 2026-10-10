/**
 * Theme and accent registry (shared with Slinger, so a palette tuned in one app can be copied to the other). The colours themselves live only in src/styles/themes.css:
 * `[data-theme='<id>']` blocks hold complete palettes, `[data-accent='<id>']` blocks hold accent colours that are
 * layered on top of any theme. Add a palette there and register it here (the theme tests check both sides agree).
 */
export type Scheme = 'light' | 'dark'

export interface ThemeInfo {
  id: string
  label: string
  scheme: Scheme
  /** High-contrast themes are held to WCAG AAA (7:1) instead of AA. */
  highContrast?: boolean
}

export const THEMES: ThemeInfo[] = [
  // Light
  { id: 'light', label: 'Light', scheme: 'light' },
  { id: 'paper', label: 'Paper', scheme: 'light' },
  { id: 'github-light', label: 'GitHub Light', scheme: 'light' },
  { id: 'solarized-light', label: 'Solarized Light', scheme: 'light' },
  { id: 'gruvbox-light', label: 'Gruvbox Light', scheme: 'light' },
  { id: 'catppuccin-latte', label: 'Catppuccin Latte', scheme: 'light' },
  { id: 'rose-pine-dawn', label: 'Rosé Pine Dawn', scheme: 'light' },
  { id: 'one-light', label: 'One Light', scheme: 'light' },
  { id: 'nord-light', label: 'Nord Light', scheme: 'light' },
  { id: 'ayu-light', label: 'Ayu Light', scheme: 'light' },
  { id: 'contrast-light', label: 'High Contrast Light', scheme: 'light', highContrast: true },
  // Dark
  { id: 'dark', label: 'Dark', scheme: 'dark' },
  { id: 'midnight', label: 'Midnight', scheme: 'dark' },
  { id: 'solarized', label: 'Solarized Dark', scheme: 'dark' },
  { id: 'github-dark', label: 'GitHub Dark', scheme: 'dark' },
  { id: 'dracula', label: 'Dracula', scheme: 'dark' },
  { id: 'nord', label: 'Nord', scheme: 'dark' },
  { id: 'gruvbox-dark', label: 'Gruvbox Dark', scheme: 'dark' },
  { id: 'catppuccin-mocha', label: 'Catppuccin Mocha', scheme: 'dark' },
  { id: 'catppuccin-frappe', label: 'Catppuccin Frappé', scheme: 'dark' },
  { id: 'tokyo-night', label: 'Tokyo Night', scheme: 'dark' },
  { id: 'one-dark', label: 'One Dark', scheme: 'dark' },
  { id: 'monokai', label: 'Monokai', scheme: 'dark' },
  { id: 'rose-pine', label: 'Rosé Pine', scheme: 'dark' },
  { id: 'ayu-mirage', label: 'Ayu Mirage', scheme: 'dark' },
  { id: 'ayu-dark', label: 'Ayu Dark', scheme: 'dark' },
  { id: 'everforest', label: 'Everforest', scheme: 'dark' },
  { id: 'kanagawa', label: 'Kanagawa', scheme: 'dark' },
  { id: 'synthwave-84', label: "Synthwave '84", scheme: 'dark' },
  { id: 'oceanic', label: 'Oceanic', scheme: 'dark' },
  { id: 'contrast', label: 'High Contrast', scheme: 'dark', highContrast: true },
]

export interface AccentInfo {
  id: string
  label: string
}

/** 'theme' means "use the theme's own accent" (no `data-accent` attribute). */
export const THEME_DEFAULT_ACCENT = 'theme'

export const ACCENTS: AccentInfo[] = [
  { id: 'blue', label: 'Blue' },
  { id: 'indigo', label: 'Indigo' },
  { id: 'violet', label: 'Violet' },
  { id: 'purple', label: 'Purple' },
  { id: 'fuchsia', label: 'Fuchsia' },
  { id: 'pink', label: 'Pink' },
  { id: 'rose', label: 'Rose' },
  { id: 'red', label: 'Red' },
  { id: 'orange', label: 'Orange' },
  { id: 'amber', label: 'Amber' },
  { id: 'yellow', label: 'Yellow' },
  { id: 'lime', label: 'Lime' },
  { id: 'green', label: 'Green' },
  { id: 'emerald', label: 'Emerald' },
  { id: 'teal', label: 'Teal' },
  { id: 'cyan', label: 'Cyan' },
  { id: 'sky', label: 'Sky' },
  { id: 'slate', label: 'Slate' },
]

/** Tokens every `[data-theme]` block must declare (the theme tests enforce this). */
export const THEME_TOKENS = [
  // surfaces and borders
  'bg', 'surface', 'surface-raised', 'surface-hover', 'border', 'border-strong',
  // text
  'text', 'text-muted', 'text-faint',
  // accent (the theme's default; a chosen accent overrides exactly these)
  'accent', 'accent-fg', 'accent-soft', 'accent-text', 'focus-ring', 'selection',
  // status
  'danger', 'danger-fg', 'danger-soft', 'success', 'success-soft', 'warning', 'warning-soft',
  // misc
  'preview-bg', 'overlay', 'shadow-pop',
  // syntax
  'syn-keyword', 'syn-string', 'syn-number', 'syn-bool', 'syn-comment', 'syn-property', 'syn-tag', 'syn-attr', 'syn-punct',
] as const

/** The tokens an accent choice replaces. */
export const ACCENT_TOKENS = ['accent', 'accent-fg', 'accent-soft', 'accent-text', 'focus-ring', 'selection'] as const

export const DEFAULT_LIGHT_THEME = 'light'
export const DEFAULT_DARK_THEME = 'dark'

export function findTheme(id: string): ThemeInfo | undefined {
  return THEMES.find((t) => t.id === id)
}

export function isAccent(id: string): boolean {
  return id === THEME_DEFAULT_ACCENT || ACCENTS.some((a) => a.id === id)
}

export interface SystemThemes {
  light: string
  dark: string
}

/**
 * The palette to show for a choice. 'system' maps to the configured light/dark theme; unknown ids (e.g. a theme
 * removed in a later version) fall back to the default of the matching scheme.
 */
export function resolveTheme(
  choice: string,
  prefersLight: boolean,
  system: SystemThemes = { light: DEFAULT_LIGHT_THEME, dark: DEFAULT_DARK_THEME },
): string {
  const find = findTheme
  if (choice === 'system') {
    const scheme: Scheme = prefersLight ? 'light' : 'dark'
    const t = find(prefersLight ? system.light : system.dark)
    return t && t.scheme === scheme ? t.id : prefersLight ? DEFAULT_LIGHT_THEME : DEFAULT_DARK_THEME
  }
  return find(choice) ? choice : DEFAULT_DARK_THEME
}
