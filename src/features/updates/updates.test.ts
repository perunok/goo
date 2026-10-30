// The update notice and in-app updater, against the in-memory backend's pretend updater (src/backend/mock.ts).
import { afterAll, beforeAll, beforeEach, vi } from 'vitest'
import { backend } from '../../app/backend'
import { ui } from '../../app/ui.svelte'
import { workspace } from '../../app/workspace.svelte'
import type { MockUpdates } from '../../backend/mock'
import { UPDATES_STORAGE_KEY } from '../../lib/updates'
import { updates } from './updates.svelte'

const mock = (backend as unknown as { updates: MockUpdates; files: Map<string, string> }).updates
const files = (backend as unknown as { files: Map<string, string> }).files
const CURRENT = __APP_VERSION__
const newer = (n = 1) => `0.${Number(CURRENT.split('.')[1]) + n}.0`
let stop: () => void

beforeAll(() => {
  stop = updates.start() // subscribes to update-state (its timers are irrelevant here)
})
afterAll(() => stop())

beforeEach(async () => {
  localStorage.clear()
  updates.resetForTests()
  ui.toasts = []
  ui.updateOpen = false
  ui.confirmRequest = null
  mock.reset()
  Object.assign(mock, { latest: newer(), tickMs: 5 })
  updates.install = await backend.updateInstallState() // the app loads this at startup
  // earlier tests may leave notes with unsaved text: forget it, so closing them does not ask
  for (const d of [...workspace.docs]) {
    d.text = d.savedText
    await workspace.close(d.path)
  }
})

const toast = () => ui.toasts.at(-1)
async function untilPhase(phase: string) {
  await vi.waitFor(() => expect(updates.install?.phase).toBe(phase), { timeout: 5000 })
}

describe('checking', () => {
  it('a manual check that finds nothing says so', async () => {
    mock.latest = CURRENT
    await updates.check('manual')
    expect(updates.available).toBeNull()
    expect(updates.lastCheckedAt).not.toBeNull()
    expect(toast()?.text).toContain('up to date')
    expect(JSON.parse(localStorage.getItem(UPDATES_STORAGE_KEY)!).lastCheckedAt).toBe(updates.lastCheckedAt)
  })

  it('an automatic check tells about a new version once, with an Update now button that stays', async () => {
    await updates.check('auto')
    expect(updates.available?.version).toBe(newer())
    expect(ui.toasts).toHaveLength(1)
    expect(toast()?.text).toBe(`goo ${newer()} is available`)
    expect(toast()?.action?.label).toBe('Update now')
    await updates.check('auto') // the same version again: no second notice
    expect(ui.toasts).toHaveLength(1)
    mock.latest = newer(2)
    await updates.check('auto') // a newer one: told again
    expect(ui.toasts).toHaveLength(2)
    expect(JSON.parse(localStorage.getItem(UPDATES_STORAGE_KEY)!).notified).toBe(newer(2))
  })

  it('an automatic check with nothing new is silent', async () => {
    mock.latest = CURRENT
    await updates.check('auto')
    expect(ui.toasts).toHaveLength(0)
  })

  it('Settings shows the answer itself instead of a notice, a manual check always answers', async () => {
    await updates.check('settings')
    expect(ui.toasts).toHaveLength(0)
    expect(updates.available?.version).toBe(newer())
    await updates.check('manual')
    expect(ui.toasts).toHaveLength(1)
  })

  it('a failed check keeps its reason until the next success; only a manual one complains', async () => {
    mock.failCheck = 'Could not reach GitHub to check for updates'
    await updates.check('auto')
    expect(updates.error).toContain('Could not reach GitHub')
    expect(ui.toasts).toHaveLength(0)
    await updates.check('manual')
    expect(toast()?.kind).toBe('error')
    mock.failCheck = null
    await updates.check('settings')
    expect(updates.error).toBeNull()
  })

  it('an installation that cannot update itself gets View release', async () => {
    mock.method = null
    updates.install = await backend.updateInstallState()
    const open = vi.spyOn(backend, 'openExternal').mockResolvedValue()
    await updates.check('auto')
    expect(toast()?.action?.label).toBe('View release')
    toast()!.action!.run()
    expect(open).toHaveBeenCalledWith(`https://github.com/perunok/goo/releases/tag/v${newer()}`)
    open.mockRestore()
    await expect(backend.downloadUpdate()).rejects.toMatchObject({ message: expect.stringContaining('development build') })
  })
})

