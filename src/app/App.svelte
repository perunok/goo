<script lang="ts">
  import { backend } from './backend'
  import { SIDEBAR_WIDTH_DEFAULT, SIDEBAR_WIDTH_MIN, settings } from './settings.svelte'
  import { ui } from './ui.svelte'
  import { isDirty, workspace } from './workspace.svelte'
  import { handleShortcut, handleZoomWheel } from './shortcuts'
  import { errorMessage } from '../backend/types'
  import { isMarkdownPath, resolveLink } from '../lib/paths'
  import Editor from '../components/editor/Editor.svelte'
  import MarkdownView from '../components/markdown/MarkdownView.svelte'
  import IconButton from '../components/ui/IconButton.svelte'
  import ConfirmHost from '../components/ui/ConfirmHost.svelte'
  import Toasts from '../components/ui/Toasts.svelte'
  import Sidebar from '../features/files/Sidebar.svelte'
  import TabBar from '../features/files/TabBar.svelte'
  import StatusBar from '../features/files/StatusBar.svelte'
  import Welcome from '../features/files/Welcome.svelte'
  import SettingsDialog from '../features/settings/SettingsDialog.svelte'
  import AboutDialog from '../features/about/AboutDialog.svelte'
  import WidthControl from '../features/settings/WidthControl.svelte'

  const doc = $derived(workspace.active)
  const mode = $derived(settings.ui.viewMode)
  let preview: MarkdownView | undefined = $state()

  $effect(() => {
    const parts = doc ? [`${doc.name}${isDirty(doc) ? ' •' : ''}`] : []
    if (workspace.workspace) parts.push(workspace.workspace.name)
    parts.push('goo')
    void backend.setWindowTitle(parts.join(' — ')).catch(() => {})
  })

  // AI assistants (MCP) may not write notes with unsaved edits: keep the backend's list current.
  $effect(() => {
    const key = workspace.unsavedKey
    void backend.setUnsaved(key ? key.split('\n') : []).catch(() => {})
  })

  // Not `<svelte:window onwheel>`: wheel listeners on the window are passive by default and could not stop the
  // webview's own page zoom.
  $effect(() => {
    window.addEventListener('wheel', handleZoomWheel, { passive: false })
    return () => window.removeEventListener('wheel', handleZoomWheel)
  })

  /** The editor keeps at least this much of the window (px), however wide the sidebar is set. */
  const MAIN_MIN_WIDTH = 240
  /** Letting go of the handle left of this closes the sidebar (px from the window's edge). */
  const SIDEBAR_CLOSE_AT = 110
  const SIDEBAR_KEY_STEP = 16

  /** The sidebar while its handle is being dragged; saved when the drag ends. */
  let sidebarDrag = $state<{ open: boolean; width: number } | null>(null)
  const sidebarOpen = $derived(sidebarDrag ? sidebarDrag.open : settings.ui.sidebar)
  const sidebarWidth = $derived(sidebarDrag ? sidebarDrag.width : settings.ui.sidebarWidth)

  function onResizeStart(e: PointerEvent) {
    if (e.button !== 0) return
    e.preventDefault()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    sidebarDrag = { open: settings.ui.sidebar, width: settings.ui.sidebarWidth }
  }

  function onResizeMove(e: PointerEvent) {
    if (!sidebarDrag) return
    const open = e.clientX >= SIDEBAR_CLOSE_AT
    // a closing drag keeps the old width, for when the sidebar opens again
    sidebarDrag = { open, width: open ? Math.max(SIDEBAR_WIDTH_MIN, Math.min(e.clientX, window.innerWidth - MAIN_MIN_WIDTH)) : settings.ui.sidebarWidth }
  }

  function onResizeEnd() {
    if (!sidebarDrag) return
    settings.setUi({ sidebar: sidebarDrag.open, sidebarWidth: sidebarDrag.width })
    sidebarDrag = null
  }

  function onResizeKey(e: KeyboardEvent) {
    const delta = e.key === 'ArrowRight' ? SIDEBAR_KEY_STEP : e.key === 'ArrowLeft' ? -SIDEBAR_KEY_STEP : 0
    if (delta) settings.setUi({ sidebar: true, sidebarWidth: settings.ui.sidebarWidth + delta })
    else if (e.key === 'Enter') settings.setUi({ sidebar: !settings.ui.sidebar })
    else return
    e.preventDefault()
  }

  function onkeydown(e: KeyboardEvent) {
    if (handleShortcut(e)) e.preventDefault()
  }

  function openLink(href: string) {
    if (!doc) return
    const target = resolveLink(doc.path, href)
    if (target && isMarkdownPath(target.path)) void workspace.openFile(target.path, { anchor: target.fragment })
    else ui.toast('Only links to other notes open in goo for now.')
  }

  function openExternal(url: string) {
    backend.openExternal(url).catch((e) => ui.toast(errorMessage(e), 'error'))
  }
