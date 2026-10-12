<script lang="ts">
  import type { TreeNode } from '../../backend/types'
  import Icon from '../../components/ui/Icon.svelte'
  import { workspace } from '../../app/workspace.svelte'
  import { canDrop, dragged } from './dnd'
  import Self from './FileTree.svelte'

  interface Props {
    nodes: TreeNode[]
    depth?: number
    /** Right-click on a row. */
    onMenu: (e: MouseEvent, node: TreeNode) => void
  }

  let { nodes, depth = 0, onMenu }: Props = $props()
  let dropTarget = $state<string | null>(null)

  function onkeydown(e: KeyboardEvent, node: TreeNode) {
    if (e.key === 'F2') {
      e.preventDefault()
      void workspace.renameEntry(node)
    } else if (e.key === 'Delete') {
      e.preventDefault()
      void workspace.deleteEntry(node)
    } else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault()
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
      onMenu(new MouseEvent('contextmenu', { clientX: r.left + 24, clientY: r.bottom }), node)
    }
  }

  function ondragstart(e: DragEvent, node: TreeNode) {
    dragged.path = node.path
    e.dataTransfer?.setData('text/plain', node.path) // some webviews only start a drag with data set
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
  }

  function ondragover(e: DragEvent, dir: TreeNode) {
    if (!canDrop(dragged.path, dir.path)) return
    e.preventDefault()
    e.stopPropagation()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
    dropTarget = dir.path
  }

  function ondrop(e: DragEvent, dir: TreeNode) {
    e.preventDefault()
    e.stopPropagation()
    dropTarget = null
    const path = dragged.path
    dragged.path = ''
    if (canDrop(path, dir.path)) void workspace.moveEntry(path, dir.path)
  }
</script>

<ul role={depth === 0 ? 'tree' : 'group'}>
  {#each nodes as node (node.path)}
    {@const open = !workspace.collapsed.has(node.path)}
    <li role="treeitem" aria-selected={workspace.activePath === node.path} aria-expanded={node.kind === 'dir' ? open : undefined}>
      {#if node.kind === 'dir'}
        <button
          type="button"
          class="row dir"
          class:drop={dropTarget === node.path}
          style:padding-left="{8 + depth * 14}px"
          onclick={() => workspace.toggleFolder(node.path)}
          oncontextmenu={(e) => {
            e.preventDefault()
            onMenu(e, node)
          }}
          onkeydown={(e) => onkeydown(e, node)}
          draggable="true"
          ondragstart={(e) => ondragstart(e, node)}
          ondragover={(e) => ondragover(e, node)}
          ondragleave={() => (dropTarget = dropTarget === node.path ? null : dropTarget)}
          ondrop={(e) => ondrop(e, node)}
          ondragend={() => (dragged.path = '')}
          title={node.name}
        >
          <span class="chev" class:open><Icon name="chevron" size={12} /></span>
          <span class="name">{node.name}</span>
        </button>
        {#if open && node.children}
          <Self nodes={node.children} depth={depth + 1} {onMenu} />
        {/if}
      {:else}
        <button
          type="button"
          class="row file"
          class:active={workspace.activePath === node.path}
          style:padding-left="{22 + depth * 14}px"
          onclick={() => workspace.openFile(node.path)}
          oncontextmenu={(e) => {
            e.preventDefault()
            onMenu(e, node)
          }}
          onkeydown={(e) => onkeydown(e, node)}
          draggable="true"
          ondragstart={(e) => ondragstart(e, node)}
          ondragend={() => (dragged.path = '')}
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
  .row.drop {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
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
