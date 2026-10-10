/**
 * Ported from Slinger. Reads src/styles/themes.css as data: which tokens each `[data-theme]` block declares, what a theme looks like with a
 * given accent layered on top, and which colour pairs miss their WCAG contrast target. Used by the theme tests (and
 * handy when tuning a palette); the app itself never needs it at runtime.
 */
import { contrast, evalColor, type Rgba } from './contrast'

export interface CssBlock {
  selectors: string[]
  decls: Record<string, string>
  /** Source order, for the cascade. */
  order: number
}

/** Parses the flat (non-nested) rule blocks of a stylesheet. Comments are dropped. */
export function parseBlocks(css: string): CssBlock[] {
  const src = css.replace(/\/\*[\s\S]*?\*\//g, '')
  const out: CssBlock[] = []
  const re = /([^{}]+)\{([^{}]*)\}/g
  let m: RegExpExecArray | null
  while ((m = re.exec(src))) {
    const decls: Record<string, string> = {}
    for (const part of m[2]!.split(';')) {
      const i = part.indexOf(':')
      if (i < 0) continue
      const prop = part.slice(0, i).trim()
      const value = part.slice(i + 1).trim()
      if (prop) decls[prop] = value
    }
    out.push({ selectors: m[1]!.split(',').map((s) => s.trim()), decls, order: out.length })
  }
  return out
}

type Attrs = Record<string, string | undefined>

/**
 * Matches the only selector shapes themes.css uses: `:root` and compounds of attribute selectors
 * (`[a]`, `[a='v']`, `[a^='v']`). Returns the specificity (attribute count), or -1 when it does not match.
 */
export function matchSelector(sel: string, attrs: Attrs): number {
  if (sel === ':root') return 1
  const parts = sel.match(/\[[^\]]+\]/g)
  if (!parts || parts.join('') !== sel) throw new Error(`unsupported selector in themes.css: ${sel}`)
  for (const p of parts) {
    const m = /^\[([\w-]+)(?:(\^?=)'([^']*)')?\]$/.exec(p)
    if (!m) throw new Error(`unsupported selector in themes.css: ${sel}`)
    const v = attrs[m[1]!]
    if (v === undefined) return -1
    if (m[2] === '=' && v !== m[3]) return -1
    if (m[2] === '^=' && !v.startsWith(m[3]!)) return -1
  }
  return parts.length
}

/** Custom properties (without `--`) and `color-scheme` that apply to an element carrying `attrs`. */
export function cascade(blocks: CssBlock[], attrs: Attrs): Record<string, string> {
  const hits: { spec: number; order: number; decls: Record<string, string> }[] = []
  for (const b of blocks) {
    const spec = Math.max(-1, ...b.selectors.map((s) => matchSelector(s, attrs)))
    if (spec >= 0) hits.push({ spec, order: b.order, decls: b.decls })
  }
  hits.sort((a, b) => a.spec - b.spec || a.order - b.order)
  const out: Record<string, string> = {}
  for (const h of hits) {
    for (const [k, v] of Object.entries(h.decls)) {
      if (k.startsWith('--')) out[k.slice(2)] = v
      else if (k === 'color-scheme') out[k] = v
    }
  }
  return out
}

/** Tokens declared directly by each `[data-theme='id']` block. */
export function themeBlocks(blocks: CssBlock[]): Map<string, Record<string, string>> {
  const out = new Map<string, Record<string, string>>()
  for (const b of blocks) {
    for (const s of b.selectors) {
      const m = /^\[data-theme='([\w-]+)'\]$/.exec(s)
      if (m) out.set(m[1]!, { ...(out.get(m[1]!) ?? {}), ...b.decls })
    }
  }
  return out
}

/** Accent ids that have their own `[data-accent='id']` block. */
export function accentIds(blocks: CssBlock[]): string[] {
  const ids: string[] = []
  for (const b of blocks) for (const s of b.selectors) {
    const m = /^\[data-accent='([\w-]+)'\]$/.exec(s)
    if (m && !ids.includes(m[1]!)) ids.push(m[1]!)
  }
  return ids
}

export interface Resolved {
  scheme: 'light' | 'dark'
  color(token: string): Rgba
}

/** The palette of `theme` with `accent` (null = theme default), as the browser would compute it on <html>. */
export function resolve(blocks: CssBlock[], theme: string, accent: string | null): Resolved {
  const vars = cascade(blocks, { 'data-theme': theme, ...(accent ? { 'data-accent': accent } : {}) })
  const scheme = vars['color-scheme'] === 'light' ? 'light' : 'dark'
  return { scheme, color: (token) => evalColor(`var(--${token})`, { vars, scheme }) }
}

export interface Check {
  fg: string
  bg: string
  /** AA target, and the stricter target used for the high-contrast themes. */
  min: number
  strict: number
  why: string
}

const pairs = (fgs: string[], bgs: string[], min: number, strict: number, why: string): Check[] =>
  fgs.flatMap((fg) => bgs.map((bg) => ({ fg, bg, min, strict, why })))

const SYNTAX = ['syn-keyword', 'syn-string', 'syn-number', 'syn-bool', 'syn-comment', 'syn-property', 'syn-tag', 'syn-attr', 'syn-punct']

/** Every foreground/background pair the UI actually renders, with its target ratio. */
export const CHECKS: Check[] = [
  ...pairs(['text'], ['bg', 'surface', 'surface-raised', 'surface-hover', 'accent-soft', 'selection'], 4.5, 7, 'body text'),
  ...pairs(['text-muted'], ['bg', 'surface', 'surface-raised'], 4.5, 7, 'secondary text'),
  ...pairs(['text-faint'], ['bg', 'surface'], 3, 4.5, 'hints / placeholders (UI text)'),
  ...pairs(['accent-fg'], ['accent'], 4.5, 7, 'text on accent (primary buttons)'),
  ...pairs(['accent', 'focus-ring'], ['bg', 'surface'], 3, 4.5, 'accent fill / focus ring against the page (UI)'),
  ...pairs(['accent-text'], ['bg', 'surface', 'accent-soft'], 4.5, 7, 'accent-coloured text'),
  ...pairs(['danger', 'success', 'warning'], ['bg', 'surface'], 4.5, 7, 'status text'),
  { fg: 'danger', bg: 'danger-soft', min: 4.5, strict: 7, why: 'status badge' },
  { fg: 'success', bg: 'success-soft', min: 4.5, strict: 7, why: 'status badge' },
  { fg: 'warning', bg: 'warning-soft', min: 4.5, strict: 7, why: 'status badge' },
  ...pairs(['text'], ['danger-soft', 'success-soft', 'warning-soft'], 4.5, 7, 'text on status background'),
  { fg: 'danger-fg', bg: 'danger', min: 4.5, strict: 7, why: 'text on danger buttons' },
  ...pairs(SYNTAX, ['bg', 'surface'], 4.5, 7, 'code'),
]

export interface Failure extends Check {
  ratio: number
  target: number
}

export function audit(p: Resolved, strict: boolean): Failure[] {
  const out: Failure[] = []
  for (const c of CHECKS) {
    const ratio = contrast(p.color(c.fg), p.color(c.bg))
    const target = strict ? c.strict : c.min
    if (ratio + 1e-9 < target) out.push({ ...c, ratio, target })
  }
  return out
}
