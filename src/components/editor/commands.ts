/** Small Markdown editing commands for the editor keymap. */
import { EditorSelection, type StateCommand } from '@codemirror/state'

/** Wraps each selection in `mark` (e.g. `**`), or removes it when the selection is already wrapped. */
export function toggleWrap(mark: string): StateCommand {
  return ({ state, dispatch }) => {
    const n = mark.length
    const tr = state.changeByRange((range) => {
      const before = state.sliceDoc(range.from - n, range.from)
      const after = state.sliceDoc(range.to, range.to + n)
      if (before === mark && after === mark) {
        return {
          changes: [
            { from: range.from - n, to: range.from },
            { from: range.to, to: range.to + n },
          ],
          range: EditorSelection.range(range.from - n, range.to - n),
        }
      }
      return {
        changes: [
          { from: range.from, insert: mark },
          { from: range.to, insert: mark },
        ],
        range: EditorSelection.range(range.from + n, range.to + n),
      }
    })
    dispatch(state.update(tr, { scrollIntoView: true, userEvent: 'input' }))
    return true
  }
}

/** Turns each selection into `[text](url)` with the cursor on the URL placeholder. */
export const insertLink: StateCommand = ({ state, dispatch }) => {
  const tr = state.changeByRange((range) => {
    const text = state.sliceDoc(range.from, range.to) || 'text'
    const insert = `[${text}](url)`
    const urlFrom = range.from + text.length + 3
    return { changes: { from: range.from, to: range.to, insert }, range: EditorSelection.range(urlFrom, urlFrom + 3) }
  })
  dispatch(state.update(tr, { scrollIntoView: true, userEvent: 'input' }))
  return true
}
