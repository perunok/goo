// Outside changes to open notes (folder watching, AI assistants, goo cloud), against the in-memory backend.
import { backend } from './backend'
import { ui } from './ui.svelte'
import { workspace } from './workspace.svelte'

type Mock = typeof backend & { files: Map<string, string> }
const files = (backend as Mock).files
const ROOT = '/home/you/notes'
const NOTE = `${ROOT}/Welcome.md`
const outside = (path: string, text: string) => files.set(path, text)
const changed = (paths: string[]) =>
  (workspace as unknown as { handleFilesChanged(e: { root: string; files: string[] }): Promise<void> }).handleFilesChanged({ root: ROOT, files: paths })

/** Answers the next confirm dialog with `id`. */
async function answer(id: string) {
  for (let i = 0; i < 20 && !ui.confirmRequest; i++) await new Promise((r) => setTimeout(r, 5))
  ui.confirmRequest!.resolve(id)
}

describe('notes changed outside goo', () => {
  beforeEach(async () => {
    if (!workspace.workspace) await workspace.pickFolder()
    for (const d of [...workspace.docs]) {
      d.text = d.savedText
      await workspace.close(d.path)
    }
    outside(NOTE, '# Welcome\n')
    await workspace.openFile(NOTE)
  })

  const doc = () => workspace.docs.find((d) => d.path === NOTE)!

  it('reloads a note without unsaved edits', async () => {
    outside(NOTE, '# Welcome\nfrom outside\n')
    await changed([NOTE])
    expect(doc().text).toBe('# Welcome\nfrom outside\n')
    expect(doc().diskChanged).toBe(false)
  })

  it('keeps unsaved edits and marks the note', async () => {
    workspace.setText(NOTE, '# Welcome\nmine\n')
    outside(NOTE, '# Welcome\ntheirs\n')
    await changed([NOTE])
    expect(doc().text).toBe('# Welcome\nmine\n')
    expect(doc().diskChanged).toBe(true)
  })

  it('ignores the echo of its own save', async () => {
    workspace.setText(NOTE, '# Welcome\nsaved\n')
    await workspace.save(NOTE)
    workspace.setText(NOTE, '# Welcome\nsaved\nstill typing')
    await changed([NOTE]) // the watcher reports goo's own write
    expect(doc().diskChanged).toBe(false)
    expect(doc().text).toBe('# Welcome\nsaved\nstill typing')
  })

  it('asks before overwriting: keep mine', async () => {
    workspace.setText(NOTE, 'mine')
    outside(NOTE, 'theirs')
    await changed([NOTE])
    const saving = workspace.save(NOTE)
    await answer('mine')
    expect(await saving).toBe(true)
    expect(files.get(NOTE)).toBe('mine')
    expect(doc().diskChanged).toBe(false)
  })

  it('asks before overwriting: load theirs', async () => {
    workspace.setText(NOTE, 'mine')
    outside(NOTE, 'theirs')
    await changed([NOTE])
    const saving = workspace.save(NOTE)
    await answer('theirs')
    expect(await saving).toBe(false)
    expect(doc().text).toBe('theirs')
    expect(files.get(NOTE)).toBe('theirs')
  })

  it('closes a clean note that was deleted outside goo, keeps a dirty one', async () => {
    files.delete(NOTE)
    await changed([NOTE])
    expect(workspace.docs.some((d) => d.path === NOTE)).toBe(false)

    outside(NOTE, '# Welcome\n')
    await workspace.openFile(NOTE)
    workspace.setText(NOTE, 'unsaved')
    files.delete(NOTE)
    await changed([NOTE])
    expect(doc().text).toBe('unsaved')
    expect(doc().diskChanged).toBe(true)
  })
})

