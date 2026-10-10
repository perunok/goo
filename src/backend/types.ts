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
}

export function isBackendError(e: unknown): e is BackendError {
  return !!e && typeof e === 'object' && typeof (e as BackendError).code === 'string' && typeof (e as BackendError).message === 'string'
}

export function errorMessage(e: unknown): string {
  if (isBackendError(e)) return e.message
  if (e instanceof Error) return e.message
  return String(e)
}
