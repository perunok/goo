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
  { keys: 'Ctrl+= / Ctrl+- / Ctrl+scroll', label: 'Font size' },
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
    case key === '\\' && hasFolder:
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

/** Scroll distance (px) for one font size step: a mouse wheel notch is at least this, a trackpad adds up to it. */
const ZOOM_WHEEL_STEP = 40
let zoomWheelDelta = 0

/**
 * Ctrl+scroll over the page: same zoom as Ctrl+= / Ctrl+-, one step at most per wheel event. Over the sidebar, tabs and
 * status bar it does nothing (they keep their size, see App.svelte). Plain scrolling is left alone.
 */
export function handleZoomWheel(e: WheelEvent) {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return
  e.preventDefault() // always, or the webview's own page zoom would apply (also behind a dialog)
  if (ui.dialog || ui.confirmRequest || ui.promptRequest) return
  if (!(e.target instanceof Element && e.target.closest('.content'))) return
  const px = e.deltaY * (e.deltaMode === e.DOM_DELTA_LINE ? ZOOM_WHEEL_STEP : e.deltaMode === e.DOM_DELTA_PAGE ? 10 * ZOOM_WHEEL_STEP : 1)
  if (Math.sign(px) !== Math.sign(zoomWheelDelta)) zoomWheelDelta = 0
  zoomWheelDelta += px
  if (Math.abs(zoomWheelDelta) < ZOOM_WHEEL_STEP) return
  settings.bumpFont(zoomWheelDelta < 0 ? 1 : -1)
  zoomWheelDelta = 0
}
