/**
 * In-memory backend for `npm run dev` in a plain browser (no Tauri) and for tests. Mirrors the Rust side's
 * behaviour where it matters to the UI: paths must be inside the open folder, names get ".md", no overwrites.
 */
import { SEED_FILES, SEED_ROOT } from './seed'
import type { BackendError, CloudStatus, ErrorCode, GooBackend, McpStatus, Tree, TreeNode, UpdateInstallState, UpdateMethod, Workspace } from './types'

const fail = (code: ErrorCode, message: string): never => {
  throw { code, message } satisfies BackendError
}

const MD = /\.(md|markdown|mdown|mkd)$/i

/** Knobs for the pretend updater (tests, and `?mockUpdate=none|manual` in the browser). */
export interface MockUpdates {
  /** The latest release GitHub would report; null: none published yet. */
  latest: string | null
  /** How this pretend installation updates itself; null: it cannot. */
  method: UpdateMethod | null
  /** Makes the next check fail with this message. */
  failCheck: string | null
  /** Milliseconds between download progress steps (12 steps). */
  tickMs: number
  /** Back to a fresh pretend installation: idle, no download running, the default knobs. */
  reset(): void
}

const compareVersions = (a: string, b: string) => {
  const x = a.split('.').map(Number)
  const y = b.split('.').map(Number)
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0)
  return 0
}

const nextMinor = (v: string) => {
  const [major, minor] = v.split('.')
  return `${major}.${Number(minor) + 1}.0`
}

export function createMockBackend(opts: { latencyMs?: number; files?: Record<string, string>; root?: string } = {}): GooBackend & {
  files: Map<string, string>
  updates: MockUpdates
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
  // In-app updates: a pretend .deb installation whose download takes about a second.
  const mode = typeof location !== 'undefined' ? new URLSearchParams(location.search).get('mockUpdate') : null
  const updates: MockUpdates = {
    latest: mode === 'none' ? __APP_VERSION__ : nextMinor(__APP_VERSION__),
    method: mode === 'manual' ? null : 'deb',
    failCheck: null,
    tickMs: 100,
    reset() {
      if (downloadTimer) clearInterval(downloadTimer)
      downloadTimer = null
      installState = { phase: 'idle', version: null, received: 0, total: null, bytesPerSec: null, error: null }
      Object.assign(updates, { latest: mode === 'none' ? __APP_VERSION__ : nextMinor(__APP_VERSION__), method: mode === 'manual' ? null : 'deb', failCheck: null, tickMs: 100 })
      for (const l of installListeners) l(installNow())
    },
  }
  let installState: Omit<UpdateInstallState, 'method' | 'unsupportedReason'> = { phase: 'idle', version: null, received: 0, total: null, bytesPerSec: null, error: null }
  const installListeners: ((s: UpdateInstallState) => void)[] = []
  let downloadTimer: ReturnType<typeof setInterval> | null = null
  const installNow = (): UpdateInstallState => ({
    ...installState,
    method: updates.method,
    unsupportedReason: updates.method ? null : 'This is a development build: update it from the source, or install a release.',
  })
  const setInstall = (patch: Partial<typeof installState>) => {
    installState = { ...installState, ...patch }
    for (const l of installListeners) l(installNow())
  }
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
    updates,
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
    async checkForUpdates() {
      await wait()
      if (updates.failCheck) fail('io', updates.failCheck)
      const current = __APP_VERSION__
      const latest = updates.latest
      const newer = latest !== null && compareVersions(latest, current) > 0
      return {
        currentVersion: current,
        latest: latest ? { version: latest, url: `https://github.com/perunok/goo/releases/tag/v${latest}`, publishedAt: Math.floor(Date.now() / 1000) - 3600 } : null,
        updateAvailable: newer,
        checkedAt: Math.floor(Date.now() / 1000),
      }
    },
    async updateInstallState() {
      return installNow()
    },
    async downloadUpdate() {
      if (!updates.method) fail('io', installNow().unsupportedReason ?? 'This installation cannot update itself.')
      if (['downloading', 'verifying', 'installing'].includes(installState.phase)) return installNow()
      const total = 78_900_000
      setInstall({ phase: 'downloading', version: updates.latest, received: 0, total, bytesPerSec: null, error: null })
      let step = 0
      downloadTimer = setInterval(() => {
        step++
        if (step < 12) setInstall({ received: Math.round((total * step) / 12), bytesPerSec: Math.round(total / 12 / (updates.tickMs / 1000)) })
        else {
          clearInterval(downloadTimer!)
          downloadTimer = null
          setInstall({ phase: 'verifying', received: total, bytesPerSec: null })
          setTimeout(() => installState.phase === 'verifying' && setInstall({ phase: 'ready' }), updates.tickMs)
        }
      }, updates.tickMs)
      return installNow()
    },
    async cancelUpdateDownload() {
      if (installState.phase === 'downloading') {
        if (downloadTimer) clearInterval(downloadTimer)
        downloadTimer = null
        setInstall({ phase: 'idle', received: 0, total: null, bytesPerSec: null, error: null })
      }
      return installNow()
    },
    async installUpdate() {
      if (installState.phase !== 'ready') fail('io', 'No downloaded update is ready to install.')
      setInstall({ phase: 'installing', error: null })
      // a browser tab cannot restart itself
      setTimeout(() => setInstall({ phase: 'error', error: 'Installing only works in the desktop app: this is the browser demo.' }), 800)
      return installNow()
    },
    onUpdateState(handler) {
      installListeners.push(handler)
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
