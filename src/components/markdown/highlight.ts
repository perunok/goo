/**
 * Static code highlighting for the preview: CodeMirror's Lezer parsers produce `tok-*` spans, coloured by the
 * theme's `--syn-*` tokens (markdown.css). The same tag groups as the editor theme (components/editor/theme.ts).
 */
import { css } from '@codemirror/lang-css'
import { html } from '@codemirror/lang-html'
import { javascript } from '@codemirror/lang-javascript'
import { json } from '@codemirror/lang-json'
import { python } from '@codemirror/lang-python'
import { rust } from '@codemirror/lang-rust'
import { xml } from '@codemirror/lang-xml'
import { highlightTree, tagHighlighter, tags as t } from '@lezer/highlight'

export const highlighter = tagHighlighter([
  { tag: [t.keyword, t.operatorKeyword, t.modifier, t.controlKeyword, t.definitionKeyword], class: 'tok-keyword' },
  { tag: [t.string, t.special(t.string), t.attributeValue, t.regexp, t.character], class: 'tok-string' },
  { tag: [t.number, t.integer, t.float], class: 'tok-number' },
  { tag: [t.bool, t.null, t.atom], class: 'tok-bool' },
  { tag: [t.propertyName, t.definition(t.propertyName)], class: 'tok-property' },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment, t.processingInstruction, t.meta, t.documentMeta], class: 'tok-comment' },
  { tag: [t.tagName, t.typeName, t.className, t.macroName], class: 'tok-tag' },
  { tag: [t.attributeName, t.variableName, t.function(t.variableName)], class: 'tok-attr' },
  { tag: [t.punctuation, t.bracket, t.angleBracket, t.separator, t.operator], class: 'tok-punct' },
])

type Parser = ReturnType<typeof json>['language']['parser']
const parsers = new Map<string, () => Parser>()
const lazy = (make: () => Parser) => {
  let p: Parser | null = null
  return () => (p ??= make())
}
const register = (names: string[], make: () => Parser) => {
  const get = lazy(make)
  for (const n of names) parsers.set(n, get)
}
register(['js', 'javascript', 'mjs', 'cjs', 'node', 'jsx'], () => javascript({ jsx: true }).language.parser)
register(['ts', 'typescript', 'tsx'], () => javascript({ typescript: true, jsx: true }).language.parser)
register(['json', 'jsonc', 'json5', 'geojson'], () => json().language.parser)
register(['xml', 'svg', 'xsd'], () => xml().language.parser)
register(['html', 'htm', 'xhtml', 'vue', 'svelte'], () => html().language.parser)
register(['css', 'scss', 'less'], () => css().language.parser)
register(['py', 'python'], () => python().language.parser)
register(['rs', 'rust'], () => rust().language.parser)

/** Code blocks larger than this are shown without highlighting (keeps huge pastes fast). */
const MAX_HIGHLIGHT_CHARS = 200_000

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

export function canHighlight(lang: string): boolean {
  return parsers.has(lang)
}

/** Escaped HTML for a code block, with `tok-*` spans where the language is known. */
export function highlightToHtml(code: string, lang: string): string {
  const make = parsers.get(lang)
  if (!make || code.length > MAX_HIGHLIGHT_CHARS) return esc(code)
  let out = ''
  let pos = 0
  try {
    highlightTree(make().parse(code), highlighter, (from, to, cls) => {
      if (from > pos) out += esc(code.slice(pos, from))
      out += `<span class="${cls}">${esc(code.slice(from, to))}</span>`
      pos = to
    })
  } catch {
    return esc(code) // parser trouble: plain text is fine
  }
  return out + esc(code.slice(pos))
}
