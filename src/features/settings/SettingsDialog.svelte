<script lang="ts">
  import Dialog from '../../components/ui/Dialog.svelte'
  import { READING_WIDTH_MAX, READING_WIDTH_MIN, settings } from '../../app/settings.svelte'
  import { ui } from '../../app/ui.svelte'
  import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../../lib/appearance'
  import { ACCENTS, THEMES, THEME_DEFAULT_ACCENT } from '../../lib/themes'
  import McpSettings from './McpSettings.svelte'

  const light = THEMES.filter((t) => t.scheme === 'light')
  const dark = THEMES.filter((t) => t.scheme === 'dark')
  const a = $derived(settings.appearance)
  /** data-accent for the previews ('theme' = none). */
  const previewAccent = $derived(a.accent === THEME_DEFAULT_ACCENT ? undefined : a.accent)
</script>

{#snippet themeCard(t: (typeof THEMES)[number])}
  {@const selected = a.theme === t.id}
  <button type="button" class="card" class:selected aria-pressed={selected} onclick={() => settings.setAppearance({ theme: t.id })}>
    <span class="mini" data-theme={t.id} data-accent={previewAccent}>
      <span class="mini-side"></span>
      <span class="mini-page">
        <span class="mini-h"></span>
        <span class="mini-l"></span>
        <span class="mini-l short"></span>
        <span class="mini-a"></span>
      </span>
    </span>
    <span class="card-label">{t.label}</span>
  </button>
{/snippet}

<Dialog title="Settings" onClose={() => ui.close()} width="760px">
  <section>
    <h3>Theme</h3>
    <label class="check">
      <input type="checkbox" checked={a.theme === 'system'} onchange={(e) => settings.setAppearance({ theme: e.currentTarget.checked ? 'system' : settings.resolvedTheme })} />
      Follow the system (light or dark)
    </label>
    {#if a.theme === 'system'}
      <div class="row">
        <label>
          Light
          <select value={a.systemLight} onchange={(e) => settings.setAppearance({ systemLight: e.currentTarget.value })}>
            {#each light as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
        <label>
          Dark
          <select value={a.systemDark} onchange={(e) => settings.setAppearance({ systemDark: e.currentTarget.value })}>
            {#each dark as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
      </div>
    {/if}
    <h4>Light</h4>
    <div class="grid">{#each light as t (t.id)}{@render themeCard(t)}{/each}</div>
    <h4>Dark</h4>
    <div class="grid">{#each dark as t (t.id)}{@render themeCard(t)}{/each}</div>
  </section>

  <section>
    <h3>Accent colour</h3>
    <div class="accents">
      <button
        type="button"
        class="accent theme-default"
        class:selected={a.accent === THEME_DEFAULT_ACCENT}
        title="The theme's own accent"
        aria-label="Theme default"
        onclick={() => settings.setAppearance({ accent: THEME_DEFAULT_ACCENT })}
      >
        <span class="dot" data-theme={settings.resolvedTheme}></span>
      </button>
      {#each ACCENTS as acc (acc.id)}
        <button
          type="button"
          class="accent"
          class:selected={a.accent === acc.id}
          title={acc.label}
          aria-label={acc.label}
          onclick={() => settings.setAppearance({ accent: acc.id })}
        >
          <span class="dot" data-theme={settings.resolvedTheme} data-accent={acc.id}></span>
        </button>
      {/each}
    </div>
  </section>

  <section>
    <h3>Text</h3>
    <label class="range">
      Font size
      <input type="range" min={FONT_SIZE_MIN} max={FONT_SIZE_MAX} value={a.fontSize} oninput={(e) => settings.setAppearance({ fontSize: Number(e.currentTarget.value) })} />
      <span>{a.fontSize}px</span>
    </label>
    <label class="range">
      Reading width
      <input
        type="range"
        min={READING_WIDTH_MIN}
        max={READING_WIDTH_MAX}
        step="20"
        disabled={settings.ui.readingWidth === 0}
        value={settings.ui.readingWidth || 820}
        oninput={(e) => settings.setUi({ readingWidth: Number(e.currentTarget.value) })}
      />
      <span>{settings.ui.readingWidth ? `${settings.ui.readingWidth}px` : 'Full'}</span>
    </label>
    <label class="check">
      <input type="checkbox" checked={settings.ui.readingWidth === 0} onchange={(e) => settings.setUi({ readingWidth: e.currentTarget.checked ? 0 : 820 })} />
      Use the full window width for text
    </label>
    <label class="check">
      <input type="checkbox" checked={settings.ui.wrap} onchange={(e) => settings.setUi({ wrap: e.currentTarget.checked })} />
      Wrap long lines in the editor
    </label>
  </section>

  <McpSettings />
</Dialog>

<style>
  section + section {
    margin-top: 18px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 0.95em;
  }
  h4 {
    margin: 12px 0 6px;
    font-size: 0.8em;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }
  .row {
    display: flex;
    gap: 16px;
    margin: 8px 0;
  }
  .row label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
  }
  select {
    height: 28px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0;
  }
  .check input,
  .range input {
    accent-color: var(--accent);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(118px, 1fr));
    gap: 10px;
  }
  .card {
    display: grid;
    gap: 5px;
    padding: 4px;
    border: 2px solid transparent;
    border-radius: calc(var(--radius) + 2px);
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.85em;
    cursor: pointer;
  }
  .card:hover {
    color: var(--text);
  }
  .card.selected {
    border-color: var(--accent);
    color: var(--text);
  }
  .mini {
    display: flex;
    height: 62px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }
  .mini-side {
    width: 24%;
    background: var(--bg);
    border-right: 1px solid var(--border);
  }
  .mini-page {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px;
    background: var(--surface);
  }
  .mini-h {
    width: 55%;
    height: 6px;
    border-radius: 2px;
    background: var(--syn-keyword);
  }
  .mini-l {
    width: 90%;
    height: 4px;
    border-radius: 2px;
    background: var(--text-muted);
    opacity: 0.6;
  }
  .mini-l.short {
    width: 65%;
  }
  .mini-a {
    width: 34%;
    height: 9px;
    margin-top: auto;
    border-radius: 3px;
    background: var(--accent);
  }
  .card-label {
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .accents {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .accent {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 50%;
    background: none;
    cursor: pointer;
  }
  .accent.selected {
    border-color: var(--text);
  }
  .dot {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
  }
  .theme-default .dot {
    background: conic-gradient(var(--accent) 0 50%, var(--surface-raised) 0);
    border: 1px solid var(--border-strong);
  }
  .range {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
  }
  .range input {
    width: 220px;
  }
</style>
