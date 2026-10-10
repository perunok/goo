<script lang="ts">
  /**
   * The Markdown source editor (CodeMirror 6). One view; each open document keeps its own EditorState (and so its
   * own undo history and selection) while you switch tabs.
   */
  import { onDestroy, onMount } from 'svelte'
  import { Compartment, EditorState } from '@codemirror/state'
  import { EditorView, drawSelection, highlightActiveLine, keymap, placeholder } from '@codemirror/view'
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
  import { bracketMatching, indentOnInput } from '@codemirror/language'
  import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
  import { languages } from '@codemirror/language-data'
  import { highlightSelectionMatches, searchKeymap } from '@codemirror/search'
  import { gooTheme } from './theme'
  import { insertLink, toggleWrap } from './commands'

  interface Props {
    docId: string
    text: string
    wrap: boolean
    /** Paths still open, so states of closed documents can be dropped. */
    openIds: string[]
    onChange: (text: string) => void
    /** Scroll position as 0..1 (for the split preview). */
    onScroll?: (ratio: number) => void
  }

  let { docId, text, wrap, openIds, onChange, onScroll }: Props = $props()

  let host: HTMLDivElement
  let view: EditorView | null = null
  let currentId = ''
  const states = new Map<string, EditorState>()
  const wrapping = new Compartment()

  function extensions() {
    return [
      history(),
      drawSelection(),
      highlightActiveLine(),
      indentOnInput(),
      bracketMatching(),
      highlightSelectionMatches(),
      EditorState.allowMultipleSelections.of(true),
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      wrapping.of(wrap ? EditorView.lineWrapping : []),
      placeholder('Start writing…'),
      keymap.of([
        { key: 'Mod-b', run: toggleWrap('**') },
        { key: 'Mod-i', run: toggleWrap('_') },
        { key: 'Mod-Shift-x', run: toggleWrap('~~') },
        { key: 'Mod-`', run: toggleWrap('`') },
        { key: 'Mod-k', run: insertLink },
        ...searchKeymap,
        ...historyKeymap,
        ...defaultKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) onChange(u.state.doc.toString())
      }),
      gooTheme,
    ]
  }

  function stateFor(id: string, value: string): EditorState {
    const saved = states.get(id)
    if (saved && saved.doc.toString() === value) return saved
    return EditorState.create({ doc: value, extensions: extensions() })
  }

  onMount(() => {
    currentId = docId
    view = new EditorView({ state: stateFor(docId, text), parent: host })
    view.scrollDOM.addEventListener('scroll', reportScroll, { passive: true })
    view.focus()
  })

  onDestroy(() => {
    view?.scrollDOM.removeEventListener('scroll', reportScroll)
    view?.destroy()
  })

  function reportScroll() {
    if (!view || !onScroll) return
    const el = view.scrollDOM
    const max = el.scrollHeight - el.clientHeight
    onScroll(max > 0 ? el.scrollTop / max : 0)
  }

  // Switching documents: park the old state, bring in the new one.
  $effect(() => {
    const id = docId
    const value = text
    if (!view || id === currentId) return
    states.set(currentId, view.state)
    for (const k of states.keys()) if (!openIds.includes(k)) states.delete(k)
    currentId = id
    view.setState(stateFor(id, value))
    view.dispatch({ effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []) })
    view.focus()
  })

  // Text changed from outside (not by typing here): replace the document, keeping undo history.
  $effect(() => {
    const value = text
    if (!view || docId !== currentId) return
    const current = view.state.doc.toString()
    if (current !== value) view.dispatch({ changes: { from: 0, to: current.length, insert: value } })
  })

  $effect(() => {
    view?.dispatch({ effects: wrapping.reconfigure(wrap ? EditorView.lineWrapping : []) })
  })

  /** For the outline / preview: put the cursor on a line and show it. */
  export function revealLine(line: number) {
    if (!view) return
    const l = view.state.doc.line(Math.min(Math.max(1, line), view.state.doc.lines))
    view.dispatch({ selection: { anchor: l.from }, scrollIntoView: true })
    view.focus()
  }
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--surface);
  }
  .editor :global(.cm-editor) {
    height: 100%;
  }
</style>
