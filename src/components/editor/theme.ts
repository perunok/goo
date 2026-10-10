/**
 * CodeMirror theme + highlight style driven entirely by CSS variables from src/styles/themes.css, so switching
 * `data-theme` restyles the editor with no reconfiguration (ported from Slinger).
 */
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { EditorView } from '@codemirror/view'
import { tags as t } from '@lezer/highlight'

const editorTheme = EditorView.theme({
  '&': { color: 'var(--text)', backgroundColor: 'transparent', fontSize: 'inherit', height: '100%' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.6' },
  '.cm-content': { caretColor: 'var(--text)', padding: '16px 0 40vh' },
  '.cm-line': { padding: '0 20px' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text)' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground': {
    background: 'var(--selection)',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--surface)',
    color: 'var(--text-faint)',
    border: 'none',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--text) 6%, transparent)' },
  '.cm-activeLineGutter': { backgroundColor: 'color-mix(in srgb, var(--text) 8%, transparent)', color: 'var(--text)' },
  '.cm-foldPlaceholder': {
    backgroundColor: 'var(--surface-raised)',
    border: '1px solid var(--border)',
    color: 'var(--text-muted)',
    padding: '0 4px',
  },
  '.cm-placeholder': { color: 'var(--text-faint)' },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': {
    backgroundColor: 'color-mix(in srgb, var(--accent) 25%, transparent)',
    outline: '1px solid var(--accent)',
  },
  '.cm-searchMatch': { backgroundColor: 'color-mix(in srgb, var(--warning) 35%, transparent)', outline: '1px solid var(--warning)' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'color-mix(in srgb, var(--accent) 45%, transparent)' },
  '.cm-selectionMatch': { backgroundColor: 'color-mix(in srgb, var(--accent) 20%, transparent)' },
  // search panel
  '.cm-panels': { backgroundColor: 'var(--surface-raised)', color: 'var(--text)', borderColor: 'var(--border)' },
  '.cm-panels-top': { borderBottom: '1px solid var(--border)' },
  '.cm-panel.cm-search': { padding: '6px 28px 6px 8px', display: 'flex', flexWrap: 'wrap', gap: '4px', alignItems: 'center' },
  '.cm-panel.cm-search input, .cm-panel.cm-search button': {
    background: 'var(--surface)',
    color: 'var(--text)',
    border: '1px solid var(--border)',
    borderRadius: '4px',
    padding: '2px 6px',
    fontSize: '12px',
  },
  '.cm-panel.cm-search button:hover': { background: 'var(--surface-hover)' },
  '.cm-panel.cm-search label': { fontSize: '12px', color: 'var(--text-muted)' },
  '.cm-panel.cm-search [name=close]': { position: 'absolute', top: '4px', right: '4px', background: 'transparent', border: 'none' },
})

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier], color: 'var(--syn-keyword)' },
  { tag: [t.string, t.special(t.string), t.attributeValue], color: 'var(--syn-string)' },
  { tag: [t.number, t.integer, t.float], color: 'var(--syn-number)' },
  { tag: [t.bool, t.null, t.atom], color: 'var(--syn-bool)' },
  { tag: [t.propertyName, t.definition(t.propertyName)], color: 'var(--syn-property)' },
  { tag: [t.comment, t.lineComment, t.blockComment], color: 'var(--syn-comment)', fontStyle: 'italic' },
  { tag: [t.tagName, t.typeName, t.className], color: 'var(--syn-tag)' },
  { tag: [t.attributeName, t.variableName, t.function(t.variableName)], color: 'var(--syn-attr)' },
  { tag: [t.punctuation, t.bracket, t.angleBracket, t.separator, t.operator], color: 'var(--syn-punct)' },
  { tag: [t.processingInstruction, t.meta, t.documentMeta], color: 'var(--syn-comment)' },
  { tag: t.link, color: 'var(--syn-property)', textDecoration: 'underline' },
  // Markdown
  { tag: t.heading1, color: 'var(--syn-keyword)', fontWeight: '700', fontSize: '1.3em' },
  { tag: t.heading2, color: 'var(--syn-keyword)', fontWeight: '700', fontSize: '1.15em' },
  { tag: t.heading3, color: 'var(--syn-keyword)', fontWeight: '600', fontSize: '1.05em' },
  { tag: t.heading, color: 'var(--syn-keyword)', fontWeight: '600' },
  { tag: t.strong, fontWeight: '600' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: [t.url, t.monospace], color: 'var(--syn-string)' },
  { tag: t.quote, color: 'var(--syn-comment)' },
  { tag: [t.contentSeparator, t.list], color: 'var(--syn-punct)' },
])

export const gooTheme = [editorTheme, syntaxHighlighting(highlight)]
