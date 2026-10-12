<script lang="ts" module>
  import type { IconName } from './icons'

  export interface MenuItem {
    label: string
    icon?: IconName
    /** Shown on the right, e.g. "F2". */
    hint?: string
    danger?: boolean
    action: () => void
  }
</script>

<script lang="ts">
  /** A right-click menu at (x, y), kept inside the window. Closes on Esc, a click elsewhere, scrolling or choosing. */
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'

  interface Props {
    x: number
    y: number
    /** `null` draws a separator. */
    items: (MenuItem | null)[]
    onClose: () => void
  }

  let { x, y, items, onClose }: Props = $props()
  let menu: HTMLDivElement
  let left = $state(0)
  let top = $state(0)

  onMount(() => {
    const r = menu.getBoundingClientRect()
    left = Math.max(4, Math.min(x, window.innerWidth - r.width - 4))
    top = Math.max(4, Math.min(y, window.innerHeight - r.height - 4))
    menu.querySelector<HTMLButtonElement>('button')?.focus()
  })

  function onpointerdown(e: PointerEvent) {
    if (!menu.contains(e.target as Node)) onClose()
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const buttons = [...menu.querySelectorAll<HTMLButtonElement>('button')]
      const i = buttons.indexOf(document.activeElement as HTMLButtonElement)
      buttons[(i + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus()
    }
  }

  function choose(item: MenuItem) {
    onClose()
    item.action()
  }
</script>

<svelte:window {onpointerdown} {onkeydown} onblur={onClose} onresize={onClose} />

<div class="menu" role="menu" bind:this={menu} style:left="{left}px" style:top="{top}px">
  {#each items as item, i (i)}
    {#if item}
      <button type="button" role="menuitem" class:danger={item.danger} onclick={() => choose(item)}>
        <span class="icon">{#if item.icon}<Icon name={item.icon} size={14} />{/if}</span>
        <span class="label">{item.label}</span>
        {#if item.hint}<span class="hint">{item.hint}</span>{/if}
      </button>
    {:else}
      <div class="sep" role="separator"></div>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 200px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) + 2px);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover,
  button:focus-visible {
    background: var(--surface-hover);
    outline: none;
  }
  button.danger {
    color: var(--danger);
  }
  .icon {
    display: inline-grid;
    place-items: center;
    width: 16px;
    color: var(--text-muted);
  }
  .danger .icon {
    color: inherit;
  }
  .label {
    flex: 1;
  }
  .hint {
    color: var(--text-faint);
    font-size: 0.85em;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
