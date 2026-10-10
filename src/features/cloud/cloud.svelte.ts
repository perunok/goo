/** goo cloud status for the UI: kept current from `cloud-status` events (src-tauri/src/cloud.rs). */
import { backend } from '../../app/backend'
import { workspace } from '../../app/workspace.svelte'
import type { CloudLink, CloudStatus } from '../../backend/types'

class CloudStore {
  status = $state<CloudStatus>({ account: null, signIn: null, signInError: null, links: [] })

  /** The link of the folder open in the window, if it is synced. */
  current = $derived<CloudLink | null>(this.status.links.find((l) => l.root === workspace.workspace?.root) ?? null)

  async init() {
    backend.onCloudStatus((s) => (this.status = s))
    workspace.onSaved = () => this.saved()
    try {
      await backend.cloudSetUtcOffset(-new Date().getTimezoneOffset())
      this.status = await backend.cloudStatus()
    } catch {
      /* the cloud is optional; the settings page shows errors when used */
    }
  }

  set(s: CloudStatus) {
    this.status = s
  }

  /** After a save: sync that folder soon (if it is linked). */
  saved() {
    const link = this.current
    if (link) void backend.cloudSyncNow(link.root).catch(() => {})
  }
}

export const cloud = new CloudStore()

/** "just now", "5 min ago", "2 h ago", or a date. */
export function ago(unixSeconds: number | null, now = Date.now()): string {
  if (!unixSeconds) return 'not yet'
  const s = Math.max(0, Math.round(now / 1000 - unixSeconds))
  if (s < 45) return 'just now'
  if (s < 3600) return `${Math.round(s / 60)} min ago`
  if (s < 86400) return `${Math.round(s / 3600)} h ago`
  return new Date(unixSeconds * 1000).toLocaleDateString()
}
