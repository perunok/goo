/**
 * The open folder, its file tree and the open documents (tabs). All I/O goes through `backend`; failures become
 * toasts, never unhandled rejections.
 */
import type { FilesChanged, OpenEvent, OpenRequest, Tree, TreeNode, Workspace } from '../backend/types'
import { errorMessage, isBackendError } from '../backend/types'
import { basename, dirname, relativeTo, sepOf, stem } from '../lib/paths'
import { backend } from './backend'
import { settings } from './settings.svelte'
import { ui } from './ui.svelte'

export interface Doc {
  path: string
  name: string
  text: string
  savedText: string
  /** Fragment to scroll to once the preview shows this doc (from a link like `x.md#plans`). */
  pendingAnchor: string | null
  /** The file changed outside goo while this note had unsaved edits (saving asks before overwriting). */
  diskChanged: boolean
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
  /** The native folder picker is open (one at a time). */
  picking = false
  /** Called after every successful save (goo cloud syncs soon after). */
  onSaved: (() => void) | null = null

  active = $derived(this.docs.find((d) => d.path === this.activePath) ?? null)
  dirtyCount = $derived(this.docs.filter(isDirty).length)
  /** Paths with unsaved edits, as one string so effects only re-run when the set changes (not on every keystroke). */
  unsavedKey = $derived(
    this.docs
      .filter(isDirty)
      .map((d) => d.path)
      .join('\n'),
  )

  async init() {
    backend.guardClose(
      () => this.dirtyCount > 0,
      async () => (await this.confirmDiscard(this.docs.filter(isDirty))) !== 'cancel',
    )
    backend.onOpenRequest(
      (e) => void this.handleOpenEvent(e),
      (message) => ui.toast(message, 'error'),
    )
    backend.onFilesChanged((e) => void this.handleFilesChanged(e))
    backend.onFilesMoved((e) => {
      if (this.workspace && e.root === this.workspace.root) this.retarget(e.from, e.to)
    })
    backend.onMcpActivity((tool) => ui.assistantBusy(tool))
    await this.loadRecents()
    // Started with a file or folder ("Open with goo", `goo notes.md`): that wins over the last session.
    if (await this.takeOpenRequest()) return
    // Otherwise reopen where the user left off (only folders from the recent list are accepted by the backend).
    // (turned off in Settings: nothing is restored, and the stored session is cleared below)
    const last = settings.ui.restoreSession ? readLast() : null
    if (last && this.recents.some((r) => r.root === last.root)) {
      if (await this.openRecent(last.root, false)) {
        for (const p of last.open) await this.openFile(p, { quiet: true, activate: false })
        const active = last.active && this.docs.some((d) => d.path === last.active) ? last.active : this.docs[0]?.path
        this.activePath = active ?? null
      }
    }
    this.persist()
  }

  /** A file or folder opened from outside while the app runs. */
  private async handleOpenEvent(e: OpenEvent) {
    if (e.switch && !(await this.confirmLeave())) {
      await backend.dropOpenRequest().catch(() => {})
      return
    }
    await this.takeOpenRequest()
  }

  private async takeOpenRequest(): Promise<boolean> {
    let req: OpenRequest | null
    try {
      req = await backend.takeOpenRequest()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
      return false
    }
    if (!req) return false
    if (req.switched || this.workspace?.root !== req.workspace.root) await this.activate(req.workspace)
    for (const f of req.files) await this.openFile(f, { activate: f === req.files[0] })
    if (req.skipped.length) {
      const names = req.skipped.map(basename).join(', ')
      ui.toast(`Not opened (not a note in ${req.workspace.name}): ${names}`)
    }
    return true
  }

