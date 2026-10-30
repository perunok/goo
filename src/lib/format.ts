/** "532 B", "1.4 KB", "78.9 MB". */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '0 B'
  if (bytes < 1024) return `${Math.round(bytes)} B`
  const units = ['KB', 'MB', 'GB']
  let value = bytes / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[i]}`
}

/** "just now", "5 min ago", "2 h ago", or a date. */
export function ago(unixSeconds: number | null, now = Date.now()): string {
  if (!unixSeconds) return 'not yet'
  const s = Math.max(0, Math.round(now / 1000 - unixSeconds))
  if (s < 45) return 'just now'
  if (s < 3600) return `${Math.round(s / 60)} min ago`
  if (s < 86400) return `${Math.round(s / 3600)} h ago`
  return new Date(unixSeconds * 1000).toLocaleDateString()
}
