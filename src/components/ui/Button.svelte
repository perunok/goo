<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    kind?: 'primary' | 'danger' | 'default' | 'ghost'
    type?: 'button' | 'submit'
    onclick?: (e: MouseEvent) => void
    disabled?: boolean
    children: Snippet
  }

  let { kind = 'default', type = 'button', onclick, disabled = false, children }: Props = $props()
</script>

<button {type} class="btn {kind}" {onclick} {disabled}>{@render children()}</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .btn.primary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 88%, var(--text));
  }
  .btn.danger {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--danger-fg);
  }
  .btn.ghost {
    background: transparent;
    border-color: transparent;
    color: var(--text-muted);
  }
  .btn.ghost:hover:not(:disabled) {
    color: var(--text);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
