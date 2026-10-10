/**
 * Where the cursor/selection is in the note being edited, for AI assistants ("the selection"). A plain object, not
 * reactive state: it changes on every cursor move and nothing on screen depends on it.
 */
export const editorSelection = {
  path: '',
  from: 0,
  to: 0,
  set(path: string, from: number, to: number) {
    this.path = path
    this.from = from
    this.to = to
  },
}

/** 1-based line of a character offset. */
export function lineAt(text: string, offset: number): number {
  let line = 1
  for (let i = 0; i < Math.min(offset, text.length); i++) if (text.charCodeAt(i) === 10) line++
  return line
}
