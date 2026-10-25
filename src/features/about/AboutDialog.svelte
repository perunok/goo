<script lang="ts">
  import Button from '../../components/ui/Button.svelte'
  import Dialog from '../../components/ui/Dialog.svelte'
  import { ui } from '../../app/ui.svelte'
  import { updates } from '../updates/updates.svelte'
  import { APP_VERSION, MANIFESTO, MANIFESTO_SIGNOFF, MANIFESTO_TITLE } from './credits'
</script>

<Dialog title="About goo" onClose={() => ui.close()} width="560px">
  <div class="top">
    <img src="/goo.svg" alt="" width="56" height="56" />
    <div>
      <div class="name">goo <span class="ver">{APP_VERSION}</span></div>
      <div class="tag">A small, local-first Markdown editor. MIT licensed.</div>
    </div>
    <div class="check">
      {#if updates.available}
        <Button kind="primary" onclick={() => { ui.close(); updates.openDialog() }}>Update to {updates.available.version}</Button>
      {:else}
        <Button disabled={updates.checking} onclick={() => void updates.check('manual')}>{updates.checking ? 'Checking…' : 'Check for updates'}</Button>
      {/if}
    </div>
  </div>
  <h3>{MANIFESTO_TITLE}</h3>
  <ol>
    {#each MANIFESTO as m (m.title)}
      <li><strong>{m.title}</strong> {m.text}</li>
    {/each}
  </ol>
  <p class="signoff">{MANIFESTO_SIGNOFF}</p>
</Dialog>

<style>
  .top {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 8px;
  }
  .top img {
    border-radius: 12px;
  }
  .check {
    margin-left: auto;
  }
  .name {
    font-size: 1.4em;
    font-weight: 700;
  }
  .ver {
    font-size: 0.6em;
    font-weight: 500;
    color: var(--text-faint);
  }
  .tag {
    color: var(--text-muted);
  }
  h3 {
    margin: 16px 0 6px;
    font-size: 1em;
  }
  ol {
    margin: 0;
    padding-left: 1.4em;
    line-height: 1.55;
  }
  li + li {
    margin-top: 4px;
  }
  li::marker {
    color: var(--accent-text);
    font-weight: 600;
  }
  .signoff {
    margin: 12px 0 0;
    color: var(--accent-text);
    font-style: italic;
  }
</style>
