import { canDrop } from './dnd'

describe('canDrop', () => {
  it('allows moving into other folders only', () => {
    expect(canDrop('/w/a.md', '/w/sub')).toBe(true)
    expect(canDrop('/w/sub/a.md', '/w')).toBe(true)
    expect(canDrop('/w/a.md', '/w')).toBe(false) // already there
    expect(canDrop('/w/dir', '/w/dir')).toBe(false) // onto itself
    expect(canDrop('/w/dir', '/w/dir/inner')).toBe(false) // into itself
    expect(canDrop('/w/dir', '/w/dir2')).toBe(true) // a sibling with a similar name
    expect(canDrop('', '/w')).toBe(false)
  })
})
