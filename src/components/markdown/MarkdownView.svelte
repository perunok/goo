<script lang="ts">
  /**
   * Rendered Markdown with Mermaid diagrams. Rendering is debounced while typing; diagrams are drawn after the HTML
   * is in place and redrawn when the palette changes (`paletteVersion`). Every link click is intercepted (render.ts
   * explains the policy).
   */
  import { tick, untrack } from 'svelte'
  import { EXTERNAL_URL, renderMarkdown } from './render'
  import { renderMermaid } from './mermaid'
  import './markdown.css'

  interface Props {
    source: string
    /** Bumps when theme/accent/font change, so diagrams are redrawn in the new colours. */
    paletteVersion: number
    /** Fragment to scroll to after the next render. */
    anchor?: string | null
    onAnchorShown?: () => void
    onOpenLink: (href: string) => void
    onOpenExternal: (url: string) => void
  }

  let { source, paletteVersion, anchor = null, onAnchorShown, onOpenLink, onOpenExternal }: Props = $props()

  let scroller: HTMLDivElement
  let content: HTMLDivElement
  // The first render is immediate (the parent keys this view by document); later edits are debounced.
  let html = $state(renderMarkdown(untrack(() => source)).html)
  let rendered = untrack(() => source)
  let generation = 0

  $effect(() => {
    const src = source
    if (src === rendered) return
    const timer = setTimeout(() => {
      rendered = src
      html = renderMarkdown(src).html
    }, 120)
    return () => clearTimeout(timer)
  })

  // After the HTML is in the DOM (and on palette changes): draw the diagrams, then honour a pending anchor.
  $effect(() => {
    void html
    void paletteVersion
    void anchor
    const gen = ++generation
    untrack(() => void drawDiagrams(gen))
  })

  async function drawDiagrams(gen: number) {
    await tick() // let {@html} reach the DOM first
    if (gen !== generation) return
    const blocks = Array.from(content.querySelectorAll<HTMLElement>('pre.md-mermaid'))
    // A palette change: the drawn diagrams need redrawing too, from the source we kept on them.
    const drawn = Array.from(content.querySelectorAll<HTMLElement>('.md-diagram[data-source]'))
    for (const el of [...blocks, ...drawn]) {
      const src = el.dataset.source ?? el.textContent ?? ''
      const result = await renderMermaid(src)
      if (gen !== generation || !el.isConnected) return
      const box = document.createElement('div')
      if ('svg' in result) {
        box.className = 'md-diagram'
        box.innerHTML = result.svg
      } else {
        box.className = 'md-diagram-error'
        const msg = document.createElement('p')
        msg.textContent = `Mermaid: ${result.error}`
        const pre = document.createElement('pre')
        pre.textContent = src
        box.append(msg, pre)
      }
      box.dataset.source = src
      el.replaceWith(box)
    }
    if (gen === generation && anchor) showAnchor(anchor)
  }

  function showAnchor(slug: string) {
    const target = Array.from(content.querySelectorAll<HTMLElement>('[data-anchor]')).find((h) => h.dataset.anchor === slug)
    if (target) target.scrollIntoView({ block: 'start' })
    onAnchorShown?.()
  }

  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a')
    if (!a) return
    e.preventDefault()
    const href = a.getAttribute('href') ?? ''
    if (href.startsWith('#')) showAnchor(decodeURIComponent(href.slice(1)))
    else if (EXTERNAL_URL.test(href)) onOpenExternal(href)
    else if (href) onOpenLink(href)
  }

  /** Follow the editor in split view (0..1). */
  export function scrollToRatio(ratio: number) {
    const max = scroller.scrollHeight - scroller.clientHeight
    scroller.scrollTop = ratio * max
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="preview" bind:this={scroller}>
  <div class="md-doc page" bind:this={content} {onclick}>
    {@html html}
  </div>
</div>

<style>
  .preview {
    height: 100%;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
  }
  .page {
    max-width: var(--reading-width, 820px);
    margin: 0 auto;
    padding: 28px 32px 40vh;
  }
</style>
