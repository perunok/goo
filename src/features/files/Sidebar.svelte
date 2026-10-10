<script lang="ts">
  import { flattenFiles, workspace } from '../../app/workspace.svelte'
  import IconButton from '../../components/ui/IconButton.svelte'
  import Icon from '../../components/ui/Icon.svelte'
  import { relativeTo } from '../../lib/paths'
  import FileTree from './FileTree.svelte'

  let filter = $state('')

  const matches = $derived.by(() => {
    const q = filter.trim().toLowerCase()
    if (!q || !workspace.tree || !workspace.workspace) return []
    const root = workspace.workspace.root
    return flattenFiles(workspace.tree.nodes)
      .map((n) => ({ node: n, rel: relativeTo(root, n.path) }))
      .filter((m) => m.rel.toLowerCase().includes(q))
      .slice(0, 200)
  })

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') filter = ''
    if (e.key === 'Enter' && matches[0]) void workspace.openFile(matches[0].node.path)
  }
</script>

<aside class="sidebar">
  {#if workspace.workspace}
    <div class="head">
      <span class="ws" title={workspace.workspace.root}>{workspace.workspace.name}</span>
      <IconButton icon="filePlus" label="New note (Ctrl+N)" onclick={() => workspace.newNote()} />
      <IconButton icon="refresh" label="Refresh" onclick={() => workspace.refreshTree()} />
      <IconButton icon="folderOpen" label="Open folder (Ctrl+O)" onclick={() => workspace.pickFolder()} />
    </div>
    <label class="filter">
      <Icon name="search" size={14} />
      <input bind:value={filter} placeholder="Filter notes" spellcheck="false" {onkeydown} />
    </label>
    <div class="scroll">
      {#if filter.trim()}
        {#each matches as m (m.node.path)}
          <button type="button" class="match" class:active={workspace.activePath === m.node.path} onclick={() => workspace.openFile(m.node.path)} title={m.rel}>
            {m.rel}
          </button>
        {:else}
          <p class="empty">No notes match “{filter}”.</p>
        {/each}
      {:else if workspace.tree && workspace.tree.nodes.length}
        <FileTree nodes={workspace.tree.nodes} />
        {#if workspace.tree.truncated}<p class="empty">This folder is very large; not every note is listed.</p>{/if}
      {:else if workspace.tree}
        <div class="empty">
          <p>No Markdown files here yet.</p>
          <button type="button" class="link" onclick={() => workspace.newNote()}>Create the first note</button>
        </div>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
    background: var(--bg);
    border-right: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 8px 6px 4px 12px;
  }
  .ws {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 8px 6px;
    padding: 0 8px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-faint);
  }
  .filter:focus-within {
    border-color: var(--focus-ring);
  }
  .filter input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 6px 12px;
  }
  .match {
    display: block;
    width: 100%;
    padding: 4px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .match:hover,
  .match.active {
    background: var(--surface-hover);
    color: var(--text);
  }
  .empty {
    margin: 8px;
    color: var(--text-faint);
    font-size: 0.92em;
  }
  .empty p {
    margin: 0 0 6px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }
</style>
