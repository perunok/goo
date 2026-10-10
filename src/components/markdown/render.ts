/**
 * Markdown (GFM via `marked`) -> HTML, sanitised by DOMPurify with a strict allowlist. Ported from Slinger's docs
 * renderer; the security model is the same.
 *
 * - Author HTML is allowed only as a small set of formatting tags: no scripts, event handlers, styles, forms,
 *   iframes/objects/embeds, SVG/MathML, ids/names (no DOM clobbering) or data-* attributes except our own.
 * - `href` may be http(s), mailto, a `#fragment` or a relative path; everything else (javascript:, data:, file:,
 *   custom schemes) is removed. The view intercepts every click: http(s)/mailto go to the OS browser, relative
 *   links to another note open that note, fragments scroll; nothing ever navigates the app window.
 * - Images: only `data:image/*` sources render. Remote or relative images are not fetched (yet); they become a
 *   placeholder with the alt text.
 * - `class` values are filtered to the classes this module emits (`md-*`, `tok-*`).
 * - ```mermaid fences become `pre.md-mermaid` placeholders; MarkdownView turns them into diagrams afterwards
 *   (mermaid.ts, which runs Mermaid with securityLevel 'strict').
 */
import DOMPurify from 'dompurify'
import { Marked, type Token, type Tokens } from 'marked'
import { highlightToHtml } from './highlight'

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

