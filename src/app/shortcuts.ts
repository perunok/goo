/** App-wide keyboard shortcuts. Editing shortcuts (bold, italic, link, search) live in the editor keymap. */
import { settings } from './settings.svelte'
import { ui } from './ui.svelte'
import { workspace } from './workspace.svelte'

export const SHORTCUTS: { keys: string; label: string }[] = [
  { keys: 'Ctrl+S', label: 'Save' },
  { keys: 'Ctrl+Shift+S', label: 'Save all' },
  { keys: 'Ctrl+N', label: 'New note' },
  { keys: 'Ctrl+O', label: 'Open folder' },
  { keys: 'Ctrl+Shift+O', label: 'Open note' },
  { keys: 'Ctrl+F', label: 'Search notes (in the editor: find in this note)' },
  { keys: 'Ctrl+Shift+F', label: 'Search notes' },
  { keys: 'Ctrl+W', label: 'Close tab' },
  { keys: 'Ctrl+Tab', label: 'Next tab' },
  { keys: 'Ctrl+E', label: 'Cycle edit / split / preview' },
  { keys: 'Ctrl+\\', label: 'Toggle sidebar' },
  { keys: 'Ctrl+,', label: 'Settings' },
  { keys: 'Ctrl+= / Ctrl+-', label: 'Font size' },
  { keys: 'Ctrl+B / Ctrl+I / Ctrl+K', label: 'Bold / italic / link (editor)' },
]

export function handleShortcut(e: KeyboardEvent): boolean {
  const mod = e.ctrlKey || e.metaKey
  if (!mod || e.altKey) return false
  const key = e.key.toLowerCase()
  const hasFolder = !!workspace.workspace
  // While a dialog is open only its own keys apply.
  if (ui.dialog || ui.confirmRequest || ui.promptRequest) return false
  switch (true) {
    case key === 's' && e.shiftKey:
      void workspace.saveAll()
      return true
    case key === 's':
      void workspace.save()
      return true
    case key === 'n' && hasFolder:
      void workspace.newNote()
      return true
    case key === 'o' && e.shiftKey:
      void workspace.pickFile()
      return true
    case key === 'o':
      void workspace.pickFolder()
      return true
    // In the editor, Ctrl+F is its own find-in-note bar (it handles the key first); everywhere else, search the notes.
    case key === 'f' && hasFolder && (e.shiftKey || !e.defaultPrevented):
      settings.setUi({ sidebar: true })
      ui.findPending = true
      return true
    case key === 'w' && !!workspace.activePath:
      void workspace.close(workspace.activePath!)
      return true
    case key === 'tab':
      workspace.cycleTab(e.shiftKey ? -1 : 1)
      return true
    case key === 'e':
      settings.cycleViewMode()
      return true
    case key === '\\':
      settings.setUi({ sidebar: !settings.ui.sidebar })
      return true
    case key === ',':
      ui.open('settings')
      return true
    case key === '=' || key === '+':
      settings.bumpFont(1)
      return true
    case key === '-':
      settings.bumpFont(-1)
      return true
    case key === '0':
      settings.setAppearance({ fontSize: 14 })
      return true
  }
  return false
}
