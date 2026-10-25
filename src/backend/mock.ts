/**
 * In-memory backend for `npm run dev` in a plain browser (no Tauri) and for tests. Mirrors the Rust side's
 * behaviour where it matters to the UI: paths must be inside the open folder, names get ".md", no overwrites.
 */
import { SEED_FILES, SEED_ROOT } from './seed'
import type { BackendError, CloudStatus, ErrorCode, GooBackend, McpStatus, Tree, TreeNode, Workspace } from './types'

const fail = (code: ErrorCode, message: string): never => {
  throw { code, message } satisfies BackendError
}

const MD = /\.(md|markdown|mdown|mkd)$/i

export function createMockBackend(opts: { latencyMs?: number; files?: Record<string, string>; root?: string } = {}): GooBackend & {
  files: Map<string, string>
} {
  const root = opts.root ?? SEED_ROOT
  const files = new Map<string, string>(Object.entries(opts.files ?? SEED_FILES).map(([rel, text]) => [`${root}/${rel}`, text]))
  const workspace: Workspace = { root, name: root.split('/').pop()! }
  /** Empty folders (folders with notes come from the note paths). */
  const folders = new Set<string>()
  let open: Workspace | null = null
  /** Notes picked with "Open note", waiting for `takeOpenRequest`. */
  let pending: string[] = []
  // MCP needs the desktop app; the browser mock only shows the settings UI.
  const mcp: McpStatus = {
    enabled: false,
    running: false,
    port: 7355,
    url: null,
    error: null,
    token: 'goo_mock-token',
    command: '/usr/bin/goo',
    args: ['--mcp-stdio'],
    devBuild: true,
    claudeCode: { found: false, connected: 'no' },
  }
  // goo cloud: a pretend server, so the settings UI can be tried in the browser.
  let cloud: CloudStatus = { account: null, signIn: null, signInError: null, links: [] }
  const cloudListeners: ((s: CloudStatus) => void)[] = []
  const cloudChanged = () => {
    cloud = structuredClone(cloud)
    for (const l of cloudListeners) l(cloud)
    return cloud
  }
  const cloudWorkspaces = [
    { id: 'ws-team', name: 'Team notes', role: 'editor' },
    { id: 'ws-read', name: 'Handbook', role: 'viewer' },
  ]
  const mcpNow = () => ({ ...mcp, url: mcp.running ? `http://127.0.0.1:${mcp.port}/mcp` : null })
  const wait = () => new Promise((r) => setTimeout(r, opts.latencyMs ?? 0))

  const inside = (path: string) => {
    if (!open) fail('no_workspace', 'No folder is open')
    if (!path.startsWith(open!.root + '/') || path.split('/').includes('..')) fail('outside_workspace', `${path} is outside the open folder`)
    return path
  }

  const validName = (name: string) => {
    const n = name.trim()
    if (!n || /[/\\:*?"<>|]/.test(n) || n === '.' || n === '..') fail('invalid_name', `"${name}" is not a valid name`)
    return n
  }
  const exists = (path: string) =>
    files.has(path) || folders.has(path) || [...files.keys(), ...folders].some((p) => p.startsWith(path + '/'))

  /** Renames/moves a note or a whole folder (every path under it). */
  function relocate(from: string, to: string): string {
    if (to === from) return to
    if (exists(to) && to.toLowerCase() !== from.toLowerCase()) fail('already_exists', `${to} already exists`)
    for (const [p, text] of [...files]) {
      if (p === from || p.startsWith(from + '/')) {
        files.delete(p)
        files.set(to + p.slice(from.length), text)
      }
    }
    for (const f of [...folders]) {
      if (f === from || f.startsWith(from + '/')) {
        folders.delete(f)
        folders.add(to + f.slice(from.length))
      }
    }
    return to
  }

  function tree(): Tree {
    const top: TreeNode[] = []
    const dirs = new Map<string, TreeNode>()
    const dirNode = (path: string): TreeNode[] => {
      if (path === root) return top
      let node = dirs.get(path)
      if (!node) {
        const parent = path.slice(0, path.lastIndexOf('/'))
        node = { name: path.slice(parent.length + 1), path, kind: 'dir', children: [] }
        dirs.set(path, node)
        dirNode(parent).push(node)
      }
      return node.children!
    }
    for (const path of files.keys()) {
      const parent = path.slice(0, path.lastIndexOf('/'))
      dirNode(parent).push({ name: path.slice(parent.length + 1), path, kind: 'file' })
    }
    for (const dir of folders) dirNode(dir)
    const sort = (nodes: TreeNode[]) => {
      nodes.sort((a, b) => (a.kind === b.kind ? a.name.toLowerCase().localeCompare(b.name.toLowerCase()) : a.kind === 'dir' ? -1 : 1))
      for (const n of nodes) if (n.children) sort(n.children)
      return nodes
    }
    return { nodes: sort(top), truncated: false }
  }

  return {
    kind: 'mock',
    files,
    async pickWorkspace() {
      await wait()
      open = workspace
      return workspace
    },
    async pickFile() {
      await wait()
      // A browser has no file picker: the last note stands in for the user's choice.
      const note = [...files.keys()].pop()
      if (!note) return null
      pending = [note]
      return { path: root, switch: open?.root !== root }
    },
    async openWorkspace(r) {
      await wait()
      if (r !== root) fail('outside_workspace', 'Pick this folder with Open Folder first')
      open = workspace
      return workspace
    },
    async recentWorkspaces() {
      return [workspace]
    },
    async forgetWorkspace() {},
    async listTree() {
      await wait()
      if (!open) fail('no_workspace', 'No folder is open')
      return tree()
    },
    async findNotes(query) {
      await wait()
      if (!open) fail('no_workspace', 'No folder is open')
      const words = query.toLowerCase().split(/\s+/).filter(Boolean)
      if (!words.length) fail('invalid_name', 'the search text is empty')
      const notes: string[] = []
      const walk = (nodes: TreeNode[]) => nodes.forEach((n) => (n.children ? walk(n.children) : notes.push(n.path)))
      walk(tree().nodes)
      return notes
        .filter((p) => {
          const name = p.slice(root.length + 1).toLowerCase()
          const text = files.get(p)!.toLowerCase()
          return words.every((w) => name.includes(w) || text.includes(w))
        })
        .slice(0, 200)
        .map((path) => ({
          path,
          lines: files
            .get(path)!
            .split('\n')
            .map((text, i) => ({ line: i + 1, text: text.trim() }))
            .filter((l) => words.some((w) => l.text.toLowerCase().includes(w)))
            .slice(0, 3),
        }))
    },
    async readFile(path) {
      await wait()
      const text = files.get(inside(path))
      if (text === undefined) fail('not_found', `${path} does not exist`)
      return text!
    },
    async writeFile(path, contents) {
      await wait()
      if (!files.has(inside(path))) fail('not_found', `${path} does not exist`)
      files.set(path, contents)
    },
    async createFile(dir, name, contents) {
      await wait()
      const n = name.trim()
      if (!n || /[/\\:*?"<>|]/.test(n) || n === '.' || n === '..') fail('invalid_name', `"${name}" is not a valid name`)
      const path = inside(`${dir}/${MD.test(n) ? n : `${n}.md`}`)
      if (files.has(path)) fail('already_exists', `${path} already exists`)
      files.set(path, contents)
      return path
    },
    async renameEntry(path, newName) {
      await wait()
      const n = validName(newName)
      const parent = inside(path).slice(0, path.lastIndexOf('/'))
      const isNote = files.has(path)
      const to = `${parent}/${isNote && !MD.test(n) ? `${n}.md` : n}`
      return relocate(path, to)
    },
    async moveEntry(path, destDir) {
      await wait()
      inside(path)
      if (destDir !== open!.root) inside(destDir)
      if (destDir === path || destDir.startsWith(path + '/')) fail('invalid_name', 'a folder cannot move into itself')
      return relocate(path, `${destDir}/${path.slice(path.lastIndexOf('/') + 1)}`)
    },
    async createFolder(dir, name) {
      await wait()
      const n = validName(name)
      const path = `${dir === open?.root ? dir : inside(dir)}/${n}`
      if (exists(path)) fail('already_exists', `${path} already exists`)
      folders.add(path)
      return path
    },
    async deleteEntry(path) {
      await wait()
      inside(path)
      if (!exists(path)) fail('not_found', `${path} does not exist`)
      for (const p of [...files.keys()]) if (p === path || p.startsWith(path + '/')) files.delete(p)
      for (const f of [...folders]) if (f === path || f.startsWith(path + '/')) folders.delete(f)
    },
    async revealEntry() {},
    onFilesMoved() {},
    async openExternal(url) {
      window.open(url, '_blank', 'noopener,noreferrer')
    },
    async setWindowTitle(title) {
      document.title = title
    },
    guardClose(hasUnsaved) {
      window.addEventListener('beforeunload', (e) => {
        if (hasUnsaved()) e.preventDefault()
      })
    },
    // A browser has no "Open with"; only "Open note" queues something.
    async takeOpenRequest() {
      const picked = pending
      pending = []
      if (!picked.length) return null
      const switched = open?.root !== root
      open = workspace
      return { workspace, files: picked, skipped: [], switched }
    },
    async dropOpenRequest() {
      pending = []
    },
    onOpenRequest() {},
    async setUnsaved() {},
    onFilesChanged() {},
    onMcpActivity() {},
    async mcpStatus() {
      return mcpNow()
    },
    async mcpSetEnabled(enabled) {
      mcp.enabled = enabled
      mcp.running = enabled
      return mcpNow()
    },
    async mcpNewToken() {
      mcp.token = `goo_mock-${Math.random().toString(16).slice(2)}`
      return mcpNow()
    },
    async mcpConnectClaudeCode() {
      return fail('io', 'Claude Code is only connected from the desktop app')
    },
    async mcpDisconnectClaudeCode() {
      return mcpNow()
    },
    async cloudStatus() {
      return cloud
    },
    onCloudStatus(handler) {
      cloudListeners.push(handler)
    },
    async cloudSetUtcOffset() {},
    onUiQuery() {},
    async cloudSignIn(serverUrl) {
      if (!/^https:\/\/./.test(serverUrl)) fail('invalid_name', 'Enter the server address, starting with https://')
      cloud.signIn = { serverUrl, userCode: 'GOO-4821', verificationUri: `${serverUrl}/device`, verificationUriComplete: `${serverUrl}/device?user_code=GOO-4821` }
      cloud.signInError = null
      setTimeout(() => {
        if (!cloud.signIn) return
        cloud.signIn = null
        cloud.account = { serverUrl, email: 'you@example.com', displayName: 'You' }
        cloudChanged()
      }, 2500)
      return cloudChanged()
    },
    async cloudCancelSignIn() {
      cloud.signIn = null
      return cloudChanged()
    },
    async cloudSignOut() {
      cloud = { account: null, signIn: null, signInError: null, links: [] }
      return cloudChanged()
    },
    async cloudWorkspaces() {
      await wait()
      return cloudWorkspaces
    },
    async cloudLink(link) {
      if (!open) fail('no_workspace', 'Open a folder first.')
      const ws = 'workspaceId' in link ? cloudWorkspaces.find((w) => w.id === link.workspaceId)! : { id: `ws-${Date.now()}`, name: link.newName, role: 'owner' }
      cloud.links = cloud.links.filter((l) => l.root !== open!.root)
      cloud.links.push({
        root: open!.root,
        workspaceId: ws.id,
        workspaceName: ws.name,
        role: ws.role,
        status: { state: 'idle', lastSyncedAt: Math.floor(Date.now() / 1000), message: null, conflicts: [], quarantined: [], deferred: [], notUploaded: [] },
      })
      return cloudChanged()
    },
    async cloudUnlink(root) {
      cloud.links = cloud.links.filter((l) => l.root !== root)
      return cloudChanged()
    },
    async cloudSyncNow() {
      for (const l of cloud.links) l.status = { ...l.status, state: 'idle', lastSyncedAt: Math.floor(Date.now() / 1000) }
      cloudChanged()
    },
  }
}