describe('file management', () => {
  beforeEach(async () => {
    if (!workspace.workspace) await workspace.pickFolder()
    for (const d of [...workspace.docs]) {
      d.text = d.savedText
      await workspace.close(d.path)
    }
  })

  const answerPrompt = async (value: string | null) => {
    for (let i = 0; i < 20 && !ui.promptRequest; i++) await new Promise((r) => setTimeout(r, 5))
    ui.promptRequest!.resolve(value)
  }
  const node = (path: string, kind: 'file' | 'dir' = 'file') => ({ name: path.split('/').pop()!, path, kind, children: kind === 'dir' ? [] : undefined })

  it('renaming an open note keeps the tab and its unsaved edits', async () => {
    outside(`${ROOT}/draft.md`, '# Draft\n')
    await workspace.openFile(`${ROOT}/draft.md`)
    workspace.setText(`${ROOT}/draft.md`, '# Draft\nunsaved')
    const renaming = workspace.renameEntry(node(`${ROOT}/draft.md`))
    await answerPrompt('final')
    await renaming
    const d = workspace.docs.find((x) => x.name === 'final.md')!
    expect(d.path).toBe(`${ROOT}/final.md`)
    expect(d.text).toBe('# Draft\nunsaved')
    expect(workspace.activePath).toBe(`${ROOT}/final.md`)
    expect(files.has(`${ROOT}/final.md`)).toBe(true)
  })

  it('moving a folder moves the open notes inside it', async () => {
    await workspace.openFile(`${ROOT}/projects/goo/ideas.md`)
    await workspace.moveEntry(`${ROOT}/projects/goo`, `${ROOT}/journal`)
    expect(workspace.docs[0]!.path).toBe(`${ROOT}/journal/goo/ideas.md`)
    expect(files.has(`${ROOT}/journal/goo/ideas.md`)).toBe(true)
  })

  it('new folders show in the tree even while empty', async () => {
    const making = workspace.newFolder()
    await answerPrompt('Archive')
    await making
    expect(workspace.tree!.nodes.some((n) => n.kind === 'dir' && n.name === 'Archive')).toBe(true)
  })

  it('deleting asks, mentions unsaved edits, and closes the notes inside', async () => {
    outside(`${ROOT}/gone/a.md`, 'a')
    await workspace.refreshTree()
    await workspace.openFile(`${ROOT}/gone/a.md`)
    workspace.setText(`${ROOT}/gone/a.md`, 'edited')
    const deleting = workspace.deleteEntry({ ...node(`${ROOT}/gone`, 'dir'), children: [node(`${ROOT}/gone/a.md`)] })
    for (let i = 0; i < 20 && !ui.confirmRequest; i++) await new Promise((r) => setTimeout(r, 5))
    expect(ui.confirmRequest!.message).toContain('Unsaved edits in a.md will be lost')
    ui.confirmRequest!.resolve('delete')
    await deleting
    expect(files.has(`${ROOT}/gone/a.md`)).toBe(false)
    expect(workspace.docs.some((d) => d.path.startsWith(`${ROOT}/gone`))).toBe(false)
  })

  it('cancelling a delete keeps everything', async () => {
    outside(`${ROOT}/keep.md`, 'k')
    const deleting = workspace.deleteEntry(node(`${ROOT}/keep.md`))
    for (let i = 0; i < 20 && !ui.confirmRequest; i++) await new Promise((r) => setTimeout(r, 5))
    ui.confirmRequest!.resolve('cancel')
    await deleting
    expect(files.has(`${ROOT}/keep.md`)).toBe(true)
  })
})

describe('tabs', () => {
  const NOTES = ['a.md', 'b.md', 'c.md'].map((n) => `${ROOT}/tabs/${n}`)
  const open = () => workspace.docs.map((d) => d.path)

  beforeEach(async () => {
    if (!workspace.workspace) await workspace.pickFolder()
    for (const d of [...workspace.docs]) {
      d.text = d.savedText
      await workspace.close(d.path)
    }
    for (const p of NOTES) {
      outside(p, '# note\n')
      await workspace.openFile(p)
    }
  })

  it('close other tabs keeps the chosen tab and makes it active', async () => {
    workspace.select(NOTES[2]!)
    await workspace.closeOthers(NOTES[1]!)
    expect(open()).toEqual([NOTES[1]])
    expect(workspace.activePath).toBe(NOTES[1])
  })

  it('asks once about every unsaved tab; cancel keeps them all', async () => {
    workspace.setText(NOTES[0]!, 'a')
    workspace.setText(NOTES[2]!, 'b')
    const closing = workspace.closeAll()
    for (let i = 0; i < 20 && !ui.confirmRequest; i++) await new Promise((r) => setTimeout(r, 5))
    expect(ui.confirmRequest!.title).toBe('2 notes have unsaved changes')
    ui.confirmRequest!.resolve('cancel')
    expect(await closing).toBe(false)
    expect(open()).toEqual(NOTES)
  })

  it("close all tabs after Don't save", async () => {
    workspace.setText(NOTES[0]!, 'a')
    const closing = workspace.closeAll()
    await answer('discard')
    expect(await closing).toBe(true)
    expect(open()).toEqual([])
    expect(workspace.activePath).toBe(null)
  })

  it('Open note opens the picked note in a tab', async () => {
    const picked = `${ROOT}/tabs/picked.md`
    outside(picked, '# picked\n') // the newest note is what the mock's picker chooses
    await workspace.pickFile()
    expect(open()).toContain(picked)
    expect(workspace.activePath).toBe(picked)
  })
})
