import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { GooBackend, Tree, Workspace } from './types'

/** The Rust side (src-tauri/src/lib.rs); argument names match the command parameters. */
export const tauriBackend: GooBackend = {
  kind: 'tauri',
  pickWorkspace: () => invoke<Workspace | null>('pick_workspace'),
  openWorkspace: (root) => invoke<Workspace>('open_workspace', { root }),
  recentWorkspaces: () => invoke<Workspace[]>('recent_workspaces'),
  forgetWorkspace: (root) => invoke<void>('forget_workspace', { root }),
  listTree: () => invoke<Tree>('list_tree'),
  readFile: (path) => invoke<string>('read_file', { path }),
  writeFile: (path, contents) => invoke<void>('write_file', { path, contents }),
  createFile: (dir, name, contents) => invoke<string>('create_file', { dir, name, contents }),
  openExternal: (url) => invoke<void>('open_external', { url }),
  setWindowTitle: (title) => getCurrentWindow().setTitle(title),
  guardClose(hasUnsaved, ask) {
    // The window closes after the handler unless it calls preventDefault (needs core:window:allow-destroy).
    void getCurrentWindow().onCloseRequested(async (event) => {
      if (hasUnsaved() && !(await ask())) event.preventDefault()
    })
  },
}

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}
