<script lang="ts">
  import { isDirty, workspace } from '../../app/workspace.svelte'
  import { settings } from '../../app/settings.svelte'
  import { ui } from '../../app/ui.svelte'
  import { relativeTo } from '../../lib/paths'
  import { findTheme } from '../../lib/themes'
  import { ago, cloud } from '../cloud/cloud.svelte'
  import { updates } from '../updates/updates.svelte'

  const CLOUD_LABEL: Record<string, string> = { idle: 'Synced', syncing: 'Syncing…', offline: 'Offline', error: 'Sync problem', signed_out: 'Signed out' }
  const sync = $derived(cloud.current)
  const syncIssues = $derived(sync ? sync.status.conflicts.length + sync.status.quarantined.length : 0)

  const doc = $derived(workspace.active)
  // Counted on a short delay so very long notes do not slow typing down.
  let counts = $state({ words: 0, chars: 0 })
  $effect(() => {
    const text = doc?.text ?? ''
    const t = setTimeout(() => {
      counts = { words: (text.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) ?? []).length, chars: text.length }
    }, 150)
    return () => clearTimeout(t)
  })
</script>

<footer class="status">
  {#if ui.assistantTool}
    <span class="assistant" title="An AI assistant is using goo (MCP)">AI assistant: {ui.assistantTool.replace(/_/g, ' ')}…</span>
  {/if}
  <span class="path">
    {#if doc && workspace.workspace}{relativeTo(workspace.workspace.root, doc.path)}{:else if workspace.workspace}{workspace.workspace.root}{/if}
  </span>
  {#if doc}
    <span>{counts.words.toLocaleString()} words</span>
    <span>{counts.chars.toLocaleString()} chars</span>
    <span class:unsaved={isDirty(doc)}>{isDirty(doc) ? 'Unsaved' : 'Saved'}</span>
  {/if}
  {#if updates.available}
    <button type="button" class="update" onclick={() => updates.openDialog()} title="goo {updates.available.version} is available (you have {updates.currentVersion})">
      Update available
    </button>
  {/if}
  {#if sync}
    <button
      type="button"
      class="cloud {sync.status.state}"
      class:warn={syncIssues > 0}
      onclick={() => ui.open('settings', 'cloud')}
      title={`goo cloud: ${sync.workspaceName}. Last synced ${ago(sync.status.lastSyncedAt)}.${sync.status.message ? ' ' + sync.status.message : ''}`}
    >
      {syncIssues ? `${syncIssues} to check` : (CLOUD_LABEL[sync.status.state] ?? sync.status.state)}
    </button>
  {/if}
  <button type="button" class="theme" onclick={() => ui.open('settings', 'appearance')} title="Themes and accents (Ctrl+,)">
    <span class="swatch"></span>{findTheme(settings.resolvedTheme)?.label ?? settings.resolvedTheme}
  </button>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 26px;
    padding: 0 12px;
    border-top: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-faint);
    font-size: 0.85em;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .unsaved {
    color: var(--accent-text);
  }
  .assistant {
    color: var(--accent-text);
    white-space: nowrap;
  }
  .update {
    padding: 0 8px;
    border: 0;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent-text);
    font: inherit;
    cursor: pointer;
  }
  .update:hover {
    background: var(--accent);
    color: var(--accent-fg);
  }
  .cloud {
    padding: 0 4px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .cloud.syncing {
    color: var(--accent-text);
  }
  .cloud.error,
  .cloud.warn {
    color: var(--warning);
  }
  .theme {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .theme:hover {
    color: var(--text);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent);
  }
</style>
