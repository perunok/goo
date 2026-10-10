<script lang="ts">
  import { isDirty, workspace } from '../../app/workspace.svelte'
  import Icon from '../../components/ui/Icon.svelte'

  function onauxclick(e: MouseEvent, path: string) {
    if (e.button === 1) void workspace.close(path)
  }
</script>

<div class="tabs" role="tablist">
  {#each workspace.docs as d (d.path)}
    {@const active = d.path === workspace.activePath}
    <div class="tab" class:active role="tab" aria-selected={active} title={d.path}>
      <button type="button" class="label" onclick={() => workspace.select(d.path)} onauxclick={(e) => onauxclick(e, d.path)}>
        {d.name}
      </button>
      <button type="button" class="close" class:dirty={isDirty(d)} aria-label="Close {d.name}" onclick={() => workspace.close(d.path)}>
        <span class="dot"></span>
        <span class="x"><Icon name="x" size={12} /></span>
      </button>
    </div>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    max-width: 220px;
    height: 34px;
    padding: 0 4px 0 12px;
    border-right: 1px solid var(--border);
    color: var(--text-muted);
    box-shadow: inset 0 -2px 0 transparent;
  }
  .tab.active {
    background: var(--surface);
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .close {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-left: 4px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text-faint);
    cursor: pointer;
  }
  .close > * {
    grid-area: 1 / 1;
  }
  .close:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    visibility: hidden;
  }
  .x {
    display: grid;
    visibility: hidden;
  }
  .tab:hover .x,
  .tab.active .x {
    visibility: visible;
  }
  .close.dirty .dot {
    visibility: visible;
  }
  .close.dirty .x {
    visibility: hidden;
  }
  .close.dirty:hover .dot {
    visibility: hidden;
  }
  .close.dirty:hover .x {
    visibility: visible;
  }
</style>
