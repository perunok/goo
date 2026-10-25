<script lang="ts">
  import { workspace } from '../../app/workspace.svelte'
  import { backend } from '../../app/backend'
  import { ui } from '../../app/ui.svelte'
  import IconButton from '../../components/ui/IconButton.svelte'
  import Icon from '../../components/ui/Icon.svelte'
  import { relativeTo } from '../../lib/paths'
  import FileTree from './FileTree.svelte'
  import ContextMenu, { type MenuItem } from '../../components/ui/ContextMenu.svelte'
  import type { NoteMatch, TreeNode } from '../../backend/types'
  import { canDrop, dragged } from './dnd'

  let filter = $state('')
  let menu = $state<{ x: number; y: number; items: (MenuItem | null)[] } | null>(null)
  let rootDrop = $state(false)

  function nodeMenu(e: MouseEvent, node: TreeNode) {
    const items: (MenuItem | null)[] =
      node.kind === 'dir'
        ? [
            { label: 'New note here', icon: 'filePlus', action: () => workspace.newNote(node.path) },
            { label: 'New folder here', icon: 'folderPlus', action: () => workspace.newFolder(node.path) },
            null,
            { label: 'Rename…', icon: 'pencil', hint: 'F2', action: () => workspace.renameEntry(node) },
            { label: 'Show in file manager', icon: 'folderOpen', action: () => workspace.revealEntry(node.path) },
            null,
            { label: 'Delete folder', icon: 'trash', hint: 'Del', danger: true, action: () => workspace.deleteEntry(node) },
          ]
        : [
            { label: 'Open', icon: 'file', action: () => workspace.openFile(node.path) },
            { label: 'Rename…', icon: 'pencil', hint: 'F2', action: () => workspace.renameEntry(node) },
            { label: 'Show in file manager', icon: 'folderOpen', action: () => workspace.revealEntry(node.path) },
            null,
            { label: 'Delete', icon: 'trash', hint: 'Del', danger: true, action: () => workspace.deleteEntry(node) },
          ]
    menu = { x: e.clientX, y: e.clientY, items }
  }

  function rootMenu(e: MouseEvent) {
    const root = workspace.workspace?.root
    if (!root || (e.target as HTMLElement).closest('.row')) return
    e.preventDefault()
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'New note', icon: 'filePlus', action: () => workspace.newNote() },
        { label: 'New folder', icon: 'folderPlus', action: () => workspace.newFolder() },
        null,
        { label: 'Show in file manager', icon: 'folderOpen', action: () => workspace.revealEntry(root) },
      ],
    }
  }

  function onRootDragOver(e: DragEvent) {
    const root = workspace.workspace?.root
    if (!root || !canDrop(dragged.path, root)) return
    e.preventDefault()
    rootDrop = true
  }

  function onRootDrop(e: DragEvent) {
    e.preventDefault()
    rootDrop = false
    const root = workspace.workspace?.root
    const path = dragged.path
    dragged.path = ''
    if (root && canDrop(path, root)) void workspace.moveEntry(path, root)
  }

  // Keyword search (names and text, every word must appear), a moment after typing stops.
  let input: HTMLInputElement | undefined = $state()
  let results = $state<NoteMatch[]>([])
  let searching = $state(false)
  let searchSeq = 0
  const words = $derived(filter.toLowerCase().split(/\s+/).filter(Boolean))

  $effect(() => {
    const q = filter.trim()
    void workspace.tree // search again when notes change on disk
    const seq = ++searchSeq
    if (!q || !workspace.workspace) {
      results = []
      searching = false
      return
    }
    searching = true
    const timer = setTimeout(async () => {
      try {
        const found = await backend.findNotes(q)
        if (seq === searchSeq) results = found
      } catch {
        if (seq === searchSeq) results = []
      } finally {
        if (seq === searchSeq) searching = false
      }
    }, 150)
    return () => clearTimeout(timer)
  })

  // Ctrl+F
  $effect(() => {
    if (ui.findPending && input) {
      ui.findPending = false
      input.focus()
      input.select()
    }
  })

  /** `text` cut into plain and keyword parts, for highlighting. */
  function marked(text: string): { t: string; hit: boolean }[] {
    if (!words.length) return [{ t: text, hit: false }]
    const re = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi')
    return text
      .split(re)
      .filter(Boolean)
      .map((t) => ({ t, hit: words.includes(t.toLowerCase()) }))
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') filter = ''
    if (e.key === 'Enter' && results[0]) void workspace.openFile(results[0].path)
  }
</script>

<aside class="sidebar">
  {#if workspace.workspace}
    <div class="head">
      <span class="ws" title={workspace.workspace.root}>{workspace.workspace.name}</span>
      <IconButton icon="filePlus" label="New note (Ctrl+N)" onclick={() => workspace.newNote()} />
      <IconButton icon="folderPlus" label="New folder" onclick={() => workspace.newFolder()} />
      <IconButton icon="refresh" label="Refresh" onclick={() => workspace.refreshTree()} />
      <IconButton icon="file" label="Open note (Ctrl+Shift+O)" onclick={() => workspace.pickFile()} />
      <IconButton icon="folderOpen" label="Open folder (Ctrl+O)" onclick={() => workspace.pickFolder()} />
    </div>
    <label class="filter">
      <Icon name="search" size={14} />
      <input bind:this={input} bind:value={filter} placeholder="Search notes (Ctrl+F)" aria-label="Search notes by keywords" spellcheck="false" {onkeydown} />
    </label>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="scroll"
      class:drop={rootDrop}
      oncontextmenu={rootMenu}
      ondragover={onRootDragOver}
      ondragleave={(e) => {
        if (e.target === e.currentTarget) rootDrop = false
      }}
      ondrop={onRootDrop}
    >
      {#if filter.trim()}
        {#each results as m (m.path)}
          {@const rel = relativeTo(workspace.workspace?.root ?? '', m.path)}
          <button type="button" class="match" class:active={workspace.activePath === m.path} onclick={() => workspace.openFile(m.path)} title={rel}>
            <span class="mname">{#each marked(rel) as p, i (i)}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
            {#each m.lines as l (l.line)}
              <span class="mline"><span class="ln">{l.line}</span>{#each marked(l.text) as p, i (i)}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
            {/each}
          </button>
        {:else}
          <p class="empty">{searching ? 'Searching…' : `No notes contain “${filter.trim()}”.`}</p>
        {/each}
      {:else if workspace.tree && workspace.tree.nodes.length}
        <FileTree nodes={workspace.tree.nodes} onMenu={nodeMenu} />
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

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
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
    border-radius: var(--radius);
  }
  .scroll.drop {
    box-shadow: inset 0 0 0 1px var(--accent);
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
  .mname,
  .mline {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mline {
    color: var(--text-faint);
    font-size: 0.85em;
  }
  .ln {
    display: inline-block;
    min-width: 2.2em;
    margin-right: 4px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }
  mark {
    border-radius: 2px;
    background: var(--accent-soft);
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
