import type { ReleaseInfo } from '../backend/types'
import { compareSemver, parseSemver } from './semver'
import { isCheckDue, isNewer, loadUpdatePrefs, shouldNotify, CHECK_INTERVAL_MS, DEFAULT_UPDATE_PREFS, type UpdatePrefs } from './updates'

const rel = (version: string): ReleaseInfo => ({ version, url: `https://github.com/perunok/goo/releases/tag/v${version}`, publishedAt: null })

describe('semver', () => {
  it('reads plain, v-prefixed and pre-release versions, and refuses the rest', () => {
    expect(parseSemver('1.2.3')).toEqual({ major: 1, minor: 2, patch: 3, prerelease: '' })
    expect(parseSemver('v0.10.0')?.minor).toBe(10)
    expect(parseSemver('1.0.0-rc.1')?.prerelease).toBe('rc.1')
    for (const bad of ['', '1.2', '1.2.3.4', 'latest', '1.2.x', 'v', '1.2.3-']) expect(parseSemver(bad), bad).toBeNull()
  })

  it('compares numerically, with pre-releases before their release', () => {
    expect(compareSemver('0.10.0', '0.9.0')).toBeGreaterThan(0)
    expect(compareSemver('1.0.0', '0.99.99')).toBeGreaterThan(0)
    expect(compareSemver('0.4.0', '0.4.0')).toBe(0)
    expect(compareSemver('0.3.9', '0.4.0')).toBeLessThan(0)
    expect(compareSemver('1.0.0-rc.1', '1.0.0')).toBeLessThan(0)
    expect(compareSemver('garbage', '1.0.0')).toBe(0)
  })
})

describe('update preferences', () => {
  it('start from the defaults and survive corruption field by field', () => {
    expect(loadUpdatePrefs(null)).toEqual(DEFAULT_UPDATE_PREFS)
    expect(loadUpdatePrefs('{nope')).toEqual(DEFAULT_UPDATE_PREFS)
    expect(loadUpdatePrefs('[1,2]')).toEqual(DEFAULT_UPDATE_PREFS)
    const p = loadUpdatePrefs(JSON.stringify({ auto: false, lastCheckedAt: 'yesterday', latest: { version: '0.5.0', url: 'https://evil.example/x' }, notified: '0.5.0-rc.1' }))
    expect(p).toEqual({ auto: false, lastCheckedAt: null, latest: null, notified: null }) // only `auto` was valid
  })

  it('round-trips a stored release and only trusts goo release links', () => {
    const stored: UpdatePrefs = { auto: true, lastCheckedAt: 1_790_000_000, latest: rel('0.5.0'), notified: '0.5.0' }
    expect(loadUpdatePrefs(JSON.stringify(stored))).toEqual(stored)
  })

  it('checks automatically at most once a day, and never when turned off', () => {
    const day = CHECK_INTERVAL_MS
    const at = (lastCheckedAt: number | null, auto = true) => ({ ...DEFAULT_UPDATE_PREFS, auto, lastCheckedAt })
    const now = 1_790_000_000_000
    expect(isCheckDue(at(null), now)).toBe(true)
    expect(isCheckDue(at(now / 1000 - 3600), now)).toBe(false)
    expect(isCheckDue(at(now / 1000 - day / 1000), now)).toBe(true)
    expect(isCheckDue(at(now / 1000 + 100), now)).toBe(true) // the clock went backwards
    expect(isCheckDue(at(null, false), now)).toBe(false)
  })

  it('offers only a newer stable release over a parseable running version', () => {
    expect(isNewer(rel('0.5.0'), '0.4.0')).toBe(true)
    expect(isNewer(rel('0.4.0'), '0.4.0')).toBe(false)
    expect(isNewer(rel('0.5.0'), 'dev')).toBe(false)
    expect(isNewer(null, '0.4.0')).toBe(false)
    expect(isNewer(rel('0.5.0'), null)).toBe(false)
  })

  it('tells the user about a version once', () => {
    const prefs = (notified: string | null): UpdatePrefs => ({ ...DEFAULT_UPDATE_PREFS, notified })
    expect(shouldNotify(prefs(null), rel('0.5.0'), '0.4.0')).toBe(true)
    expect(shouldNotify(prefs('0.5.0'), rel('0.5.0'), '0.4.0')).toBe(false)
    expect(shouldNotify(prefs('0.5.0'), rel('0.5.1'), '0.4.0')).toBe(true)
    expect(shouldNotify(prefs(null), rel('0.4.0'), '0.4.0')).toBe(false)
  })
})
