import { lineAt } from '../components/editor/selection'

describe('lineAt', () => {
  it('counts lines from 1', () => {
    expect(lineAt('a\nb\nc', 0)).toBe(1)
    expect(lineAt('a\nb\nc', 2)).toBe(2)
    expect(lineAt('a\nb\nc', 99)).toBe(3)
  })
})
