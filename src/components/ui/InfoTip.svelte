<script lang="ts">
  /**
   * A small (i) button whose explanation shows on hover or keyboard focus (Escape hides it). The panel is positioned
   * against the window, so a scrolling dialog body cannot clip it; it opens above the button, or below when there is no
   * room.
   */
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'

  interface Props {
    /** Accessible name of the button, e.g. "About update checks". */
    label: string
    children: Snippet
  }

  let { label, children }: Props = $props()

  const WIDTH = 320
  const id = `infotip-${Math.random().toString(36).slice(2, 10)}`
  let button = $state<HTMLButtonElement | null>(null)
  let open = $state(false)
  let pos = $state({ left: 0, top: 0, above: true })

  function show() {
    if (!button) return
    const r = button.getBoundingClientRect()
    const above = r.top > 180
    pos = { left: Math.max(8, Math.min(r.left - 8, window.innerWidth - WIDTH - 8)), top: above ? r.top - 6 : r.bottom + 6, above }
    open = true
  }
  const hide = () => (open = false)
</script>

<span class="wrap" role="presentation" onmouseenter={show} onmouseleave={hide}>
  <button
    bind:this={button}
    type="button"
    aria-label={label}
    aria-describedby={open ? id : undefined}
    onfocus={show}
    onblur={hide}
    onkeydown={(e) => {
      if (e.key === 'Escape' && open) {
        e.stopPropagation()
        hide()
      }
    }}
  >
    <Icon name="info" size={14} />
  </button>
  {#if open}
    <span {id} role="tooltip" style:left="{pos.left}px" style:top="{pos.top}px" style:width="{WIDTH}px" style:transform={pos.above ? 'translateY(-100%)' : undefined}>
      {@render children()}
    </span>
  {/if}
</span>

<style>
  .wrap {
    display: inline-flex;
  }
  button {
    display: inline-flex;
    padding: 2px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text-muted);
    cursor: help;
  }
  button:hover {
    color: var(--text);
  }
  [role='tooltip'] {
    position: fixed;
    z-index: 70;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
    color: var(--text);
    font-size: 0.86em;
    line-height: 1.45;
    pointer-events: none;
  }
</style>
