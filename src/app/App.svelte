<script lang="ts">
  import { backend } from './backend'
  import { settings } from './settings.svelte'
  import { ui } from './ui.svelte'
  import { isDirty, workspace } from './workspace.svelte'
  import { handleShortcut } from './shortcuts'
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
  import UpdateDialog from '../features/updates/UpdateDialog.svelte'
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

<div class="app" class:with-sidebar={!!workspace.workspace && settings.ui.sidebar}>
  {#if workspace.workspace && settings.ui.sidebar}
    <Sidebar />
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
    {#if settings.ui.statusBar}<StatusBar />{/if}
  </main>
</div>

{#if ui.dialog === 'settings'}<SettingsDialog />{/if}
{#if ui.dialog === 'about'}<AboutDialog />{/if}
{#if ui.updateOpen}<UpdateDialog />{/if}
<ConfirmHost />
<Toasts />

<style>
  .app {
    display: grid;
    grid-template-columns: 1fr;
    height: 100vh;
  }
  .app.with-sidebar {
    grid-template-columns: minmax(200px, 260px) 1fr;
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
