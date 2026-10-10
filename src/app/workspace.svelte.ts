/**
 * The open folder, its file tree and the open documents (tabs). All I/O goes through `backend`; failures become
 * toasts, never unhandled rejections.
 */
import type { Tree, TreeNode, Workspace } from '../backend/types'
import { errorMessage } from '../backend/types'
import { basename, relativeTo, stem } from '../lib/paths'
import { backend } from './backend'
import { ui } from './ui.svelte'

export interface Doc {
  path: string
  name: string
  text: string
  savedText: string
  /** Fragment to scroll to once the preview shows this doc (from a link like `x.md#plans`). */
  pendingAnchor: string | null
}

const LAST_KEY = 'goo.lastSession'

interface LastSession {
  root: string
  open: string[]
  active: string | null
}

export function isDirty(d: Doc): boolean {
  return d.text !== d.savedText
}

class WorkspaceStore {
  workspace = $state<Workspace | null>(null)
  tree = $state<Tree | null>(null)
  recents = $state<Workspace[]>([])
  docs = $state<Doc[]>([])
  activePath = $state<string | null>(null)
  /** Folders collapsed in the tree (absolute paths). */
  collapsed = $state<Set<string>>(new Set())
  loading = $state(false)

  active = $derived(this.docs.find((d) => d.path === this.activePath) ?? null)
  dirtyCount = $derived(this.docs.filter(isDirty).length)

  async init() {
    backend.guardClose(
      () => this.dirtyCount > 0,
      async () => (await this.confirmDiscard(this.docs.filter(isDirty))) !== 'cancel',
    )
    await this.loadRecents()
    // Reopen where the user left off (only folders from the recent list are accepted by the backend).
    const last = readLast()
    if (last && this.recents.some((r) => r.root === last.root)) {
      if (await this.openRecent(last.root, false)) {
        for (const p of last.open) await this.openFile(p, { quiet: true, activate: false })
        const active = last.active && this.docs.some((d) => d.path === last.active) ? last.active : this.docs[0]?.path
        this.activePath = active ?? null
      }
    }
    this.persist()
  }

  async loadRecents() {
    try {
      this.recents = await backend.recentWorkspaces()
    } catch (e) {
      this.recents = []
      ui.toast(errorMessage(e), 'error')
    }
  }

