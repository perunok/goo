<script lang="ts">
  import { workspace } from '../../app/workspace.svelte'
  import { ui } from '../../app/ui.svelte'
  import Button from '../../components/ui/Button.svelte'
  import Icon from '../../components/ui/Icon.svelte'
  import IconButton from '../../components/ui/IconButton.svelte'
  import { MANIFESTO_SIGNOFF } from '../about/credits'
</script>

<section class="welcome">
  <img src="/goo.svg" alt="" width="88" height="88" />
  <h1>goo</h1>
  <p class="tag">Plain Markdown, your folders, your colours.</p>
  <div class="actions">
    <Button kind="primary" onclick={() => workspace.pickFolder()}>
      <Icon name="folderOpen" /> Open a folder
    </Button>
    <Button onclick={() => workspace.pickFile()}>
      <Icon name="file" /> Open a note
    </Button>
  </div>

  {#if workspace.recents.length}
    <div class="recents">
      <h2>Recent</h2>
      <ul>
        {#each workspace.recents as r (r.root)}
          <li>
            <button type="button" class="recent" onclick={() => workspace.openRecent(r.root)} title={r.root}>
              <Icon name="folder" size={14} />
              <span class="rname">{r.name}</span>
              <span class="rpath">{r.root}</span>
            </button>
            <IconButton icon="x" label="Remove from recent" size={12} onclick={() => workspace.forget(r.root)} />
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <dl class="keys">
    <dt><kbd>Ctrl</kbd> <kbd>O</kbd></dt><dd>Open folder</dd>
    <dt><kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>O</kbd></dt><dd>Open note</dd>
    <dt><kbd>Ctrl</kbd> <kbd>N</kbd></dt><dd>New note</dd>
    <dt><kbd>Ctrl</kbd> <kbd>E</kbd></dt><dd>Edit / split / preview</dd>
    <dt><kbd>Ctrl</kbd> <kbd>,</kbd></dt><dd>Themes and accents</dd>
  </dl>

  <button type="button" class="signoff" onclick={() => ui.open('about')}>{MANIFESTO_SIGNOFF}</button>
</section>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: 100%;
    padding: 24px;
    overflow: auto;
    text-align: center;
  }
  img {
    border-radius: 22px;
    box-shadow: var(--shadow-pop);
  }
  h1 {
    margin: 8px 0 0;
    font-size: 2.2em;
    letter-spacing: -0.02em;
  }
  .tag {
    margin: 0 0 10px;
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }
  .recents {
    width: min(520px, 100%);
    margin-top: 18px;
    text-align: left;
  }
  h2 {
    margin: 0 0 6px 8px;
    font-size: 0.8em;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .recent {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .recent:hover {
    background: var(--surface-hover);
  }
  .rname {
    font-weight: 500;
  }
  .rpath {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-faint);
    font-size: 0.9em;
  }
  .keys {
    display: grid;
    grid-template-columns: auto auto;
    gap: 6px 14px;
    margin: 22px 0 0;
    color: var(--text-muted);
    font-size: 0.9em;
  }
  dt {
    text-align: right;
  }
  dd {
    margin: 0;
    text-align: left;
  }
  kbd {
    padding: 1px 6px;
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--surface-raised);
    font-family: var(--font-mono);
    font-size: 0.85em;
  }
  .signoff {
    margin-top: 18px;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    font-style: italic;
    cursor: pointer;
  }
</style>
