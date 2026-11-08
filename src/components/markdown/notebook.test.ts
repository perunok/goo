import { notebookPreview, notebookToMarkdown } from './notebook'
import { renderMarkdown } from './render'

const nb = (cells: unknown[], language = 'python') => JSON.stringify({ nbformat: 4, metadata: { language_info: { name: language } }, cells })

describe('notebooks', () => {
  it('keeps markdown cells and fences code in the kernel language', () => {
    const md = notebookToMarkdown(nb([
      { cell_type: 'markdown', source: ['# Title\n', 'Text'] },
      { cell_type: 'code', source: 'x = 1', outputs: [] },
      { cell_type: 'code', source: '', outputs: [] },
    ]))
    expect(md).toBe('# Title\nText\n\n```python\nx = 1\n```\n')
  })

  it('shows text, errors and images; strips terminal colours', () => {
    const md = notebookToMarkdown(nb([
      {
        cell_type: 'code',
        source: 'go()',
        outputs: [
          { output_type: 'stream', name: 'stdout', text: ['hello\n'] },
          { output_type: 'stream', name: 'stderr', text: 'careful' },
          { output_type: 'error', ename: 'ValueError', evalue: 'bad', traceback: ['\u001b[0;31mValueError\u001b[0m: bad'] },
          { output_type: 'display_data', data: { 'image/png': 'iVBO\nRw0=', 'text/plain': '<Figure>' } },
          { output_type: 'execute_result', data: { 'text/plain': ['42'] } },
        ],
      },
    ]))
    expect(md).toContain('<div class="md-nb-out">\n\n```\nhello\n```\n\n</div>')
    expect(md).toContain('<div class="md-nb-err">\n\n```\ncareful\n```')
    expect(md).toContain('```\nValueError: bad\n```')
    expect(md).toContain('![output](data:image/png;base64,iVBORw0=)')
    expect(md).not.toContain('<Figure>')
    expect(md).toContain('```\n42\n```')
  })

  it('a fence inside a cell cannot close the block early', () => {
    const md = notebookToMarkdown(nb([{ cell_type: 'code', source: 'print("```")', outputs: [] }]))
    expect(md).toBe('````python\nprint("```")\n````\n')
  })

  it('HTML output keeps tables but loses scripts and styles once rendered', () => {
    const html = '<style>td{color:red}</style>\n<table>\n\n<tr><td onclick="x()">1</td></tr>\n</table>\n<script>alert(1)</script>'
    const md = notebookToMarkdown(nb([{ cell_type: 'code', source: 'df', outputs: [{ output_type: 'execute_result', data: { 'text/html': html, 'text/plain': 'df' } }] }]))
    const out = renderMarkdown(md).html
    expect(out).toContain('<table>')
    expect(out).toContain('<td>1</td>')
    expect(out).not.toMatch(/script|style|onclick|alert/)
  })

  it('embeds pasted images from attachments', () => {
    const md = notebookToMarkdown(nb([{ cell_type: 'markdown', source: '![a](attachment:a.png)', attachments: { 'a.png': { 'image/png': 'AAAA' } } }]))
    expect(md).toBe('![a](data:image/png;base64,AAAA)\n')
  })

  it('says when an output cannot be shown, and reads nbformat 3', () => {
    expect(notebookToMarkdown(nb([{ cell_type: 'code', source: 'w', outputs: [{ output_type: 'display_data', data: { 'application/vnd.jupyter.widget-view+json': {} } }] }]))).toContain('Interactive output')
    const v3 = JSON.stringify({ nbformat: 3, worksheets: [{ cells: [{ cell_type: 'code', input: 'x', outputs: [{ output_type: 'pyout', text: '1' }] }] }] })
    expect(notebookToMarkdown(v3)).toBe('```\nx\n```\n\n<div class="md-nb-out">\n\n```\n1\n```\n\n</div>\n')
  })

  it('explains broken files instead of throwing', () => {
    expect(notebookPreview('{not json')).toMatch(/^> \*\*This notebook could not be shown:\*\*/)
    expect(notebookPreview('{"a": 1}')).toContain('not a Jupyter notebook')
  })
})
