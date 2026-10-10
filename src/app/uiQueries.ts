/**
 * Answers questions the Rust side asks the window for AI assistants (src-tauri/src/uibridge.rs): the note on
 * screen with its unsaved text and selection, and whether Mermaid diagrams render.
 */
import { editorSelection, lineAt } from '../components/editor/selection'
import { mermaidError } from '../components/markdown/mermaid'
import { isDirty, workspace } from './workspace.svelte'

export async function answerUiQuery(kind: string, args: unknown): Promise<unknown> {
  switch (kind) {
    case 'open_note': {
      const d = workspace.active
      if (!d) return { path: null }
      const sel = editorSelection.path === d.path ? editorSelection : { from: 0, to: 0 }
      const from = Math.min(sel.from, sel.to, d.text.length)
      const to = Math.min(Math.max(sel.from, sel.to), d.text.length)
      return {
        path: d.path,
        text: d.text,
        dirty: isDirty(d),
        cursor_line: lineAt(d.text, to),
        selection: from === to ? null : { text: d.text.slice(from, to), from_line: lineAt(d.text, from), to_line: lineAt(d.text, to) },
      }
    }
    case 'check_mermaid': {
      const sources = (args as { sources?: unknown })?.sources
      if (!Array.isArray(sources)) throw new Error('sources must be a list')
      const out: (string | null)[] = []
      for (const s of sources) out.push(await mermaidError(String(s)))
      return out
    }
    default:
      throw new Error(`unknown question "${kind}"`)
  }
}
