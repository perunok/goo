import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { CloudStatus, CloudWorkspace, FilesChanged, GooBackend, McpStatus, OpenEvent, OpenRequest, Tree, Workspace } from './types'

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
  takeOpenRequest: () => invoke<OpenRequest | null>('take_open_request'),
  dropOpenRequest: () => invoke<void>('drop_open_request'),
  onOpenRequest(handler, onError) {
    void listen<OpenEvent>('open-request', (e) => handler(e.payload))
    void listen<string>('open-error', (e) => onError(e.payload))
  },
  setUnsaved: (paths) => invoke<void>('set_unsaved', { paths }),
  onFilesChanged(handler) {
    void listen<FilesChanged>('files-changed', (e) => handler(e.payload))
  },
  onMcpActivity(handler) {
    void listen<string>('mcp-activity', (e) => handler(e.payload))
  },
  mcpStatus: () => invoke<McpStatus>('mcp_status'),
  mcpSetEnabled: (enabled) => invoke<McpStatus>('mcp_set_enabled', { enabled }),
  mcpNewToken: () => invoke<McpStatus>('mcp_new_token'),
  mcpConnectClaudeCode: () => invoke<McpStatus>('mcp_connect_claude_code'),
  mcpDisconnectClaudeCode: () => invoke<McpStatus>('mcp_disconnect_claude_code'),
  cloudStatus: () => invoke<CloudStatus>('cloud_status'),
  onCloudStatus(handler) {
    void listen<CloudStatus>('cloud-status', (e) => handler(e.payload))
  },
  cloudSetUtcOffset: (minutes) => invoke<void>('cloud_set_utc_offset', { minutes }),
  cloudSignIn: (serverUrl) => invoke<CloudStatus>('cloud_sign_in', { serverUrl }),
  cloudCancelSignIn: () => invoke<CloudStatus>('cloud_cancel_sign_in'),
  cloudSignOut: () => invoke<CloudStatus>('cloud_sign_out'),
  cloudWorkspaces: () => invoke<CloudWorkspace[]>('cloud_workspaces'),
  cloudLink: (link) =>
    invoke<CloudStatus>('cloud_link', 'workspaceId' in link ? { workspaceId: link.workspaceId, newName: null } : { workspaceId: null, newName: link.newName }),
  cloudUnlink: (root) => invoke<CloudStatus>('cloud_unlink', { root }),
  cloudSyncNow: (root) => invoke<void>('cloud_sync_now', { root: root ?? null }),
}

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}
