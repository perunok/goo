import { readFileSync } from 'node:fs'
import { contrast, parseHex } from '../lib/contrast'
import { accentIds, audit, cascade, parseBlocks, resolve, themeBlocks, type Failure } from '../lib/themeAudit'
import { ACCENTS, ACCENT_TOKENS, THEMES, THEME_TOKENS } from '../lib/themes'

// Read from disk: the renderer test config disables CSS processing, so `?raw` CSS imports come back empty.
const read = (name: string) => readFileSync(new URL(name, import.meta.url), 'utf8')
const themesCss = read('./themes.css')
const appCss = read('./app.css')

const blocks = parseBlocks(themesCss)
const themes = themeBlocks(blocks)

describe('theme registry and themes.css agree', () => {
  it('registers every palette exactly once, and every registered theme has a palette', () => {
    const ids = THEMES.map((t) => t.id)
    expect(new Set(ids).size).toBe(ids.length)
    expect([...themes.keys()].sort()).toEqual([...ids].sort())
  })

  it('keeps the original theme ids', () => {
    for (const id of ['light', 'dark', 'midnight', 'solarized', 'contrast']) expect(themes.has(id)).toBe(true)
  })

  it.each(THEMES.map((t) => [t.id, t] as const))('%s declares every token and its color-scheme', (id, info) => {
    const decls = themes.get(id)!
    const declared = Object.keys(decls).filter((k) => k.startsWith('--')).map((k) => k.slice(2)).sort()
    expect(declared).toEqual([...THEME_TOKENS].sort())
    expect(decls['color-scheme']).toBe(info.scheme)
  })

  it('has a block for every accent (and no unregistered ones), each with light and dark variants', () => {
    expect(accentIds(blocks).sort()).toEqual(ACCENTS.map((a) => a.id).sort())
    for (const a of ACCENTS) {
      const vars = cascade(blocks, { 'data-accent': a.id })
      for (const k of ['a-light', 'a-light-fg', 'a-light-text', 'a-dark', 'a-dark-fg', 'a-dark-text']) {
        expect(parseHex(vars[k] ?? ''), `${a.id} --${k}`).not.toBeNull()
      }
    }
  })

  it('an accent replaces exactly the accent tokens', () => {
    const base = cascade(blocks, { 'data-theme': 'dark' })
    const withAccent = cascade(blocks, { 'data-theme': 'dark', 'data-accent': 'violet' })
    const changed = THEME_TOKENS.filter((k) => base[k] !== withAccent[k])
    expect(changed.sort()).toEqual([...ACCENT_TOKENS].sort())
  })

  it('picks whichever of light or dark ink reads better on each accent', () => {
    const inks = ['#ffffff', '#10121a'].map((h) => parseHex(h)!)
    for (const a of ACCENTS) {
      const vars = cascade(blocks, { 'data-accent': a.id })
      for (const s of ['light', 'dark']) {
        const fill = parseHex(vars[`a-${s}`]!)!
        const fg = parseHex(vars[`a-${s}-fg`]!)!
        const best = Math.max(...inks.map((i) => contrast(i, fill)))
        expect(contrast(fg, fill), `${a.id} ${s}`).toBeCloseTo(best, 5)
      }
    }
  })
})

describe('contrast (WCAG 2.x)', () => {
  const combos = THEMES.flatMap((t) => [null, ...ACCENTS.map((a) => a.id)].map((accent) => [t, accent] as const))

  it(`checks all ${THEMES.length} themes x ${ACCENTS.length + 1} accent choices`, () => {
    const failures: string[] = []
    for (const [t, accent] of combos) {
      const f: Failure[] = audit(resolve(blocks, t.id, accent), !!t.highContrast)
      for (const x of f) failures.push(`${t.id} + ${accent ?? 'theme default'}: ${x.fg} on ${x.bg} = ${x.ratio.toFixed(2)} (needs ${x.target}, ${x.why})`)
    }
    expect(failures).toEqual([])
  })

  it('holds the high-contrast themes to AAA', () => {
    const hc = THEMES.filter((t) => t.highContrast)
    expect(hc.map((t) => t.scheme).sort()).toEqual(['dark', 'light'])
    for (const t of hc) {
      const p = resolve(blocks, t.id, null)
      expect(contrast(p.color('text'), p.color('bg'))).toBeGreaterThanOrEqual(7)
      expect(contrast(p.color('accent-fg'), p.color('accent'))).toBeGreaterThanOrEqual(7)
    }
  })

  it('resolves the light variant of an accent on light themes and the dark variant on dark themes', () => {
    const vars = cascade(blocks, { 'data-accent': 'blue' })
    expect(resolve(blocks, 'github-light', 'blue').color('accent')).toEqual(parseHex(vars['a-light']!))
    expect(resolve(blocks, 'dracula', 'blue').color('accent')).toEqual(parseHex(vars['a-dark']!))
  })
})

describe('colours only come from themes.css', () => {
  const sources = {
    ...import.meta.glob(['../**/*.svelte', '../**/*.ts', '!../**/*.test.ts', '!../dev/**'], { query: '?raw', import: 'default', eager: true }),
    './app.css': appCss,
    // the first-paint stylesheet, the page and the pre-paint theme script
    './boot.css': read('./boot.css'),
    '../../index.html': read('../../index.html'),
    '../../public/theme-init.js': read('../../public/theme-init.js'),
    '../components/markdown/markdown.css': read('../components/markdown/markdown.css'),
  } as Record<string, string>
  // plus the non-colour layout variables (reading-width is set inline by App.svelte)
  const known = new Set<string>([...THEME_TOKENS, 'font-sans', 'font-mono', 'font-size', 'radius', 'reading-width'])

  it('components, stores and stylesheets contain no literal colours', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(30)
    const hits: string[] = []
    for (const [file, src] of Object.entries(sources)) {
      // strip comments so prose like "issue #123" does not count
      const code = src.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1').replace(/<!--[\s\S]*?-->/g, '')
      for (const m of code.matchAll(/#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|oklch|lab)\(/g)) {
        const around = code.slice(Math.max(0, m.index! - 20), m.index! + 12)
        hits.push(`${file}: ${around.replace(/\s+/g, ' ')}`)
      }
    }
    expect(hits).toEqual([])
  })

  it('native input and textarea placeholders use the faint text token (as CodeMirror placeholders do)', () => {
    const rule = appCss.match(/input::placeholder,\s*textarea::placeholder\s*\{([^}]*)\}/)
    expect(rule?.[1]).toMatch(/color:\s*var\(--text-faint\)/)
    expect(rule?.[1]).toMatch(/opacity:\s*1/)
  })

  it('every var(--token) used by the UI exists', () => {
    const unknown = new Set<string>()
    for (const src of Object.values(sources)) {
      for (const m of src.matchAll(/var\(--([\w-]+)/g)) {
        const name = m[1]!
        // `var(--m-${method})` style templates: the prefix must belong to real tokens
        const ok = name.endsWith('-') ? [...known].some((k) => k.startsWith(name)) : known.has(name)
        if (!ok) unknown.add(name)
      }
    }
    expect([...unknown]).toEqual([])
  })

})