  /**
   * Notes changed on disk outside the editor: another program (folder watching), an AI assistant (MCP) or goo cloud.
   * The tree is refreshed; open notes without unsaved edits reload; notes with unsaved edits keep them and are marked,
   * so saving asks before overwriting. goo's own saves come back here too and are recognised (same text).
   */
  private async handleFilesChanged(e: FilesChanged) {
    if (!this.workspace || e.root !== this.workspace.root) return
    await this.refreshTree()
    for (const path of e.files) {
      const d = this.docs.find((x) => x.path === path)
      if (!d) continue
      let text: string
      try {
        text = await backend.readFile(path)
      } catch (err) {
        if (isBackendError(err) && err.code === 'not_found') {
          if (isDirty(d)) {
            d.diskChanged = true
            ui.toast(`${d.name} was moved or deleted outside goo; your unsaved text is still here.`)
          } else {
            ui.toast(`${d.name} was moved or deleted outside goo.`)
            await this.close(d.path)
          }
        } else ui.toast(errorMessage(err), 'error')
        continue
      }
      if (text === d.savedText) continue // our own save, or nothing new
      if (!isDirty(d)) {
        d.text = text
        d.savedText = text
        d.diskChanged = false
      } else if (!d.diskChanged) {
        d.diskChanged = true
        ui.toast(`${d.name} changed outside goo. Your unsaved edits are kept; saving will ask before overwriting.`)
      }
    }
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
    if (this.picking) return
    this.picking = true
    try {
      if (!(await this.confirmLeave())) return
      const ws = await backend.pickWorkspace()
      if (ws) await this.activate(ws)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    } finally {
      this.picking = false
    }
  }

