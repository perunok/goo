/**
 * Update notice: when to ask the backend for the latest release, and whether to tell the user about it.
 * Persisted as JSON in `localStorage['goo.updates']`: `{ auto, lastCheckedAt, latest, notified }`.
 */
import type { ReleaseInfo } from '../backend/types'
import { compareSemver, parseSemver } from './semver'

export const UPDATES_STORAGE_KEY = 'goo.updates'
/** Automatic checks: at most once a day. */
export const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000

export interface UpdatePrefs {
  /** Check for new releases automatically (default on). */
  auto: boolean
  /** Unix seconds of the last successful check. */
  lastCheckedAt: number | null
  /** The latest release seen by that check. */
  latest: ReleaseInfo | null
  /** The newest version the user was already notified about (never notified twice for one version). */
  notified: string | null
}

export const DEFAULT_UPDATE_PREFS: UpdatePrefs = { auto: true, lastCheckedAt: null, latest: null, notified: null }

const RELEASE_URL_PREFIX = 'https://github.com/perunok/goo/releases/'

const isStableVersion = (v: unknown): v is string => typeof v === 'string' && v.length <= 64 && parseSemver(v)?.prerelease === ''

function parseRelease(v: unknown): ReleaseInfo | null {
  if (!v || typeof v !== 'object') return null
  const o = v as Record<string, unknown>
  if (!isStableVersion(o.version) || typeof o.url !== 'string' || !o.url.startsWith(RELEASE_URL_PREFIX)) return null
  const publishedAt = typeof o.publishedAt === 'number' && Number.isFinite(o.publishedAt) ? o.publishedAt : null
  return { version: o.version, url: o.url, publishedAt }
}

/** Reads the stored preferences; anything missing or invalid falls back to the default field by field. */
export function loadUpdatePrefs(raw: string | null): UpdatePrefs {
  let o: Record<string, unknown> = {}
  try {
    const parsed: unknown = raw ? JSON.parse(raw) : null
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) o = parsed as Record<string, unknown>
  } catch {
    /* corrupt: defaults */
  }
  return {
    auto: typeof o.auto === 'boolean' ? o.auto : DEFAULT_UPDATE_PREFS.auto,
    lastCheckedAt: typeof o.lastCheckedAt === 'number' && Number.isFinite(o.lastCheckedAt) ? o.lastCheckedAt : null,
    latest: parseRelease(o.latest),
    notified: isStableVersion(o.notified) ? o.notified : null,
  }
}

/** An automatic check is due: on, and never checked, last checked a day or more ago, or the clock went backwards. */
export function isCheckDue(prefs: UpdatePrefs, nowMs: number, intervalMs = CHECK_INTERVAL_MS): boolean {
  if (!prefs.auto) return false
  if (prefs.lastCheckedAt === null) return true
  const elapsed = nowMs - prefs.lastCheckedAt * 1000
  return elapsed < 0 || elapsed >= intervalMs
}

/** `latest` is newer than the running version (an unparseable current version never offers anything). */
export function isNewer(latest: ReleaseInfo | null, currentVersion: string | null): latest is ReleaseInfo {
  if (!latest || !currentVersion || !parseSemver(currentVersion)) return false
  return compareSemver(latest.version, currentVersion) > 0
}

/** Notify about `latest` unless the user was already told about this (or a newer) version. */
export function shouldNotify(prefs: UpdatePrefs, latest: ReleaseInfo | null, currentVersion: string | null): latest is ReleaseInfo {
  if (!isNewer(latest, currentVersion)) return false
  return prefs.notified === null || compareSemver(latest.version, prefs.notified) > 0
}
