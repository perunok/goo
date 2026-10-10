/**
 * Colour maths for the theme tests (WCAG 2.x contrast) and for evaluating the small subset of CSS colour
 * syntax used in src/styles/themes.css: hex, rgb()/rgba(), color-mix(in srgb, ...) and light-dark().
 */
export interface Rgba {
  r: number
  g: number
  b: number
  a: number
}

export function parseHex(hex: string): Rgba | null {
  const m = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(hex.trim())
  if (!m) return null
  let h = m[1]!
  if (h.length <= 4) h = [...h].map((c) => c + c).join('')
  const n = (i: number) => parseInt(h.slice(i, i + 2), 16)
  return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 }
}

export function toHex(c: Rgba): string {
  const p = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0')
  return `#${p(c.r)}${p(c.g)}${p(c.b)}`
}

function channel(v: number): number {
  const s = v / 255
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

/** WCAG relative luminance (0..1). */
export function luminance(c: Rgba): number {
  return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/** WCAG contrast ratio (1..21). A translucent foreground is composited over the background first. */
export function contrast(fg: Rgba, bg: Rgba): number {
  const f = fg.a < 1 ? mix(fg, bg, fg.a) : fg
  const a = luminance(f)
  const b = luminance(bg)
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
}

/** `color-mix(in srgb, a p, b)`: p is the weight of `a` (0..1), interpolated in gamma-encoded sRGB. */
export function mix(a: Rgba, b: Rgba, p: number): Rgba {
  const q = 1 - p
  return { r: a.r * p + b.r * q, g: a.g * p + b.g * q, b: a.b * p + b.b * q, a: a.a * p + b.a * q }
}

/** Splits on top-level commas (ignores commas nested in parentheses). */
export function splitArgs(s: string): string[] {
  const out: string[] = []
  let depth = 0
  let cur = ''
  for (const ch of s) {
    if (ch === '(') depth++
    if (ch === ')') depth--
    if (ch === ',' && depth === 0) {
      out.push(cur.trim())
      cur = ''
    } else cur += ch
  }
  if (cur.trim()) out.push(cur.trim())
  return out
}

export interface EvalContext {
  /** Custom property values (raw CSS text) visible on the element. */
  vars: Record<string, string>
  scheme: 'light' | 'dark'
  /**
   * Optional fallback for colours outside the subset above (named colours, hsl(), oklch(), ...): the renderer passes a
   * browser-backed converter for user-defined custom themes. Returns null when it cannot convert either.
   */
  leaf?: (expr: string) => Rgba | null
}

function viaLeaf(e: string, ctx: EvalContext, why: string): Rgba {
  const c = ctx.leaf?.(e)
  if (c) return c
  throw new Error(why)
}

/** Evaluates a CSS colour expression; throws on anything outside the supported subset. */
export function evalColor(expr: string, ctx: EvalContext, depth = 0): Rgba {
  if (depth > 20) throw new Error(`colour reference cycle: ${expr}`)
  const e = expr.trim()
  const hex = parseHex(e)
  if (hex) return hex
  const fn = /^([a-z-]+)\((.*)\)$/is.exec(e)
  if (!fn) return viaLeaf(e, ctx, `unsupported colour: ${e}`)
  const name = fn[1]!.toLowerCase()
  const args = splitArgs(fn[2]!)
  if (name === 'var') {
    const key = args[0]!.replace(/^--/, '')
    const v = ctx.vars[key]
    if (v === undefined) {
      if (args[1] !== undefined) return evalColor(args[1], ctx, depth + 1)
      throw new Error(`undefined variable --${key}`)
    }
    return evalColor(v, ctx, depth + 1)
  }
  if (name === 'light-dark') return evalColor(ctx.scheme === 'light' ? args[0]! : args[1]!, ctx, depth + 1)
  if (name === 'rgb' || name === 'rgba') {
    const n = args.length === 1 ? args[0]!.split(/[\s/]+/) : args
    const [r, g, b, a] = n.map((x, i) => (x.endsWith('%') ? (parseFloat(x) * (i < 3 ? 255 : 1)) / 100 : parseFloat(x)))
    if (n.length < 3 || n.length > 4 || [r, g, b, a ?? 1].some((v) => !Number.isFinite(v))) return viaLeaf(e, ctx, `unsupported colour: ${e}`)
    return { r: r!, g: g!, b: b!, a: a ?? 1 }
  }
  if (name === 'color-mix') {
    if (!/^in srgb$/i.test(args[0]!)) return viaLeaf(e, ctx, `color-mix must use srgb: ${e}`)
    const part = (s: string) => {
      const m = /^(.*?)(?:\s+([\d.]+)%)?$/s.exec(s.trim())!
      return { color: evalColor(m[1]!, ctx, depth + 1), pct: m[2] === undefined ? undefined : parseFloat(m[2]) / 100 }
    }
    const a = part(args[1]!)
    const b = part(args[2]!)
    const p = a.pct ?? (b.pct === undefined ? 0.5 : 1 - b.pct)
    return mix(a.color, b.color, p)
  }
  return viaLeaf(e, ctx, `unsupported colour function: ${name}`)
}
