<script lang="ts">
  /**
   * Settings, laid out like Slinger's: sections down the left, the chosen one on the right, the version and Done in
   * the footer. Each section keeps its own state in a store; this only arranges them.
   */
  import { onMount } from 'svelte'
  import { READING_WIDTH_MAX, READING_WIDTH_MIN, settings } from '../../app/settings.svelte'
  import { SETTINGS_TABS, ui } from '../../app/ui.svelte'
  import Button from '../../components/ui/Button.svelte'
  import Dialog from '../../components/ui/Dialog.svelte'
  import InfoTip from '../../components/ui/InfoTip.svelte'
  import Tabs from '../../components/ui/Tabs.svelte'
  import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../../lib/appearance'
  import { ago } from '../../lib/format'
  import { APP_VERSION } from '../about/credits'
  import CloudSettings from '../cloud/CloudSettings.svelte'
  import { updates } from '../updates/updates.svelte'
  import AccentPicker from './AccentPicker.svelte'
  import McpSettings from './McpSettings.svelte'
  import ThemeGallery from './ThemeGallery.svelte'

  // "checked 5 min ago" stays current while the dialog is open
  let now = $state(Math.floor(Date.now() / 1000))
  onMount(() => {
    const timer = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000)
    return () => clearInterval(timer)
  })

  const tabs = $derived(SETTINGS_TABS.map((t) => ({ id: t.id, label: t.label, dot: t.id === 'updates' && !!updates.available })))
  const a = $derived(settings.appearance)
  const phase = $derived(updates.install?.phase)
  const progress = $derived(updates.install?.total ? Math.floor((updates.install.received / updates.install.total) * 100) : 0)
</script>

