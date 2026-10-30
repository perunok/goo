<script lang="ts">
  /** Accessible tab list (roving tabindex, arrow keys). Panels are rendered by the parent. */
  export interface TabDef {
    id: string
    label: string
    /** A small dot after the label (something needs attention). */
    dot?: boolean
  }

  interface Props {
    tabs: TabDef[]
    value: string
    onchange: (id: string) => void
    label: string
    idPrefix?: string
    /** Vertical lists sit beside their panel and use Up/Down keys. */
    orientation?: 'horizontal' | 'vertical'
  }

  let { tabs, value, onchange, label, idPrefix = 'tab', orientation = 'horizontal' }: Props = $props()
  const vertical = $derived(orientation === 'vertical')

  function onkeydown(e: KeyboardEvent) {
    const i = tabs.findIndex((t) => t.id === value)
    let next = -1
    if (e.key === (vertical ? 'ArrowDown' : 'ArrowRight')) next = (i + 1) % tabs.length
    else if (e.key === (vertical ? 'ArrowUp' : 'ArrowLeft')) next = (i - 1 + tabs.length) % tabs.length
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = tabs.length - 1
    if (next < 0) return
    e.preventDefault()
    onchange(tabs[next]!.id)
    queueMicrotask(() => document.getElementById(`${idPrefix}-${tabs[next]!.id}`)?.focus())
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div role="tablist" aria-label={label} aria-orientation={orientation} class:vertical {onkeydown} tabindex="-1">
  {#each tabs as t (t.id)}
    <button
      type="button"
      role="tab"
      id="{idPrefix}-{t.id}"
      aria-selected={t.id === value}
      aria-controls="{idPrefix}-panel-{t.id}"
      tabindex={t.id === value ? 0 : -1}
      class:active={t.id === value}
      onclick={() => onchange(t.id)}
    >
      {t.label}
      {#if t.dot}<span class="dot" aria-label="needs attention"></span>{/if}
    </button>
  {/each}
</div>

<style>
  [role='tablist'] {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--border);
  }
  [role='tablist'].vertical {
    flex-direction: column;
    border-bottom: 0;
    border-right: 1px solid var(--border);
  }
  button {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 7px 12px;
    border: 0;
    background: none;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
    box-shadow: inset 0 -2px 0 transparent;
  }
  .vertical button {
    margin-right: -1px;
    box-shadow: inset -2px 0 0 transparent;
  }
  button:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  button.active {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .vertical button.active {
    background: var(--accent-soft);
    box-shadow: inset -2px 0 0 var(--accent);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
