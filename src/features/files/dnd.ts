/** The note or folder being dragged in the tree (HTML drag data is not readable during dragover in every webview). */
export const dragged = { path: '' }

/** Whether `path` may be dropped into the folder `dir`: not onto itself, its own parent, or inside itself. */
export function canDrop(path: string, dir: string): boolean {
  if (!path || path === dir) return false
  const sep = path.includes('\\') && !path.includes('/') ? '\\' : '/'
  const parent = path.slice(0, path.lastIndexOf(sep))
  return parent !== dir && !dir.startsWith(path + sep)
}
