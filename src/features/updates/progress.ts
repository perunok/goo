/** Wording and numbers for the Update goo dialog (pure). */
import type { UpdateInstallState, UpdateMethod } from '../../backend/types'

export function percent(s: Pick<UpdateInstallState, 'received' | 'total'>): number {
  if (!s.total) return 0
  return Math.min(100, Math.floor((s.received / s.total) * 100))
}

/** "7 s" / "2 min" left at the current speed, or null when unknown. */
export function timeLeft(s: Pick<UpdateInstallState, 'received' | 'total' | 'bytesPerSec'>): string | null {
  if (!s.total || !s.bytesPerSec || s.bytesPerSec <= 0) return null
  const sec = Math.ceil((s.total - s.received) / s.bytesPerSec)
  if (sec <= 0) return null
  return sec < 60 ? `${sec} s` : `${Math.ceil(sec / 60)} min`
}

export function methodNote(method: UpdateMethod): string {
  switch (method) {
    case 'deb':
      return "goo downloads the new .deb from GitHub, checks it against GitHub's checksum, and installs it with your system's package manager (pkexec apt-get), which asks for your password. Then it starts again."
    case 'appimage':
      return "goo downloads the new AppImage from GitHub, checks it against GitHub's checksum, and replaces this AppImage file with it. Then it starts again."
    case 'nsis':
      return "goo downloads the new installer from GitHub, checks it against GitHub's checksum, and runs it in the background; Windows may ask for permission. goo starts again when it is done."
    case 'dmg':
      return "goo downloads the new disk image from GitHub, checks it against GitHub's checksum, and replaces goo.app with the new one after it quits. Then it starts again."
  }
}
