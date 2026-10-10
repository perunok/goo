/**
 * Mermaid diagrams, themed from the app's own tokens so every theme x accent pair gets matching diagrams.
 *
 * Mermaid is large, so it is loaded on first use (its own chunk). It runs with securityLevel 'strict': no click
 * handlers, no scripts, and labels are sanitised by Mermaid's DOMPurify before the SVG reaches us.
 */
import type { Mermaid, MermaidConfig } from 'mermaid'

let loading: Promise<Mermaid> | null = null
let configuredFor = ''
let seq = 0
const cache = new Map<string, string>()
const CACHE_SIZE = 64

function load(): Promise<Mermaid> {
  loading ??= import('mermaid').then((m) => m.default)
  return loading
}

/**
 * The value of a colour token as `#rrggbb`. Tokens hold expressions like `light-dark(...)` / `color-mix(...)`, and
 * Mermaid's colour maths only understands plain colours, so the browser resolves it: the computed `color` of a probe
 * element, normalised through a 1x1 canvas (computed colour-mix() results come back as `color(srgb ...)`).
 */
function tokenColors(names: readonly string[]): Record<string, string> {
  const probe = document.createElement('span')
  probe.style.display = 'none'
  document.body.append(probe)
  const ctx = document.createElement('canvas').getContext('2d', { willReadFrequently: true })
  const out: Record<string, string> = {}
  for (const name of names) {
    probe.style.color = `var(--${name})`
    const computed = getComputedStyle(probe).color
    out[name] = ctx ? toHex(ctx, computed) : computed
  }
  probe.remove()
  return out
}

function toHex(ctx: CanvasRenderingContext2D, color: string): string {
  ctx.clearRect(0, 0, 1, 1)
  ctx.fillStyle = 'black'
  ctx.fillStyle = color
  ctx.fillRect(0, 0, 1, 1)
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data
  return '#' + [r, g, b].map((v) => (v ?? 0).toString(16).padStart(2, '0')).join('')
}

const TOKENS = [
  'bg', 'surface', 'surface-raised', 'border', 'border-strong', 'text', 'text-muted',
  'accent', 'accent-fg', 'accent-soft', 'accent-text',
  'danger', 'danger-soft', 'success', 'success-soft', 'warning', 'warning-soft',
  'syn-keyword', 'syn-string', 'syn-number', 'syn-bool', 'syn-property', 'syn-tag', 'syn-attr',
] as const

/** Mermaid `base` theme variables derived from the current palette. */
export function themeVariables(): Record<string, string | boolean> {
  const c = tokenColors(TOKENS) as Record<(typeof TOKENS)[number], string>
  const dark = getComputedStyle(document.documentElement).colorScheme.includes('dark')
  const font = getComputedStyle(document.documentElement).getPropertyValue('--font-sans').trim()
  const pies = [c.accent, c['syn-string'], c['syn-number'], c['syn-keyword'], c['syn-property'], c['syn-tag'], c['syn-attr'], c['syn-bool'], c.success, c.warning, c.danger, c['text-muted']]
  return {
    darkMode: dark,
    fontFamily: font,
    background: c.surface,
    textColor: c.text,
    titleColor: c.text,
    lineColor: c['text-muted'],
    primaryColor: c['accent-soft'],
    primaryTextColor: c.text,
    primaryBorderColor: c.accent,
    secondaryColor: c['surface-raised'],
    secondaryTextColor: c.text,
    secondaryBorderColor: c['border-strong'],
    tertiaryColor: c.surface,
    tertiaryTextColor: c.text,
    tertiaryBorderColor: c.border,
    mainBkg: c['accent-soft'],
    nodeBorder: c.accent,
    nodeTextColor: c.text,
    clusterBkg: c['surface-raised'],
    clusterBorder: c.border,
    edgeLabelBackground: c.surface,
    noteBkgColor: c['warning-soft'],
    noteTextColor: c.text,
    noteBorderColor: c.warning,
    // sequence
    actorBkg: c['accent-soft'],
    actorBorder: c.accent,
    actorTextColor: c.text,
    actorLineColor: c['border-strong'],
    signalColor: c.text,
    signalTextColor: c.text,
    labelBoxBkgColor: c['surface-raised'],
    labelBoxBorderColor: c.border,
    labelTextColor: c.text,
    loopTextColor: c.text,
    activationBkgColor: c['surface-raised'],
    activationBorderColor: c['border-strong'],
    sequenceNumberColor: c['accent-fg'],
    // gantt
    sectionBkgColor: c['surface-raised'],
    altSectionBkgColor: c.surface,
    sectionBkgColor2: c['surface-raised'],
    gridColor: c.border,
    taskBkgColor: c['accent-soft'],
    taskBorderColor: c.accent,
    taskTextColor: c.text,
    taskTextLightColor: c.text,
    taskTextDarkColor: c.text,
    taskTextOutsideColor: c.text,
    activeTaskBkgColor: c.accent,
    activeTaskBorderColor: c.accent,
    doneTaskBkgColor: c['surface-raised'],
    doneTaskBorderColor: c['border-strong'],
    critBkgColor: c['danger-soft'],
    critBorderColor: c.danger,
    todayLineColor: c.danger,
    // pie
    ...Object.fromEntries(pies.map((p, i) => [`pie${i + 1}`, p])),
    pieTitleTextColor: c.text,
    pieSectionTextColor: c.bg,
    pieLegendTextColor: c.text,
    pieStrokeColor: c.surface,
    pieOuterStrokeColor: c.border,
    // state / class / er
    labelColor: c.text,
    altBackground: c['surface-raised'],
    attributeBackgroundColorOdd: c.surface,
    attributeBackgroundColorEven: c['surface-raised'],
    // git graph
    git0: c.accent,
    git1: c['syn-string'],
    git2: c['syn-number'],
    git3: c['syn-keyword'],
    gitBranchLabel0: c['accent-fg'],
    commitLabelColor: c.text,
    commitLabelBackground: c['surface-raised'],
  }
}

/** Something that changes whenever the palette does (theme, accent or font size). */
export function paletteKey(): string {
  const d = document.documentElement
  return [d.dataset.theme, d.dataset.accent ?? '', d.style.getPropertyValue('--font-size')].join('|')
}

export type MermaidResult = { svg: string } | { error: string }

let queue: Promise<unknown> = Promise.resolve()

/**
 * Renders one diagram for the current palette. Results are cached per (palette, source). Renders run one at a time:
 * Mermaid keeps global state (its config, a scratch element) between initialize() and render().
 */
export function renderMermaid(source: string): Promise<MermaidResult> {
  const run = queue.then(() => renderNow(source))
  queue = run.catch(() => {})
  return run
}

async function renderNow(source: string): Promise<MermaidResult> {
  const palette = paletteKey()
  const key = `${palette}\u0000${source}`
  const hit = cache.get(key)
  if (hit !== undefined) return { svg: hit }
  const mermaid = await load()
  if (configuredFor !== palette) {
    const config: MermaidConfig = {
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'base',
      themeVariables: themeVariables(),
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue('--font-sans').trim(),
    }
    mermaid.initialize(config)
    configuredFor = palette
  }
  const id = `goo-mermaid-${++seq}`
  try {
    const { svg } = await mermaid.render(id, source)
    if (cache.size >= CACHE_SIZE) cache.delete(cache.keys().next().value!)
    cache.set(key, svg)
    return { svg }
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) }
  } finally {
    // Mermaid leaves its scratch element behind when a diagram fails to parse.
    document.getElementById(`d${id}`)?.remove()
    document.getElementById(id)?.remove()
  }
}