/** Language id from a fence info string (` ```json title="x" ` -> "json"). */
export function fenceLanguage(info: string | undefined): string {
  return (info ?? '').trim().split(/\s+/)[0]!.toLowerCase().replace(/[^a-z0-9+#.-]/g, '')
}

const unescapeEntities = (s: string) =>
  s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')

/** GitHub-style heading slug: lower case, punctuation dropped, spaces to dashes. */
export function slugify(text: string): string {
  return (
    unescapeEntities(text)
      .toLowerCase()
      .trim()
      .replace(/[^\p{L}\p{N}\s_-]/gu, '')
      .replace(/\s/g, '-') || 'section'
  )
}

/** Visible text of inline tokens (for heading slugs). */
function plainText(tokens: Token[]): string {
  return tokens.map((tok) => ('tokens' in tok && tok.tokens ? plainText(tok.tokens) : 'text' in tok ? String(tok.text) : '')).join('')
}

/** YAML front matter at the very top (`---` ... `---`): shown as a muted block instead of a stray rule + heading. */
export function splitFrontMatter(source: string): { frontMatter: string | null; body: string } {
  const m = /^---[ \t]*\r?\n([\s\S]*?)\r?\n(?:---|\.\.\.)[ \t]*(?:\r?\n|$)/.exec(source)
  return m ? { frontMatter: m[1]!, body: source.slice(m[0].length) } : { frontMatter: null, body: source }
}

export interface Heading {
  depth: number
  text: string
  slug: string
}

function createMarked(): { marked: Marked; reset: () => void; headings: Heading[] } {
  const seen = new Map<string, number>()
  const headings: Heading[] = []
  const marked = new Marked({
    gfm: true,
    breaks: false,
    async: false,
    renderer: {
      heading({ tokens, depth }: Tokens.Heading) {
        const inner = this.parser.parseInline(tokens)
        const text = plainText(tokens)
        const base = slugify(text)
        const n = seen.get(base) ?? 0
        seen.set(base, n + 1)
        const slug = n === 0 ? base : `${base}-${n}`
        headings.push({ depth, text: unescapeEntities(text), slug })
        return `<h${depth} data-anchor="${esc(slug)}">${inner}<a class="md-anchor" href="#${esc(slug)}" aria-label="Link to this section">#</a></h${depth}>\n`
      },
      code({ text, lang }: Tokens.Code) {
        const language = fenceLanguage(lang)
        if (language === 'mermaid') return `<pre class="md-mermaid"><code>${esc(text)}</code></pre>\n`
        const label = language ? `<span class="md-code-lang" aria-hidden="true">${esc(language)}</span>` : ''
        return `<pre class="md-code">${label}<code>${highlightToHtml(text, language)}</code></pre>\n`
      },
    },
  })
  return {
    marked,
    headings,
    reset: () => {
      seen.clear()
      headings.length = 0
    },
  }
}

// ---------------------------------------------------------------------------
// Sanitising
// ---------------------------------------------------------------------------

const ALLOWED_TAGS = [
  'a', 'abbr', 'b', 'blockquote', 'br', 'caption', 'code', 'dd', 'del', 'details', 'div', 'dl', 'dt', 'em',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'i', 'img', 'input', 'ins', 'kbd', 'li', 'mark', 'ol', 'p', 'pre',
  's', 'samp', 'small', 'span', 'strike', 'strong', 'sub', 'summary', 'sup', 'table', 'tbody', 'td', 'tfoot',
  'th', 'thead', 'tr', 'u', 'ul', 'var',
]
const ALLOWED_ATTR = [
  'href', 'title', 'alt', 'src', 'align', 'colspan', 'rowspan', 'start', 'reversed', 'type', 'checked', 'disabled',
  'class', 'open', 'width', 'height', 'data-anchor', 'aria-label', 'aria-hidden',
]
/** http(s), mailto, #fragment, or a relative reference (no scheme). */
const ALLOWED_URI = /^(?:(?:https?|mailto):|#|[^a-z]|[a-z0-9+.-]+(?:[^a-z0-9+.\-:]|$))/i
const SAFE_IMAGE = /^data:image\/(?:png|jpe?g|gif|webp|bmp|avif|x-icon)[;,]/i
const OWN_CLASS = /^(?:md-[a-z-]+|tok-[a-z]+)$/
export const EXTERNAL_URL = /^(?:https?:|mailto:)/i

let purifier: ReturnType<typeof DOMPurify> | null = null
function purify() {
  if (purifier) return purifier
  const p = DOMPurify(window)
  p.addHook('uponSanitizeAttribute', (_node, data) => {
    if (data.attrName === 'class') {
      data.attrValue = data.attrValue.split(/\s+/).filter((c) => OWN_CLASS.test(c)).join(' ')
      if (!data.attrValue) data.keepAttr = false
    }
  })
  p.addHook('afterSanitizeAttributes', (node) => {
    const el = node as Element
    if (el.tagName === 'A') {
      const href = (el.getAttribute('href') ?? '').trim()
      if (EXTERNAL_URL.test(href)) {
        el.setAttribute('class', 'md-ext')
        el.setAttribute('title', el.getAttribute('title') || href)
      } else if (href && !href.startsWith('#') && !el.classList.contains('md-anchor')) {
        el.setAttribute('class', 'md-link')
      }
    } else if (el.tagName === 'IMG') {
      const src = (el.getAttribute('src') ?? '').trim()
      if (!SAFE_IMAGE.test(src)) {
        el.removeAttribute('src')
        el.setAttribute('class', 'md-img-blocked')
        el.setAttribute('title', src)
      }
    } else if (el.tagName === 'INPUT') {
      // Only GFM task-list checkboxes, never editable.
      el.setAttribute('type', 'checkbox')
      el.setAttribute('disabled', '')
    }
  })
  purifier = p
  return p
}

const PURIFY_CONFIG = {
  ALLOWED_TAGS,
  ALLOWED_ATTR,
  ALLOWED_URI_REGEXP: ALLOWED_URI,
  ALLOW_DATA_ATTR: false,
  ALLOW_ARIA_ATTR: false,
  ALLOW_UNKNOWN_PROTOCOLS: false,
  KEEP_CONTENT: true,
  RETURN_DOM_FRAGMENT: true,
} as const

export function sanitizeFragment(dirty: string): DocumentFragment {
  const frag = purify().sanitize(dirty, PURIFY_CONFIG) as unknown as DocumentFragment
  for (const img of Array.from(frag.querySelectorAll('img.md-img-blocked'))) {
    const box = document.createElement('span')
    box.className = 'md-img-blocked'
    box.textContent = img.getAttribute('alt') || 'Image'
    box.setAttribute('title', `Not loaded: ${img.getAttribute('title') || 'image'} (only embedded images are shown for now)`)
    img.replaceWith(box)
  }
  return frag
}

export function sanitizeHtml(dirty: string): string {
  const box = document.createElement('div')
  box.append(sanitizeFragment(dirty))
  return box.innerHTML
}

// ---------------------------------------------------------------------------
// Public entry
// ---------------------------------------------------------------------------

export interface Rendered {
  html: string
  headings: Heading[]
}

let engine: ReturnType<typeof createMarked> | null = null
const cache = new Map<string, Rendered>()
const CACHE_SIZE = 16

/** Rendered, sanitised HTML for a Markdown document, plus its headings (for an outline). */
export function renderMarkdown(source: string): Rendered {
  const hit = cache.get(source)
  if (hit) return hit
  engine ??= createMarked()
  engine.reset()
  const { frontMatter, body } = splitFrontMatter(source)
  let dirty = frontMatter === null ? '' : `<pre class="md-frontmatter"><code>${esc(frontMatter)}</code></pre>\n`
  try {
    dirty += engine.marked.parse(body) as string
  } catch {
    dirty += `<pre class="md-plain">${esc(body)}</pre>`
  }
  const out: Rendered = { html: sanitizeHtml(dirty), headings: [...engine.headings] }
  if (cache.size >= CACHE_SIZE) cache.delete(cache.keys().next().value!)
  cache.set(source, out)
  return out
}