  /** Native folder picker. */
  async pickFolder() {
    if (!(await this.confirmLeave())) return
    try {
      const ws = await backend.pickWorkspace()
      if (ws) await this.activate(ws)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  async openRecent(root: string, confirm = true): Promise<boolean> {
    if (confirm && !(await this.confirmLeave())) return false
    try {
      await this.activate(await backend.openWorkspace(root))
      return true
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
      await this.loadRecents()
      return false
    }
  }

  async forget(root: string) {
    try {
      await backend.forgetWorkspace(root)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
    await this.loadRecents()
  }

  closeFolder = async () => {
    if (!(await this.confirmLeave())) return
    this.workspace = null
    this.tree = null
    this.docs = []
    this.activePath = null
    this.persist()
    void this.loadRecents()
  }

  private async activate(ws: Workspace) {
    this.workspace = ws
    this.docs = []
    this.activePath = null
    this.collapsed = new Set()
    await this.refreshTree()
    void this.loadRecents()
    this.persist()
  }

  async refreshTree() {
    if (!this.workspace) return
    this.loading = true
    try {
      this.tree = await backend.listTree()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    } finally {
      this.loading = false
    }
  }

  toggleFolder(path: string) {
    const next = new Set(this.collapsed)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    this.collapsed = next
  }

  // -------------------------------------------------------------------------
  // Documents
  // -------------------------------------------------------------------------

  async openFile(path: string, opts: { anchor?: string | null; quiet?: boolean; activate?: boolean } = {}): Promise<boolean> {
    const existing = this.docs.find((d) => d.path === path)
    if (existing) {
      existing.pendingAnchor = opts.anchor ?? null
      if (opts.activate !== false) this.activePath = path
      this.persist()
      return true
    }
    try {
      const text = await backend.readFile(path)
      this.docs = [...this.docs, { path, name: basename(path), text, savedText: text, pendingAnchor: opts.anchor ?? null }]
      if (opts.activate !== false) this.activePath = path
      this.persist()
      return true
    } catch (e) {
      if (!opts.quiet) ui.toast(errorMessage(e), 'error')
      return false
    }
  }

  setText(path: string, text: string) {
    const d = this.docs.find((x) => x.path === path)
    if (d && d.text !== text) d.text = text
  }

  async save(path = this.activePath): Promise<boolean> {
    const d = this.docs.find((x) => x.path === path)
    if (!d) return false
    if (!isDirty(d)) return true
    const text = d.text
    try {
      await backend.writeFile(d.path, text)
      d.savedText = text
      return true
    } catch (e) {
      ui.toast(`Could not save ${d.name}: ${errorMessage(e)}`, 'error')
      return false
    }
  }

  async saveAll(): Promise<boolean> {
    let ok = true
    for (const d of this.docs.filter(isDirty)) ok = (await this.save(d.path)) && ok
    return ok
  }

  async close(path: string) {
    const d = this.docs.find((x) => x.path === path)
    if (!d) return
    if (isDirty(d)) {
      const choice = await this.confirmDiscard([d])
      if (choice === 'cancel') return
    }
    const i = this.docs.indexOf(d)
    this.docs = this.docs.filter((x) => x !== d)
    if (this.activePath === path) this.activePath = (this.docs[i] ?? this.docs[i - 1])?.path ?? null
    this.persist()
  }

  select(path: string) {
    this.activePath = path
    this.persist()
  }

  cycleTab(delta: number) {
    if (this.docs.length < 2) return
    const i = this.docs.findIndex((d) => d.path === this.activePath)
    this.select(this.docs[(i + delta + this.docs.length) % this.docs.length]!.path)
  }

  /** Asks for a name and creates a note in `dir` (default: the folder's root). */
  async newNote(dir?: string) {
    const ws = this.workspace
    if (!ws) return
    const target = dir ?? ws.root
    const where = relativeTo(ws.root, target)
    const name = await ui.prompt({
      title: 'New note',
      label: where === target || where === '' ? `In ${ws.name}` : `In ${ws.name}/${where}`,
      value: 'Untitled',
      confirmLabel: 'Create',
    })
    if (name === null) return
    try {
      const path = await backend.createFile(target, name, `# ${stem(name) || name}\n\n`)
      await this.refreshTree()
      await this.openFile(path)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  // -------------------------------------------------------------------------
  // Helpers
  // -------------------------------------------------------------------------

  /** Before switching or closing the folder: offers to save unsaved notes. */
  private async confirmLeave(): Promise<boolean> {
    const dirty = this.docs.filter(isDirty)
    if (dirty.length === 0) return true
    return (await this.confirmDiscard(dirty)) !== 'cancel'
  }

  /** 'save' (and saved), 'discard', or 'cancel'. */
  private async confirmDiscard(dirty: Doc[]): Promise<'save' | 'discard' | 'cancel'> {
    const names = dirty.map((d) => d.name).join(', ')
    const choice = await ui.confirm({
      title: dirty.length === 1 ? 'Unsaved changes' : `${dirty.length} notes have unsaved changes`,
      message: `Save the changes to ${names}?`,
      actions: [
        { id: 'save', label: dirty.length === 1 ? 'Save' : 'Save all', kind: 'primary' },
        { id: 'discard', label: "Don't save", kind: 'danger' },
        { id: 'cancel', label: 'Cancel' },
      ],
    })
    if (choice === 'save') {
      let ok = true
      for (const d of dirty) ok = (await this.save(d.path)) && ok
      return ok ? 'save' : 'cancel'
    }
    return choice === 'discard' ? 'discard' : 'cancel'
  }

  private persist() {
    try {
      if (!this.workspace) localStorage.removeItem(LAST_KEY)
      else {
        const last: LastSession = { root: this.workspace.root, open: this.docs.map((d) => d.path), active: this.activePath }
        localStorage.setItem(LAST_KEY, JSON.stringify(last))
      }
    } catch {
      /* storage blocked: nothing to restore next time */
    }
  }
}

function readLast(): LastSession | null {
  try {
    const v = JSON.parse(localStorage.getItem(LAST_KEY) ?? 'null') as LastSession | null
    return v && typeof v.root === 'string' && Array.isArray(v.open) ? v : null
  } catch {
    return null
  }
}

/** Flat list of files in a tree (for filtering). */
export function flattenFiles(nodes: TreeNode[], out: TreeNode[] = []): TreeNode[] {
  for (const n of nodes) {
    if (n.kind === 'file') out.push(n)
    else if (n.children) flattenFiles(n.children, out)
  }
  return out
}

export const workspace = new WorkspaceStore()