</script>

<svelte:window {onkeydown} />

<div
  class="app"
  class:with-sidebar={!!workspace.workspace}
  class:sidebar-closed={!sidebarOpen}
  class:resizing={!!sidebarDrag}
  style:--sidebar-width="clamp({SIDEBAR_WIDTH_MIN}px, {sidebarWidth}px, max({SIDEBAR_WIDTH_MIN}px, 100vw - {MAIN_MIN_WIDTH}px))"
>
  {#if workspace.workspace}
    <!-- Stays mounted while closed, so the filter and the tree's scroll position survive a toggle. -->
    <div class="side" inert={!sidebarOpen}>
      <Sidebar />
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (a focusable separator is a splitter) -->
    <div
      class="resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      aria-valuenow={sidebarOpen ? sidebarWidth : 0}
      title="Drag to resize or close, double-click to reset"
      tabindex="0"
      onpointerdown={onResizeStart}
      onpointermove={onResizeMove}
      onpointerup={onResizeEnd}
      onpointercancel={onResizeEnd}
      ondblclick={() => settings.setUi({ sidebar: true, sidebarWidth: SIDEBAR_WIDTH_DEFAULT })}
      onkeydown={onResizeKey}
    ></div>
  {/if}

  <main>
    <div class="topbar">
      {#if workspace.workspace}
        <IconButton icon="sidebar" label="Toggle sidebar (Ctrl+\)" active={settings.ui.sidebar} onclick={() => settings.setUi({ sidebar: !settings.ui.sidebar })} />
      {/if}
      <TabBar />
      <div class="spacer"></div>
      {#if doc}
        <div class="modes" role="group" aria-label="View">
          <IconButton icon="edit" label="Edit" active={mode === 'edit'} onclick={() => settings.setUi({ viewMode: 'edit' })} />
          <IconButton icon="split" label="Split" active={mode === 'split'} onclick={() => settings.setUi({ viewMode: 'split' })} />
          <IconButton icon="eye" label="Preview" active={mode === 'preview'} onclick={() => settings.setUi({ viewMode: 'preview' })} />
          <WidthControl />
        </div>
      {/if}
      <IconButton icon="settings" label="Settings (Ctrl+,)" onclick={() => ui.open('settings')} />
      <IconButton icon="help" label="About goo" onclick={() => ui.open('about')} />
    </div>

    <div class="content" style:--reading-width={settings.ui.readingWidth ? `${settings.ui.readingWidth}px` : 'none'}>
      {#if !workspace.workspace}
        <Welcome />
      {:else if doc}
        <div class="panes {mode}">
          {#if mode !== 'preview'}
            <Editor
              docId={doc.path}
              text={doc.text}
              wrap={settings.ui.wrap}
              openIds={workspace.docs.map((d) => d.path)}
              onChange={(t) => workspace.setText(doc.path, t)}
              onScroll={(r) => preview?.scrollToRatio(r)}
            />
          {/if}
          {#if mode !== 'edit'}
            {#key doc.path}
              <MarkdownView
                bind:this={preview}
                source={doc.text}
                paletteVersion={settings.paletteVersion}
                anchor={doc.pendingAnchor}
                onAnchorShown={() => (doc.pendingAnchor = null)}
                onOpenLink={openLink}
                onOpenExternal={openExternal}
              />
            {/key}
          {/if}
        </div>
      {:else}
        <div class="nodoc">
          <p>Pick a note on the left, or</p>
          <button type="button" class="link" onclick={() => workspace.newNote()}>create a new one</button>
        </div>
      {/if}
    </div>
    <StatusBar />
  </main>
</div>

{#if ui.dialog === 'settings'}<SettingsDialog />{/if}
{#if ui.dialog === 'about'}<AboutDialog />{/if}
<ConfirmHost />
<Toasts />

<style>
  .app {
    position: relative;
    /* the frame (sidebar, tabs, status bar) keeps its size; only .content zooms */
    font-size: 14px;
    display: grid;
    grid-template-columns: 1fr;
    /* one row exactly the window's height: a long file tree scrolls inside the sidebar instead of stretching the row */
    grid-template-rows: minmax(0, 1fr);
    height: 100vh;
  }
  .app.with-sidebar {
    grid-template-columns: var(--sidebar-width) 1fr;
    transition: grid-template-columns 0.16s ease;
  }
  .app.with-sidebar.sidebar-closed {
    grid-template-columns: 0px 1fr;
  }
  .app.resizing {
    cursor: col-resize;
    user-select: none;
  }
  .app.with-sidebar.resizing,
  .resizing .side {
    transition: none;
  }
  /* sits on the sidebar's edge (the window's left edge while closed), so it is grabbed from either side */
  .resizer {
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(var(--sidebar-width) - 4px);
    z-index: 5;
    width: 8px;
    cursor: col-resize;
    touch-action: none;
    transition: left 0.16s ease;
  }
  .sidebar-closed .resizer {
    left: 0;
    width: 6px;
  }
  .resizing .resizer {
    transition: none;
  }
  .resizer::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 2px;
    background: transparent;
    transition: background 0.12s ease 0.1s;
  }
  .sidebar-closed .resizer::after {
    left: 0;
  }
  .resizer:hover::after,
  .resizer:focus-visible::after,
  .resizing .resizer::after {
    background: var(--accent);
  }
  .resizer:focus-visible {
    outline: none;
  }
  .side {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    transition: visibility 0.16s;
  }
  /* full width while the column animates, so the rows slide instead of re-wrapping */
  .side > :global(.sidebar) {
    width: var(--sidebar-width);
  }
  .sidebar-closed .side {
    visibility: hidden;
  }
  @media (prefers-reduced-motion: reduce) {
    .app.with-sidebar,
    .side,
    .resizer {
      transition: none;
    }
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 36px;
    padding: 0 6px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .spacer {
    flex: 1;
  }
  .modes {
    display: flex;
    gap: 2px;
    margin-right: 6px;
    padding-right: 6px;
    border-right: 1px solid var(--border);
  }
  .content {
    font-size: var(--font-size);
    flex: 1;
    min-height: 0;
    background: var(--surface);
  }
  .panes {
    display: grid;
    height: 100%;
    min-height: 0;
  }
  .panes.split {
    grid-template-columns: 1fr 1fr;
  }
  .panes.split > :global(:first-child) {
    border-right: 1px solid var(--border);
  }
  /* Edit-only: the text column gets the reading width too, centred. */
  .panes.edit :global(.cm-content) {
    max-width: var(--reading-width);
    margin: 0 auto;
  }
  .nodoc {
    display: grid;
    place-content: center;
    justify-items: center;
    height: 100%;
    color: var(--text-faint);
  }
  .nodoc p {
    margin: 0 0 4px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  @media (max-width: 760px) {
    .panes.split {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 1fr;
    }
  }
</style>
