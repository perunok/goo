import type { UpdateMethod } from '../../backend/types'
import { methodNote, percent, timeLeft } from './progress'

describe('update dialog numbers', () => {
  it('percent is capped and safe without a total', () => {
    expect(percent({ received: 25, total: 100 })).toBe(25)
    expect(percent({ received: 150, total: 100 })).toBe(100)
    expect(percent({ received: 5, total: null })).toBe(0)
    expect(percent({ received: 5, total: 0 })).toBe(0)
  })

  it('time left rounds up and says nothing when unknown', () => {
    expect(timeLeft({ received: 0, total: 1000, bytesPerSec: 100 })).toBe('10 s')
    expect(timeLeft({ received: 0, total: 100_000, bytesPerSec: 1000 })).toBe('2 min')
    expect(timeLeft({ received: 1000, total: 1000, bytesPerSec: 100 })).toBeNull()
    expect(timeLeft({ received: 0, total: 1000, bytesPerSec: null })).toBeNull()
    expect(timeLeft({ received: 0, total: null, bytesPerSec: 100 })).toBeNull()
  })

  it('explains every way goo can update itself', () => {
    const notes = (['deb', 'appimage', 'nsis', 'dmg'] as UpdateMethod[]).map(methodNote)
    for (const n of notes) expect(n).toContain("GitHub's checksum")
    expect(notes[0]).toContain('pkexec apt-get')
    expect(notes[3]).toContain('goo.app')
  })
})
