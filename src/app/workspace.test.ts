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
