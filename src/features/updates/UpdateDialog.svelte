<script lang="ts">
  /**
   * Update goo: download the new release (progress, speed, time left, cancel), then "Restart to update". The work
   * happens in the backend (crates/goo-update); this only shows `updates.install` and calls the store.
   */
  import { ui } from '../../app/ui.svelte'
  import Button from '../../components/ui/Button.svelte'
  import Dialog from '../../components/ui/Dialog.svelte'
  import InfoTip from '../../components/ui/InfoTip.svelte'
  import { formatBytes } from '../../lib/format'
  import { methodNote, percent, timeLeft } from './progress'
  import { updates } from './updates.svelte'

  const s = $derived(updates.install)
  const version = $derived(s?.version ?? updates.available?.version ?? null)
  const busy = $derived(s?.phase === 'installing')
  const pct = $derived(s ? percent(s) : 0)
  const release = $derived(updates.available ?? updates.latest)

  function close() {
    if (!busy) ui.updateOpen = false
  }
</script>

<Dialog title="Update goo" width="480px" onClose={close} {busy}>
  <div class="content" data-testid="update-dialog">
    <div class="head">
      {#if version}
        <span><strong>goo {version}</strong>{updates.currentVersion ? ` · you have ${updates.currentVersion}` : ''}</span>
        <button type="button" class="link" onclick={() => updates.openRelease(release)}>Release notes</button>
      {:else}
        <span>You have goo {updates.currentVersion}.</span>
      {/if}
      {#if s?.method}<InfoTip label="How goo updates">{methodNote(s.method)}</InfoTip>{/if}
    </div>

    {#if !s?.method}
      <p class="muted" data-testid="update-unsupported">{s?.unsupportedReason ?? 'This installation cannot update itself.'} Download the new version from the release page.</p>
    {:else if s.phase === 'downloading' || s.phase === 'verifying'}
      <div data-testid="update-progress">
        <div class="bar" role="progressbar" aria-label="Download progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={s.phase === 'verifying' ? 100 : pct}>
          <div class="fill" class:pulse={s.phase === 'verifying'} style:width="{s.phase === 'verifying' ? 100 : pct}%"></div>
        </div>
        <p class="stats muted" data-testid="update-progress-text">
          {#if s.phase === 'verifying'}
            <span>Checking the download…</span>
          {:else}
            <span>{s.total ? `${formatBytes(s.received)} of ${formatBytes(s.total)} (${pct}%)` : `${formatBytes(s.received)} downloaded`}</span>
            <span>{s.bytesPerSec ? `${formatBytes(s.bytesPerSec)}/s${timeLeft(s) ? ` · ${timeLeft(s)} left` : ''}` : ''}</span>
          {/if}
        </p>
      </div>
    {:else if s.phase === 'ready'}
      <p data-testid="update-ready">
        Downloaded and verified.
        <span class="muted">Restart to install it{s.method === 'deb' ? ' (your system asks for your password)' : ''}; goo reopens your folder and notes, and saves unsaved changes first.</span>
      </p>
    {:else if s.phase === 'installing'}
      <p data-testid="update-installing">{s.method === 'deb' ? 'Installing… enter your password in the system dialog. goo restarts when it is done.' : 'Installing… goo restarts in a moment.'}</p>
    {:else if s.phase === 'error'}
      <p class="error" role="alert" data-testid="update-error">{s.error}</p>
    {:else}
      <p class="muted">Download the new version, then restart to install it.</p>
    {/if}
  </div>

  {#snippet footer()}
    {#if !s?.method}
      <Button onclick={close}>Close</Button>
      <Button kind="primary" onclick={() => updates.openRelease(release)}>View release</Button>
    {:else if s.phase === 'downloading' || s.phase === 'verifying'}
      <Button onclick={close}>Hide</Button>
      <Button onclick={() => void updates.cancelDownload()} disabled={s.phase === 'verifying'}>Cancel</Button>
    {:else if s.phase === 'ready'}
      <Button onclick={close}>Later</Button>
      <Button kind="primary" onclick={() => void updates.restartToUpdate()}>Restart to update</Button>
    {:else if s.phase === 'installing'}
      <Button kind="primary" disabled>Installing…</Button>
    {:else}
      <Button onclick={close}>Close</Button>
      {#if s.phase === 'error'}<Button onclick={() => updates.openRelease(release)}>View release</Button>{/if}
      <Button kind="primary" disabled={!updates.available} onclick={() => void updates.updateNow()}>{s.phase === 'error' ? 'Try again' : 'Update now'}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  p {
    margin: 0;
    line-height: 1.5;
  }
  .muted {
    color: var(--text-muted);
  }
  .error {
    color: var(--danger);
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font-size: 0.85em;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .bar {
    height: 8px;
    overflow: hidden;
    border-radius: 4px;
    background: var(--surface-raised);
  }
  .fill {
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
    transition: width 0.2s;
  }
  .fill.pulse {
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px;
    margin-top: 6px;
    font-size: 0.85em;
  }
</style>
