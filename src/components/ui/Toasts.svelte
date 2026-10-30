<script lang="ts">
  import { ui } from '../../app/ui.svelte'
  import IconButton from './IconButton.svelte'
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each ui.toasts as t (t.id)}
    <div class="toast {t.kind}">
      <span class="text">
        {t.text}
        {#if t.detail}<span class="detail">{t.detail}</span>{/if}
      </span>
      {#if t.action}
        {@const action = t.action}
        <button type="button" class="action" onclick={() => { ui.dismiss(t.id); action.run() }}>{action.label}</button>
      {/if}
      <IconButton icon="close" label="Dismiss" size={14} onclick={() => ui.dismiss(t.id)} />
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 36px;
    display: grid;
    gap: 8px;
    z-index: 50;
    max-width: min(440px, calc(100vw - 32px));
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 6px 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
    box-shadow: var(--shadow-pop);
  }
  .toast.error {
    border-color: var(--danger);
    background: var(--danger-soft);
  }
  .text {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .detail {
    display: block;
    color: var(--text-muted);
    font-size: 0.88em;
  }
  .action {
    flex-shrink: 0;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-fg);
    font: inherit;
    cursor: pointer;
  }
</style>
