<script lang="ts">
  import type { TreeNode } from '../../backend/types'
  import Icon from '../../components/ui/Icon.svelte'
  import { workspace } from '../../app/workspace.svelte'
  import Self from './FileTree.svelte'

  let { nodes, depth = 0 }: { nodes: TreeNode[]; depth?: number } = $props()
</script>

<ul role={depth === 0 ? 'tree' : 'group'}>
  {#each nodes as node (node.path)}
    {@const open = !workspace.collapsed.has(node.path)}
    <li role="treeitem" aria-selected={workspace.activePath === node.path} aria-expanded={node.kind === 'dir' ? open : undefined}>
      {#if node.kind === 'dir'}
        <button type="button" class="row dir" style:padding-left="{8 + depth * 14}px" onclick={() => workspace.toggleFolder(node.path)} title={node.name}>
          <span class="chev" class:open><Icon name="chevron" size={12} /></span>
          <span class="name">{node.name}</span>
        </button>
        {#if open && node.children}
          <Self nodes={node.children} depth={depth + 1} />
        {/if}
      {:else}
        <button
          type="button"
          class="row file"
          class:active={workspace.activePath === node.path}
          style:padding-left="{22 + depth * 14}px"
          onclick={() => workspace.openFile(node.path)}
          title={node.name}
        >
          <span class="name">{node.name.replace(/\.(md|markdown|mdown|mkd)$/i, '')}</span>
        </button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 26px;
    padding-right: 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .row.dir {
    color: var(--text);
    font-weight: 500;
  }
  .row.active {
    background: var(--accent-soft);
    color: var(--text);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: inline-grid;
    place-items: center;
    width: 14px;
    color: var(--text-faint);
    transition: transform 0.12s ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
</style>
