/**
 * Jupyter notebooks (.ipynb), read-only: the notebook JSON becomes Markdown for the normal preview, so it goes
 * through the same renderer and sanitiser as a note (render.ts). Markdown cells stay Markdown, code cells become
 * fenced code in the kernel's language, and saved outputs follow: text, errors, embedded images, HTML (sanitised,
 * so tables stay and scripts go) or a note that an interactive output cannot be shown. Nothing is ever run.
 */

type Multiline = string | string[]

interface Output {
  output_type?: string
  name?: string
  text?: Multiline
  data?: Record<string, Multiline | object>
  ename?: string
  evalue?: string
  traceback?: string[]
}

interface Cell {
  cell_type?: string
  /** `input` in nbformat 3. */
  source?: Multiline
  input?: Multiline
  outputs?: Output[]
  attachments?: Record<string, Record<string, Multiline>>
}

interface Notebook {
  cells?: Cell[]
  /** nbformat 3 kept its cells here. */
  worksheets?: { cells?: Cell[] }[]
  metadata?: { language_info?: { name?: string }; kernelspec?: { language?: string } }
}

const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif']

const join = (v: Multiline | undefined) => (Array.isArray(v) ? v.join('') : (v ?? ''))
const stripAnsi = (s: string) => s.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '')

/** A code fence that the text cannot close early (longer than any backtick run inside it). */
function fence(text: string, lang = ''): string {
  const longest = Math.max(0, ...(text.match(/`+/g) ?? []).map((r) => r.length))
  const ticks = '`'.repeat(Math.max(3, longest + 1))
  return `${ticks}${lang}\n${text.replace(/\n$/, '')}\n${ticks}`
}

/** An output block: a `<div>` the stylesheet marks as output, with Markdown inside (blank lines let it parse). */
function outputBlock(markdown: string, kind: 'out' | 'err' = 'out'): string {
  return `<div class="md-nb-${kind}">\n\n${markdown}\n\n</div>`
}

/** HTML output as one HTML block: no blank lines (they would end the block), no styles or scripts. */
function htmlBlock(html: string): string {
  const clean = html
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/gi, '')
    .replace(/\n\s*\n/g, '\n')
    .trim()
  return `<div class="md-nb-out md-nb-html">\n${clean}\n</div>`
}

function renderOutput(o: Output): string | null {
  switch (o.output_type) {
    case 'stream': {
      const text = stripAnsi(join(o.text))
      return text.trim() ? outputBlock(fence(text), o.name === 'stderr' ? 'err' : 'out') : null
    }
    case 'error': {
      const trace = (o.traceback ?? []).map(stripAnsi).join('\n')
      return outputBlock(fence(trace || `${o.ename ?? 'Error'}: ${o.evalue ?? ''}`), 'err')
    }
    case 'execute_result':
    case 'display_data':
    case 'pyout': {
      const data = o.data ?? {}
      for (const type of IMAGE_TYPES) {
        const b64 = data[type]
        if (typeof b64 === 'string' || Array.isArray(b64)) return outputBlock(`![output](data:${type};base64,${join(b64).replace(/\s/g, '')})`)
      }
      if (data['text/markdown'] !== undefined) return outputBlock(join(data['text/markdown'] as Multiline))
      if (data['text/html'] !== undefined) return htmlBlock(join(data['text/html'] as Multiline))
      if (data['text/plain'] !== undefined) return outputBlock(fence(stripAnsi(join(data['text/plain'] as Multiline))))
      // nbformat 3 put the text right on the output
      if (o.text !== undefined) return outputBlock(fence(stripAnsi(join(o.text))))
      return Object.keys(data).length ? outputBlock('*Interactive output: open the notebook in Jupyter to see it.*') : null
    }
    default:
      return null
  }
}

/** Markdown cells may point at pasted images with `attachment:name.png`; those become embedded images. */
function withAttachments(md: string, attachments: Cell['attachments']): string {
  if (!attachments) return md
  return md.replace(/attachment:([^\s)"']+)/g, (whole, name: string) => {
    const files = attachments[decodeURIComponent(name)] ?? {}
    const type = IMAGE_TYPES.find((t) => files[t] !== undefined)
    return type ? `data:${type};base64,${join(files[type]).replace(/\s/g, '')}` : whole
  })
}

/** The notebook as Markdown. Throws when `json` is not a notebook. */
export function notebookToMarkdown(json: string): string {
  const nb = JSON.parse(json) as Notebook
  const cells = nb?.cells ?? nb?.worksheets?.[0]?.cells
  if (!Array.isArray(cells)) throw new Error('this is not a Jupyter notebook (no cells)')
  const lang = (nb.metadata?.language_info?.name ?? nb.metadata?.kernelspec?.language ?? '').toLowerCase().replace(/[^a-z0-9+#.-]/g, '')
  const parts: string[] = []
  for (const cell of cells) {
    const source = join(cell.source ?? cell.input)
    if (cell.cell_type === 'markdown') {
      if (source.trim()) parts.push(withAttachments(source, cell.attachments))
    } else if (cell.cell_type === 'code') {
      if (source.trim()) parts.push(fence(source, lang))
      for (const o of cell.outputs ?? []) {
        const out = renderOutput(o)
        if (out) parts.push(out)
      }
    } else if (source.trim()) {
      // raw (and nbformat 3 heading) cells: shown as they are
      parts.push(cell.cell_type === 'heading' ? `## ${source}` : fence(source))
    }
  }
  return parts.join('\n\n') + '\n'
}

/** For the preview: the notebook as Markdown, or an explanation when it cannot be read. */
export function notebookPreview(json: string): string {
  try {
    return notebookToMarkdown(json)
  } catch (e) {
    return `> **This notebook could not be shown:** ${e instanceof Error ? e.message : String(e)}\n`
  }
}