<Dialog title="Settings" onClose={() => ui.close()} width="920px" height="min(70vh, 42rem)" flush>
  <div class="layout">
    <div class="nav">
      <Tabs label="Settings sections" idPrefix="settings" orientation="vertical" value={ui.settingsTab} onchange={(id) => (ui.settingsTab = id as typeof ui.settingsTab)} {tabs} />
    </div>
    <div class="panel" role="tabpanel" id="settings-panel-{ui.settingsTab}" aria-labelledby="settings-{ui.settingsTab}">
      {#if ui.settingsTab === 'appearance'}
        <section>
          <h3 id="theme-heading">Theme</h3>
          <ThemeGallery />
        </section>
        <section>
          <h3 id="accent-heading">Accent colour</h3>
          <AccentPicker />
        </section>
        <section class="field">
          <label for="font-size">Font size: {a.fontSize}px</label>
          <input id="font-size" type="range" min={FONT_SIZE_MIN} max={FONT_SIZE_MAX} step="1" value={a.fontSize} oninput={(e) => settings.setAppearance({ fontSize: Number(e.currentTarget.value) })} />
        </section>
      {:else if ui.settingsTab === 'layout'}
        <section class="stack">
          <h3>Layout</h3>
          <label class="check">
            <input type="checkbox" checked={settings.ui.sidebar} onchange={(e) => settings.setUi({ sidebar: e.currentTarget.checked })} />
            Show the sidebar <span class="faint">(Ctrl+\)</span>
          </label>
          <label class="check">
            <input type="checkbox" checked={settings.ui.statusBar} onchange={(e) => settings.setUi({ statusBar: e.currentTarget.checked })} />
            Show the status bar
          </label>
          <div class="row">
            <label for="reading-width">Reading width</label>
            <input
              id="reading-width"
              type="range"
              min={READING_WIDTH_MIN}
              max={READING_WIDTH_MAX}
              step="20"
              disabled={settings.ui.readingWidth === 0}
              value={settings.ui.readingWidth || 820}
              oninput={(e) => settings.setUi({ readingWidth: Number(e.currentTarget.value) })}
            />
            <span class="faint">{settings.ui.readingWidth ? `${settings.ui.readingWidth}px` : 'Full'}</span>
            <InfoTip label="About the reading width">
              How wide a line of text may get in the preview, and in the editor when it is shown alone. Also the button next to the
              Edit / Split / Preview switch at the top.
            </InfoTip>
          </div>
          <label class="check">
            <input type="checkbox" checked={settings.ui.readingWidth === 0} onchange={(e) => settings.setUi({ readingWidth: e.currentTarget.checked ? 0 : 820 })} />
            Use the full window width for text
          </label>
        </section>
        <section class="stack">
          <h3>Editor & notes</h3>
          <label class="check">
            <input type="checkbox" checked={settings.ui.wrap} onchange={(e) => settings.setUi({ wrap: e.currentTarget.checked })} />
            Wrap long lines in the editor
          </label>
          <div class="row">
            <label class="check">
              <input type="checkbox" checked={settings.ui.restoreSession} onchange={(e) => settings.setUi({ restoreSession: e.currentTarget.checked })} />
              Reopen the last folder and notes on startup
            </label>
            <InfoTip label="About reopening notes">
              Next time you start goo it opens the folder you had open and the notes you had open in it, in the same order. Unsaved
              text is not kept (goo asks before closing a note with unsaved changes). Turning this off also erases what is currently
              remembered.
            </InfoTip>
          </div>
        </section>
      {:else if ui.settingsTab === 'cloud'}
        <CloudSettings />
      {:else if ui.settingsTab === 'updates'}
        <section class="stack" aria-labelledby="updates-heading">
          <h3 id="updates-heading">Updates</h3>
          <div class="row">
            <label class="check">
              <input type="checkbox" checked={updates.auto} onchange={(e) => updates.setAuto(e.currentTarget.checked)} />
              Check for new releases automatically
            </label>
            <InfoTip label="About update checks">
              Once a day goo asks GitHub which version of goo is the latest release (github.com/perunok/goo) and shows a
              notification when there is a newer one. The request sends nothing about you or your notes. Nothing is downloaded until
              you click Update now; then goo downloads the installer from GitHub, checks it against GitHub's checksum, and
              installs it when you restart.
            </InfoTip>
          </div>
          <div class="status" data-testid="update-status" aria-live="polite">
            {#if updates.available}
              <strong>goo {updates.available.version} is available.</strong>
              {#if updates.canInstall}
                <Button kind="primary" onclick={() => (phase === 'ready' ? updates.openDialog() : void updates.updateNow())}>
                  {phase === 'downloading' || phase === 'verifying' ? `Downloading… ${progress}%` : phase === 'ready' ? 'Restart to update' : phase === 'installing' ? 'Installing…' : 'Update now'}
                </Button>
              {:else}
                <Button kind="primary" onclick={() => updates.openRelease()}>View release</Button>
              {/if}
            {:else if updates.error}
              <span class="bad">Could not check: {updates.error}</span>
            {:else if updates.lastCheckedAt !== null}
              <span class="muted">Up to date. Checked {ago(updates.lastCheckedAt, now * 1000)}.</span>
            {/if}
            <Button disabled={updates.checking} onclick={() => void updates.check('settings').then(() => (now = Math.floor(Date.now() / 1000)))}>
              {updates.checking ? 'Checking…' : 'Check now'}
            </Button>
          </div>
          {#if !updates.canInstall && updates.install?.unsupportedReason}
            <p class="faint small">{updates.install.unsupportedReason} New versions are on the release page.</p>
          {/if}
        </section>
      {:else}
        <McpSettings />
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <span class="version">goo {APP_VERSION}</span>
    <Button kind="primary" onclick={() => ui.close()}>Done</Button>
  {/snippet}
</Dialog>

<style>
  .layout {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .nav {
    flex-shrink: 0;
    width: 11.5rem;
    padding: 8px 0;
    overflow: auto;
  }
  .panel {
    /* anything absolutely positioned inside (visually hidden radios) stays inside and scrolls with the panel */
    position: relative;
    flex: 1;
    min-width: 0;
    padding: 16px 20px;
    overflow: auto;
  }
  section {
    margin-bottom: 22px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 0.95em;
    font-weight: 600;
  }
  .field {
    display: grid;
    gap: 4px;
  }
  .field label {
    font-weight: 600;
  }
  input[type='range'] {
    width: 16rem;
    accent-color: var(--accent);
  }
  .stack {
    display: grid;
    gap: 10px;
  }
  .stack h3 {
    margin: 0;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .check input {
    accent-color: var(--accent);
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .muted {
    color: var(--text-muted);
  }
  .faint {
    color: var(--text-faint);
  }
  .bad {
    color: var(--danger);
  }
  .small {
    margin: 0;
    font-size: 0.85em;
  }
  .version {
    margin-right: auto;
    color: var(--text-faint);
    font-size: 0.85em;
  }
</style>
