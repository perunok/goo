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
  let open: Workspace | null = null
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
    // A browser has no "Open with"; nothing is ever pending.
    async takeOpenRequest() {
      return null
    },
    async dropOpenRequest() {},
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
