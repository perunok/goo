<script lang="ts">
  /** Modal on top of the native <dialog> (focus trap, Esc and the backdrop come for free). */
  import type { Snippet } from 'svelte'
  import { onMount } from 'svelte'
  import IconButton from './IconButton.svelte'

  interface Props {
    title: string
    onClose: () => void
    width?: string
    children: Snippet
    footer?: Snippet
  }

  let { title, onClose, width = '480px', children, footer }: Props = $props()
  let dlg: HTMLDialogElement

  onMount(() => {
    dlg.showModal()
    return () => dlg.close()
  })

  function oncancel(e: Event) {
    e.preventDefault()
    onClose()
  }

  function onclick(e: MouseEvent) {
    if (e.target === dlg) onClose() // backdrop
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dlg} {oncancel} {onclick} style:width aria-label={title}>
  <header>
    <h2>{title}</h2>
    <IconButton icon="close" label="Close" onclick={onClose} />
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</dialog>

<style>
  dialog {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    padding: 0;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) + 4px);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
  }
  dialog:not([open]) {
    display: none;
  }
  dialog::backdrop {
    background: var(--overlay);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 8px 18px;
  }
  h2 {
    margin: 0;
    font-size: 1.05em;
    font-weight: 600;
  }
  .body {
    padding: 4px 18px 16px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--border);
  }
</style>
