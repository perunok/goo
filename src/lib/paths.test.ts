import { basename, dirname, relativeTo, resolveLink, sepOf, stem } from './paths'

describe('paths', () => {
  it('splits POSIX and Windows paths', () => {
    expect(basename('/a/b/c.md')).toBe('c.md')
    expect(basename('C:\\a\\c.md')).toBe('c.md')
    expect(dirname('/a/b/c.md')).toBe('/a/b')
    expect(dirname('/c.md')).toBe('/')
    expect(dirname('C:\\a\\c.md')).toBe('C:\\a')
    expect(sepOf('C:\\a')).toBe('\\')
    expect(sepOf('/a')).toBe('/')
    expect(stem('/a/Read Me.markdown')).toBe('Read Me')
  })

  it('makes paths relative to the workspace', () => {
    expect(relativeTo('/w', '/w/a/b.md')).toBe('a/b.md')
    expect(relativeTo('/w/', '/w/b.md')).toBe('b.md')
    expect(relativeTo('C:\\w', 'C:\\w\\a\\b.md')).toBe('a/b.md')
    expect(relativeTo('/w', '/other/b.md')).toBe('/other/b.md')
  })

  it('resolves relative note links', () => {
    expect(resolveLink('/w/a/note.md', 'other.md')).toEqual({ path: '/w/a/other.md', fragment: null })
    expect(resolveLink('/w/a/note.md', '../b/x.md#Plans')).toEqual({ path: '/w/b/x.md', fragment: 'Plans' })
    expect(resolveLink('/w/a/note.md', './My%20Note.md')).toEqual({ path: '/w/a/My Note.md', fragment: null })
    expect(resolveLink('C:\\w\\a\\n.md', '../x.md')).toEqual({ path: 'C:\\w\\x.md', fragment: null })
  })

  it('ignores links that are not relative files', () => {
    for (const href of ['https://x.dev', 'mailto:a@b.c', '#top', '/etc/passwd', 'javascript:alert(1)', '']) {
      expect(resolveLink('/w/n.md', href)).toBeNull()
    }
  })
})
