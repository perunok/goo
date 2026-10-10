/**
 * Path helpers for the absolute paths the backend hands out. They work for both POSIX (`/home/a/b.md`) and
 * Windows (`C:\Users\a\b.md`) paths; the separator is taken from the path itself.
 */
export function sepOf(path: string): '/' | '\\' {
  return path.includes('\\') && !path.includes('/') ? '\\' : '/'
}

export function basename(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return path.slice(i + 1)
}

export function dirname(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return i <= 0 ? path.slice(0, i + 1) : path.slice(0, i)
}

/** "notes/todo.md" -> "todo" */
export function stem(path: string): string {
  return basename(path).replace(/\.(md|markdown|mdown|mkd)$/i, '')
}

/** `path` relative to `root` with `/` separators, or the path itself when it is not inside `root`. */
export function relativeTo(root: string, path: string): string {
  const sep = sepOf(root)
  const prefix = root.endsWith(sep) ? root : root + sep
  return path.startsWith(prefix) ? path.slice(prefix.length).split(sep).join('/') : path
}

/**
 * Resolves a relative link from a note (`../ideas.md#plans`, `Diagrams.md`) against the note's folder.
 * Returns the absolute path and the fragment, or null for anything that is not a relative file link.
 */
export function resolveLink(fromFile: string, href: string): { path: string; fragment: string | null } | null {
  if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('#') || href.startsWith('/') || href.startsWith('\\')) return null
  const hash = href.indexOf('#')
  const rawPath = hash >= 0 ? href.slice(0, hash) : href
  const fragment = hash >= 0 ? href.slice(hash + 1) : null
  let decoded: string
  try {
    decoded = decodeURIComponent(rawPath.split('?')[0]!)
  } catch {
    decoded = rawPath
  }
  const sep = sepOf(fromFile)
  const parts = dirname(fromFile).split(sep)
  for (const seg of decoded.split(/[/\\]/)) {
    if (seg === '' || seg === '.') continue
    if (seg === '..') {
      if (parts.length > 1) parts.pop()
    } else parts.push(seg)
  }
  return { path: parts.join(sep), fragment: fragment ? decodeURIComponent(fragment) : null }
}

export function isMarkdownPath(path: string): boolean {
  return /\.(md|markdown|mdown|mkd)$/i.test(path)
}