  /**
   * Native file picker for notes. Opens like "Open with goo": in the open folder when the note is inside it,
   * otherwise in the note's own folder (asking about unsaved notes before switching).
   */
  async pickFile() {
    if (this.picking) return
    this.picking = true
    try {
      const picked = await backend.pickFile()
      if (picked) await this.handleOpenEvent(picked)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    } finally {
      this.picking = false
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
      this.docs = [...this.docs, { path, name: basename(path), text, savedText: text, pendingAnchor: opts.anchor ?? null, diskChanged: false }]
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
    if (d.diskChanged) {
      const choice = await ui.confirm({
        title: `${d.name} changed outside goo`,
        message: 'Another program changed this note after you started editing. Keep your version (overwrite theirs), or load theirs and drop your edits?',
        actions: [
          { id: 'mine', label: 'Keep mine', kind: 'primary' },
          { id: 'theirs', label: 'Load theirs', kind: 'danger' },
          { id: 'cancel', label: 'Cancel' },
        ],
      })
      if (choice === 'theirs') {
        try {
          const disk = await backend.readFile(d.path)
          d.text = disk
          d.savedText = disk
          d.diskChanged = false
        } catch (e) {
          ui.toast(errorMessage(e), 'error')
        }
        return false
      }
      if (choice !== 'mine') return false
    }
    const text = d.text
    try {
      try {
        await backend.writeFile(d.path, text)
      } catch (e) {
        // the file is gone (moved/deleted outside goo): recreate it from the editor
        if (!(isBackendError(e) && e.code === 'not_found' && d.diskChanged && this.workspace)) throw e
        const rel = relativeTo(this.workspace.root, d.path)
        const dir = d.path.slice(0, d.path.length - basename(d.path).length - 1) || this.workspace.root
        await backend.createFile(dir, basename(d.path), text).catch(() => {
          throw new Error(`${rel} could not be recreated`)
        })
      }
      d.savedText = text
      d.diskChanged = false
      this.onSaved?.()
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

  /** Closes the tabs in `paths`, asking once about the ones with unsaved edits. False when cancelled. */
  async closeTabs(paths: string[]): Promise<boolean> {
    const closing = this.docs.filter((d) => paths.includes(d.path))
    const dirty = closing.filter(isDirty)
    if (dirty.length && (await this.confirmDiscard(dirty)) === 'cancel') return false
    const i = this.docs.findIndex((d) => d.path === this.activePath)
    const kept = this.docs.filter((d) => !closing.includes(d))
    if (!kept.some((d) => d.path === this.activePath)) {
      // like closing one tab: the next tab to the right that stays, else the nearest one to the left
      const next = this.docs.slice(i + 1).find((d) => kept.includes(d)) ?? this.docs.slice(0, Math.max(i, 0)).reverse().find((d) => kept.includes(d))
      this.activePath = next?.path ?? null
    }
    this.docs = kept
    this.persist()
    return true
  }

  /** Closes every tab but `path`, which becomes the active one. */
  async closeOthers(path: string) {
    if (await this.closeTabs(this.docs.filter((d) => d.path !== path).map((d) => d.path))) this.select(path)
  }

  closeAll() {
    return this.closeTabs(this.docs.map((d) => d.path))
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

  // -------------------------------------------------------------------------
  // File management
  // -------------------------------------------------------------------------

  /**
   * A note or folder moved from `from` to `to`: open tabs inside it follow (unsaved edits included), and so do the
   * active tab and the collapsed folders.
   */
  retarget(from: string, to: string) {
    const sep = sepOf(from)
    const moved = (p: string) => (p === from ? to : p.startsWith(from + sep) ? to + p.slice(from.length) : null)
    for (const d of this.docs) {
      const next = moved(d.path)
      if (next) {
        d.path = next
        d.name = basename(next)
      }
    }
    const active = this.activePath && moved(this.activePath)
    if (active) this.activePath = active
    this.collapsed = new Set([...this.collapsed].map((c) => moved(c) ?? c))
    this.persist()
  }

  /** Paths of the open notes inside `path` (the note itself, or everything under a folder). */
  private docsUnder(path: string): Doc[] {
    const sep = sepOf(path)
    return this.docs.filter((d) => d.path === path || d.path.startsWith(path + sep))
  }

  async renameEntry(node: TreeNode) {
    const isNote = node.kind === 'file'
    const name = await ui.prompt({
      title: isNote ? 'Rename note' : 'Rename folder',
      label: `New name for ${node.name}`,
      value: isNote ? stem(node.name) : node.name,
      confirmLabel: 'Rename',
    })
    if (name === null || name === (isNote ? stem(node.name) : node.name)) return
    try {
      const to = await backend.renameEntry(node.path, name)
      this.retarget(node.path, to)
      await this.refreshTree()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  async moveEntry(path: string, destDir: string) {
    if (dirname(path) === destDir) return
    try {
      const to = await backend.moveEntry(path, destDir)
      this.retarget(path, to)
      const open = new Set(this.collapsed)
      open.delete(destDir)
      this.collapsed = open
      await this.refreshTree()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  async newFolder(dir?: string) {
    const ws = this.workspace
    if (!ws) return
    const target = dir ?? ws.root
    const name = await ui.prompt({ title: 'New folder', label: `In ${relativeTo(ws.root, target) === target ? ws.name : `${ws.name}/${relativeTo(ws.root, target)}`}`, value: 'New folder', confirmLabel: 'Create' })
    if (name === null) return
    try {
      await backend.createFolder(target, name)
      const open = new Set(this.collapsed)
      open.delete(target)
      this.collapsed = open
      await this.refreshTree()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  /** Moves a note or folder to the system trash after asking. Open notes inside it are closed. */
  async deleteEntry(node: TreeNode) {
    const isNote = node.kind === 'file'
    const affected = this.docsUnder(node.path)
    const unsaved = affected.filter(isDirty)
    const count = isNote ? 1 : flattenFiles(node.children ?? []).length
    const lines = [
      isNote ? `${node.name} goes to the system trash, where you can restore it.` : `The folder ${node.name} and the ${count} note${count === 1 ? '' : 's'} in it go to the system trash, where you can restore them.`,
    ]
    if (unsaved.length) lines.push(`Unsaved edits in ${unsaved.map((d) => d.name).join(', ')} will be lost.`)
    const choice = await ui.confirm({
      title: isNote ? `Delete ${node.name}?` : `Delete the folder ${node.name}?`,
      message: lines.join(' '),
      actions: [
        { id: 'delete', label: 'Move to trash', kind: 'danger' },
        { id: 'cancel', label: 'Cancel' },
      ],
    })
    if (choice !== 'delete') return
    try {
      await backend.deleteEntry(node.path)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
      return
    }
    for (const d of affected) {
      d.text = d.savedText // already confirmed: close without asking again
      await this.close(d.path)
    }
    await this.refreshTree()
    ui.toast(`${node.name} moved to the trash.`)
  }

  async revealEntry(path: string) {
    try {
      await backend.revealEntry(path)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
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
