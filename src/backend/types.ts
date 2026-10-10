/**
 * Everything the UI asks of the outside world. Two implementations: `tauri.ts` (the Rust commands in
 * src-tauri/src/lib.rs) and `mock.ts` (in-memory, for `npm run dev` in a plain browser and for tests).
 */
export interface Workspace {
  /** Absolute path of the folder. */
  root: string
  name: string
}

export interface TreeNode {
  name: string
  /** Absolute path. */
  path: string
  kind: 'dir' | 'file'
  children?: TreeNode[]
}

export interface Tree {
  nodes: TreeNode[]
  /** The folder had more entries than goo lists. */
  truncated: boolean
}

export type ErrorCode =
  | 'not_found'
  | 'outside_workspace'
  | 'no_workspace'
  | 'invalid_name'
  | 'already_exists'
  | 'too_large'
  | 'not_text'
  | 'io'

/** The error shape the Rust side sends (goo_core::CoreError). */
export interface BackendError {
  code: ErrorCode
  message: string
}

/** A file or folder opened from outside the app ("Open with goo", `goo notes.md`), once taken. */
export interface OpenRequest {
  workspace: Workspace
  /** The notes to open (several when a selection was opened); empty when a folder was opened. */
  files: string[]
  /** Paths left out: not Markdown, or in another folder than the rest of the selection. */
  skipped: string[]
  /** The open folder changed. */
  switched: boolean
}

/** Announces an open request while the app runs; `switch` = it leaves the current folder. */
export interface OpenEvent {
  path: string
  switch: boolean
}

/** AI assistant access (MCP), see src-tauri/src/mcp.rs. */
export interface McpStatus {
  enabled: boolean
  running: boolean
  port: number
  url: string | null
  error: string | null
  token: string
  /** What assistants run: `command args...`. */
  command: string
  args: string[]
  devBuild: boolean
  claudeCode: { found: boolean; connected: 'no' | 'yes' | 'outdated' }
}

/** Notes an AI assistant created or changed on disk. */
export interface FilesChanged {
  root: string
  files: string[]
  /** Folders changed too (from folder watching): refresh the tree. */
  tree?: boolean
}

/** goo cloud (src-tauri/src/cloud.rs). */
export interface CloudFolderStatus {
  state: 'idle' | 'syncing' | 'offline' | 'error' | 'signed_out'
  lastSyncedAt: number | null
  message: string | null
  conflicts: { path: string; conflict_path: string }[]
  quarantined: { path: string; reason: string; message: string }[]
  deferred: string[]
  notUploaded: string[]
}

export interface CloudLink {
  root: string
  workspaceId: string
  workspaceName: string
  role: 'owner' | 'admin' | 'editor' | 'viewer' | string
  status: CloudFolderStatus
}

export interface CloudStatus {
  account: { serverUrl: string; email: string; displayName: string } | null
  /** A device sign-in waiting for approval in the browser. */
  signIn: { serverUrl: string; userCode: string; verificationUri: string; verificationUriComplete: string } | null
  signInError: string | null
  links: CloudLink[]
}

export interface CloudWorkspace {
  id: string
  name: string
  role: string
}

export interface GooBackend {
  readonly kind: 'tauri' | 'mock'
  /** Native folder picker; null when cancelled. */
  pickWorkspace(): Promise<Workspace | null>
  /** Reopen a folder from the recent list. */
  openWorkspace(root: string): Promise<Workspace>
  recentWorkspaces(): Promise<Workspace[]>
  forgetWorkspace(root: string): Promise<void>
  listTree(): Promise<Tree>
  readFile(path: string): Promise<string>
  writeFile(path: string, contents: string): Promise<void>
  /** Creates a note (".md" appended when missing) and returns its path. */
  createFile(dir: string, name: string, contents: string): Promise<string>
  /** Opens an http(s)/mailto link outside the app. */
  openExternal(url: string): Promise<void>
  setWindowTitle(title: string): Promise<void>
  /**
   * Called when the user closes the window: when `hasUnsaved()` is true, `ask()` decides (true = close anyway).
   * In a browser only the native "leave site?" prompt is possible.
   */
  guardClose(hasUnsaved: () => boolean, ask: () => Promise<boolean>): void
  /** Claims the pending open request (from the launch arguments or the last `open-request` event). */
  takeOpenRequest(): Promise<OpenRequest | null>
  /** Discards it (the user chose to stay). */
  dropOpenRequest(): Promise<void>
  /** Files opened while the app runs (second launch, macOS Finder), and paths that cannot be opened. */
  onOpenRequest(handler: (e: OpenEvent) => void, onError: (message: string) => void): void
  /** Tells the backend which notes have unsaved edits (AI assistants may not write them). */
  setUnsaved(paths: string[]): Promise<void>
  onFilesChanged(handler: (e: FilesChanged) => void): void
  /** An AI assistant is running a tool. */
  onMcpActivity(handler: (tool: string) => void): void
  mcpStatus(): Promise<McpStatus>
  mcpSetEnabled(enabled: boolean): Promise<McpStatus>
  mcpNewToken(): Promise<McpStatus>
  mcpConnectClaudeCode(): Promise<McpStatus>
  mcpDisconnectClaudeCode(): Promise<McpStatus>
  cloudStatus(): Promise<CloudStatus>
  onCloudStatus(handler: (s: CloudStatus) => void): void
  cloudSetUtcOffset(minutes: number): Promise<void>
  cloudSignIn(serverUrl: string): Promise<CloudStatus>
  cloudCancelSignIn(): Promise<CloudStatus>
  cloudSignOut(): Promise<CloudStatus>
  cloudWorkspaces(): Promise<CloudWorkspace[]>
  /** Links the open folder to an existing workspace, or to a new one called `newName`. */
  cloudLink(link: { workspaceId: string } | { newName: string }): Promise<CloudStatus>
  cloudUnlink(root: string): Promise<CloudStatus>
  /** Sync soon (after a save: that folder; no root: every linked folder). */
  cloudSyncNow(root?: string): Promise<void>
  /** Questions from the Rust side for AI assistants (src-tauri/src/uibridge.rs); the handler's result is the answer. */
  onUiQuery(handler: (kind: string, args: unknown) => Promise<unknown>): void
}

export function isBackendError(e: unknown): e is BackendError {
  return !!e && typeof e === 'object' && typeof (e as BackendError).code === 'string' && typeof (e as BackendError).message === 'string'
}

export function errorMessage(e: unknown): string {
  if (isBackendError(e)) return e.message
  if (e instanceof Error) return e.message
  return String(e)
}