describe('updating', () => {
  it('Update now opens the dialog, downloads, verifies and is ready to restart', async () => {
    await updates.check('settings')
    await updates.updateNow()
    expect(ui.updateOpen).toBe(true)
    expect(updates.install?.phase).toBe('downloading')
    await untilPhase('ready')
    expect(updates.install).toMatchObject({ version: newer(), received: 78_900_000, total: 78_900_000, error: null })
  })

  it('a download can be cancelled and started again', async () => {
    mock.tickMs = 30
    await updates.updateNow()
    await vi.waitFor(() => expect(updates.install!.received).toBeGreaterThan(0))
    await updates.cancelDownload()
    expect(updates.install).toMatchObject({ phase: 'idle', received: 0 })
    mock.tickMs = 5
    await updates.updateNow()
    await untilPhase('ready')
  })

  it('does not start a second download while one runs or is ready', async () => {
    mock.tickMs = 30
    await updates.updateNow()
    const first = updates.install
    await updates.updateNow()
    expect(updates.install?.phase).toBe('downloading')
    expect(first?.version).toBe(updates.install?.version)
    await untilPhase('ready')
    await updates.updateNow()
    expect(updates.install?.phase).toBe('ready')
  })

  it('a refused download shows why instead of failing silently', async () => {
    mock.method = null
    updates.install = null // not known yet: the refusal comes as a toast
    await updates.updateNow()
    expect(toast()?.kind).toBe('error')
    updates.install = await backend.updateInstallState()
    ui.toasts = []
    await updates.updateNow()
    expect(updates.install?.phase).toBe('error')
    expect(updates.install?.error).toContain('development build')
  })

  async function readyWithUnsavedNote() {
    await workspace.pickFolder()
    const note = '/home/you/notes/Welcome.md'
    files.set(note, '# Welcome\n')
    await workspace.openFile(note)
    await updates.updateNow()
    await untilPhase('ready')
    return note
  }

  it('asks to save unsaved notes first, and "Not now" changes nothing', async () => {
    const note = await readyWithUnsavedNote()
    workspace.setText(note, '# Welcome\nmy unsaved words')
    const restarting = updates.restartToUpdate()
    await vi.waitFor(() => expect(ui.confirmRequest).not.toBeNull())
    expect(ui.confirmRequest!.title).toBe('Save before updating?')
    expect(ui.confirmRequest!.message).toContain('1 note has unsaved changes')
    ui.confirmRequest!.resolve('cancel')
    await restarting
    expect(updates.install?.phase).toBe('ready') // nothing installed, nothing saved
    expect(files.get(note)).toBe('# Welcome\n')
    expect(workspace.docs.find((d) => d.path === note)?.text).toContain('my unsaved words')
  })

  it('saves unsaved notes, then installs', async () => {
    const note = await readyWithUnsavedNote()
    workspace.setText(note, '# Welcome\nsaved before the restart')
    const restarting = updates.restartToUpdate()
    await vi.waitFor(() => expect(ui.confirmRequest).not.toBeNull())
    ui.confirmRequest!.resolve('save')
    await restarting
    expect(files.get(note)).toBe('# Welcome\nsaved before the restart')
    expect(updates.install?.phase).toBe('installing')
    await untilPhase('error') // the browser demo cannot restart: it says so
    expect(updates.install?.error).toContain('browser demo')
  })

  it('installs at once when nothing is unsaved', async () => {
    await readyWithUnsavedNote()
    await updates.restartToUpdate()
    expect(ui.confirmRequest).toBeNull()
    expect(updates.install?.phase).toBe('installing')
    await untilPhase('error')
  })
})

describe('the schedule', () => {
  afterEach(() => vi.useRealTimers())

  it('checks 8 seconds after startup when one is due, and then not again within the day', async () => {
    vi.useFakeTimers()
    const check = vi.spyOn(backend, 'checkForUpdates')
    const stopFake = updates.start()
    expect(check).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(8000)
    expect(check).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(3 * 60 * 60 * 1000) // three hourly ticks: not due yet
    expect(check).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(22 * 60 * 60 * 1000) // a day later: due
    expect(check).toHaveBeenCalledTimes(2)
    stopFake()
    check.mockRestore()
  })

  it('never checks on its own when turned off', async () => {
    vi.useFakeTimers()
    updates.setAuto(false)
    const check = vi.spyOn(backend, 'checkForUpdates')
    const stopFake = updates.start()
    await vi.advanceTimersByTimeAsync(48 * 60 * 60 * 1000)
    expect(check).not.toHaveBeenCalled()
    stopFake()
    check.mockRestore()
  })
})
