/**
 * New-release notice. The backend asks GitHub (checkForUpdates); this store decides when (once a day while "Check for
 * new releases automatically" is on, plus "Check for updates" in About and Settings > Updates) and shows one
 * notification per new version. Its "Update now" opens the Update goo dialog: the backend downloads the installer
 * (progress pushed as `update-state`), verifies it, and installs it on "Restart to update" (crates/goo-update).
 * Installations that cannot update themselves (development builds, .rpm, .msi, ...) get "View release" instead.
 */
import { backend } from '../../app/backend'
import { ui } from '../../app/ui.svelte'
import { workspace } from '../../app/workspace.svelte'
import { errorMessage, type ReleaseInfo, type UpdateCheckResult, type UpdateInstallState } from '../../backend/types'
import { isCheckDue, isNewer, loadUpdatePrefs, shouldNotify, UPDATES_STORAGE_KEY, type UpdatePrefs } from '../../lib/updates'

/** First automatic check this long after startup (startup work goes first), then re-checked hourly whether one is due. */
const STARTUP_DELAY_MS = 8000
const POLL_MS = 60 * 60 * 1000

/** `auto`: silent unless a new version; `manual`: always answers with a notification; `settings`: answers inline. */
export type CheckSource = 'auto' | 'manual' | 'settings'

function readPrefs(): UpdatePrefs {
  try {
    return loadUpdatePrefs(localStorage.getItem(UPDATES_STORAGE_KEY))
  } catch {
    return loadUpdatePrefs(null)
  }
}

class UpdatesStore {
  #prefs = readPrefs()
  auto = $state(this.#prefs.auto)
  latest = $state<ReleaseInfo | null>(this.#prefs.latest)
  lastCheckedAt = $state<number | null>(this.#prefs.lastCheckedAt)
  currentVersion = $state<string>(__APP_VERSION__)
  checking = $state(false)
  /** The last check's failure (cleared by the next success). */
  error = $state<string | null>(null)
  /** The in-app updater's state (null until known). */
  install = $state<UpdateInstallState | null>(null)

  /** This installation can download and install updates itself. */
  get canInstall(): boolean {
    return !!this.install?.method
  }

  /** The newer release, if the last check found one. */
  get available(): ReleaseInfo | null {
    return isNewer(this.latest, this.currentVersion) ? this.latest : null
  }

  /** Starts the automatic schedule; returns the stop function. */
  start(): () => void {
    let stopped = false
    const tick = () => {
      if (!stopped && isCheckDue(this.#prefs, Date.now())) void this.check('auto')
    }
    const first = setTimeout(tick, STARTUP_DELAY_MS)
    const poll = setInterval(tick, POLL_MS)
    backend.onUpdateState((s) => (this.install = s))
    backend.updateInstallState().then(
      (s) => (this.install ??= s),
      () => {},
    )
    return () => {
      stopped = true
      clearTimeout(first)
      clearInterval(poll)
    }
  }

  async check(source: CheckSource): Promise<UpdateCheckResult | null> {
    if (this.checking) return null
    this.checking = true
    try {
      // The notice offers "Update now" only on installations that can update themselves: know that first.
      this.install ??= await backend.updateInstallState().catch(() => null)
      const result = await backend.checkForUpdates()
      this.currentVersion = result.currentVersion
      this.latest = result.latest
      this.lastCheckedAt = result.checkedAt
      this.error = null
      const newer = isNewer(result.latest, result.currentVersion)
      const notify = source === 'manual' ? newer : shouldNotify(this.#prefs, result.latest, result.currentVersion)
      this.#save({ lastCheckedAt: result.checkedAt, latest: result.latest, notified: newer ? result.latest!.version : this.#prefs.notified })
      // Settings shows the answer itself; the notification is for the automatic check and the manual one.
      if (notify && source !== 'settings') this.#notify(result.latest!, result.currentVersion)
      else if (source === 'manual') ui.toast(`goo is up to date: ${result.currentVersion} is the latest release.`)
      return result
    } catch (e) {
      this.error = errorMessage(e)
      if (source === 'manual') ui.toast(`Could not check for updates: ${this.error}`, 'error')
      return null
    } finally {
      this.checking = false
    }
  }

  setAuto(v: boolean) {
    this.auto = v
    this.#save({ auto: v })
  }

  /** Opens the Update goo dialog without starting anything. */
  openDialog() {
    ui.updateOpen = true
  }

  /** Opens the dialog and starts downloading (unless a download is already going or done). */
  async updateNow(): Promise<void> {
    ui.updateOpen = true
    const phase = this.install?.phase
    if (phase === 'downloading' || phase === 'verifying' || phase === 'ready' || phase === 'installing') return
    try {
      this.install = await backend.downloadUpdate()
    } catch (e) {
      if (this.install) this.install = { ...this.install, phase: 'error', error: errorMessage(e) }
      else ui.toast(`Could not download the update: ${errorMessage(e)}`, 'error')
    }
  }

  async cancelDownload(): Promise<void> {
    try {
      this.install = await backend.cancelUpdateDownload()
    } catch (e) {
      ui.toast(`Could not cancel the download: ${errorMessage(e)}`, 'error')
    }
  }

  /**
   * Installs the downloaded update; goo quits and starts again. goo reopens the same folder and tabs but not unsaved
   * text, so notes with unsaved changes are saved first (or the update waits). A failure stays in `install.error`.
   */
  async restartToUpdate(): Promise<void> {
    const dirty = workspace.dirtyCount
    if (dirty > 0) {
      const choice = await ui.confirm({
        title: 'Save before updating?',
        message: `${dirty === 1 ? '1 note has' : `${dirty} notes have`} unsaved changes. goo saves ${dirty === 1 ? 'it' : 'them'}, installs the update and starts again.`,
        actions: [
          { id: 'save', label: 'Save and restart', kind: 'primary' },
          { id: 'cancel', label: 'Not now' },
        ],
      })
      if (choice !== 'save') return
      if (!(await workspace.saveAll())) return // the failed save said why; nothing is lost
    }
    try {
      this.install = await backend.installUpdate()
    } catch (e) {
      if (this.install) this.install = { ...this.install, phase: 'error', error: errorMessage(e) }
    }
  }

  openRelease(release: ReleaseInfo | null = this.available) {
    if (!release) return
    backend.openExternal(release.url).catch((e) => ui.toast(`Could not open the release page: ${errorMessage(e)}`, 'error'))
  }

  /** Test hook: forget this session and re-read localStorage. */
  resetForTests() {
    this.#prefs = readPrefs()
    this.auto = this.#prefs.auto
    this.latest = this.#prefs.latest
    this.lastCheckedAt = this.#prefs.lastCheckedAt
    this.currentVersion = __APP_VERSION__
    this.checking = false
    this.error = null
    this.install = null
  }

  #notify(release: ReleaseInfo, currentVersion: string) {
    // Stays until dismissed: it is shown at most once per version.
    ui.toast(`goo ${release.version} is available`, 'info', 0, {
      detail: `You have ${currentVersion}.`,
      action: this.canInstall ? { label: 'Update now', run: () => void this.updateNow() } : { label: 'View release', run: () => this.openRelease(release) },
    })
  }

  #save(patch: Partial<UpdatePrefs>) {
    this.#prefs = { ...this.#prefs, ...patch }
    try {
      localStorage.setItem(UPDATES_STORAGE_KEY, JSON.stringify(this.#prefs))
    } catch {
      /* storage unavailable: this session only */
    }
  }
}

export const updates = new UpdatesStore()
