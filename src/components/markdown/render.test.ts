import { fenceLanguage, renderMarkdown, slugify, splitFrontMatter } from './render'
import { highlightToHtml } from './highlight'

const html = (src: string) => renderMarkdown(src).html

describe('renderMarkdown', () => {
  it('renders GFM: tables, task lists, strikethrough', () => {
    const out = html('| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n- [ ] todo\n\n~~old~~')
    expect(out).toContain('<table>')
    expect(out).toMatch(/<input checked="" disabled="" type="checkbox">/)
    expect(out).toContain('<del>old</del>')
  })

  it('gives headings unique anchors and reports them', () => {
    const r = renderMarkdown('# Intro\n\n## Intro\n\n### Café & co')
    expect(r.headings.map((h) => h.slug)).toEqual(['intro', 'intro-1', 'café--co'])
    expect(r.html).toContain('data-anchor="intro-1"')
  })

  it('turns mermaid fences into placeholders with the escaped source', () => {
    const out = html('```mermaid\nflowchart LR\n  A --> B<script>\n```')
    expect(out).toContain('<pre class="md-mermaid"><code>flowchart LR\n  A --&gt; B&lt;script&gt;</code></pre>')
  })

  it('highlights known languages and labels the block', () => {
    const out = html('```ts\nconst x = 1\n```')
    expect(out).toContain('class="md-code-lang"')
    expect(out).toContain('<span class="tok-keyword">const</span>')
  })

  it('strips scripts, handlers, styles, iframes and javascript: links', () => {
    const out = html(
      '<script>alert(1)</script><img src=x onerror=alert(1)><iframe src="https://x"></iframe>' +
        '<p style="color:red" onclick="x()">hi</p>\n\n[bad](javascript:alert(1)) <a href="data:text/html,x">d</a>',
    )
    expect(out).not.toMatch(/<script|onerror|onclick|<iframe|style=|javascript:|data:text/)
    expect(out).toContain('<p>hi</p>')
  })

  it('drops foreign classes and ids (no DOM clobbering)', () => {
    const out = html('<div class="app evil md-ok" id="app" name="x">t</div>')
    expect(out).toContain('class="md-ok"')
    expect(out).not.toMatch(/id=|name=|evil/)
  })

  it('marks external and note links differently', () => {
    const out = html('[web](https://example.com) [note](other.md#x) [top](#intro)')
    expect(out).toMatch(/<a href="https:\/\/example.com" class="md-ext"/)
    expect(out).toMatch(/<a href="other.md#x" class="md-link"/)
    expect(out).toMatch(/<a href="#intro">top<\/a>/)
  })

  it('shows remote and relative images as placeholders, embedded ones as images', () => {
    const out = html('![cat](https://x/cat.png) ![dog](dog.png) ![dot](data:image/png;base64,iVBORw0KGgo=)')
    expect(out.match(/class="md-img-blocked"/g)).toHaveLength(2)
    expect(out).toContain('<img src="data:image/png;base64,iVBORw0KGgo=" alt="dot">')
  })

  it('shows front matter as a block instead of a rule and heading', () => {
    expect(splitFrontMatter('---\ntitle: x\n---\n# Hi').frontMatter).toBe('title: x')
    expect(splitFrontMatter('# no front matter\n---\n').frontMatter).toBeNull()
    const out = html('---\ntitle: x\n---\n# Hi')
    expect(out).toContain('<pre class="md-frontmatter"><code>title: x</code></pre>')
    expect(out).not.toContain('<hr>')
  })
})

describe('helpers', () => {
  it('reads the language from a fence info string', () => {
    expect(fenceLanguage('json title="x"')).toBe('json')
    expect(fenceLanguage(' Mermaid ')).toBe('mermaid')
    expect(fenceLanguage(undefined)).toBe('')
  })

  it('slugifies like GitHub', () => {
    expect(slugify('Hello, World!')).toBe('hello-world')
    expect(slugify('???')).toBe('section')
  })

  it('escapes code in unknown languages', () => {
    expect(highlightToHtml('<b>&</b>', 'nope')).toBe('&lt;b&gt;&amp;&lt;/b&gt;')
  })
})
