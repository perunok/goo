/** Just enough semantic versioning for update notices: "1.2.3" (a leading "v" is allowed), stable releases only. */
export interface Semver {
  major: number
  minor: number
  patch: number
  /** Anything after "-" (a pre-release); empty for a stable version. */
  prerelease: string
}

export function parseSemver(v: string): Semver | null {
  const m = /^v?(\d{1,9})\.(\d{1,9})\.(\d{1,9})(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(v.trim())
  return m ? { major: +m[1]!, minor: +m[2]!, patch: +m[3]!, prerelease: m[4] ?? '' } : null
}

/** Negative when a < b, 0 when equal, positive when a > b (a pre-release sorts before its release). */
export function compareSemver(a: string, b: string): number {
  const x = parseSemver(a)
  const y = parseSemver(b)
  if (!x || !y) return 0
  for (const k of ['major', 'minor', 'patch'] as const) if (x[k] !== y[k]) return x[k] - y[k]
  if (x.prerelease === y.prerelease) return 0
  if (!x.prerelease) return 1
  if (!y.prerelease) return -1
  return x.prerelease < y.prerelease ? -1 : 1
}
