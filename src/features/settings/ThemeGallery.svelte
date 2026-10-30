<script lang="ts">
  /**
   * Theme picker: live thumbnails grouped Light / Dark, a filter, and which themes "System" switches between. Every
   * thumbnail is drawn with its own theme and the chosen accent (ThemePreview).
   */
  import { settings } from '../../app/settings.svelte'
  import Icon from '../../components/ui/Icon.svelte'
  import { THEMES, THEME_DEFAULT_ACCENT, type Scheme, type ThemeInfo } from '../../lib/themes'
  import ThemePreview from './ThemePreview.svelte'

  let query = $state('')
  let show = $state<'all' | Scheme>('all')

  const a = $derived(settings.appearance)
  const accent = $derived(a.accent === THEME_DEFAULT_ACCENT ? null : a.accent)
  const words = $derived(query.trim().toLowerCase().split(/\s+/).filter(Boolean))
  const matches = (t: ThemeInfo) => (show === 'all' || t.scheme === show) && words.every((w) => `${t.label} ${t.scheme}`.toLowerCase().includes(w))
  const groups = $derived((['light', 'dark'] as const).map((scheme) => ({ scheme, title: scheme === 'light' ? 'Light' : 'Dark', items: THEMES.filter((t) => t.scheme === scheme && matches(t)) })))
  const showSystem = $derived(show === 'all' && words.every((w) => 'system follows os light dark'.includes(w)))
  const empty = $derived(!showSystem && groups.every((g) => g.items.length === 0))
  const lights = THEMES.filter((t) => t.scheme === 'light')
  const darks = THEMES.filter((t) => t.scheme === 'dark')
</script>

<div class="filters">
  <input type="search" placeholder="Filter themes…" aria-label="Filter themes" bind:value={query} />
  <div role="group" aria-label="Show themes" class="seg">
    {#each [['all', 'All'], ['light', 'Light'], ['dark', 'Dark']] as const as [id, label] (id)}
      <button type="button" aria-pressed={show === id} class:on={show === id} onclick={() => (show = id)}>{label}</button>
    {/each}
  </div>
  <span class="count">{THEMES.length} themes</span>
</div>

<div role="radiogroup" aria-labelledby="theme-heading" class="gallery">
  {#if showSystem}
    <div class="system">
      <label class="card" class:selected={a.theme === 'system'}>
        <input type="radio" name="theme" value="system" checked={a.theme === 'system'} onchange={() => settings.setAppearance({ theme: 'system' })} />
        <span class="pair" aria-hidden="true">
          <ThemePreview theme={a.systemLight} {accent} />
          <ThemePreview theme={a.systemDark} {accent} />
        </span>
        <span class="name">
          <span class="label">System <span class="faint">follows your OS</span></span>
          {#if a.theme === 'system'}<Icon name="check" size={13} />{/if}
        </span>
      </label>
      <div class="pickers">
        <label>
          <Icon name="sun" size={14} />
          <span>When the OS is light</span>
          <select value={a.systemLight} onchange={(e) => settings.setAppearance({ systemLight: e.currentTarget.value })}>
            {#each lights as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
        <label>
          <Icon name="moon" size={14} />
          <span>When the OS is dark</span>
          <select value={a.systemDark} onchange={(e) => settings.setAppearance({ systemDark: e.currentTarget.value })}>
            {#each darks as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
          </select>
        </label>
      </div>
    </div>
  {/if}

  {#each groups as g (g.scheme)}
    {#if g.items.length}
      <section aria-label="{g.title} themes">
        <h4>{g.title}</h4>
        <div class="cards">
          {#each g.items as t (t.id)}
            <label class="card" class:selected={a.theme === t.id} data-theme-option={t.id}>
              <input type="radio" name="theme" value={t.id} checked={a.theme === t.id} onchange={() => settings.setAppearance({ theme: t.id })} />
              <ThemePreview theme={t.id} {accent} />
              <span class="name">
                <span class="label">{t.label}</span>
                {#if a.theme === t.id}<Icon name="check" size={13} />{/if}
              </span>
            </label>
          {/each}
        </div>
      </section>
    {/if}
  {/each}

  {#if empty}<p class="none">No themes match “{query}”.</p>{/if}
</div>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .filters input {
    width: 14rem;
  }
  .seg {
    display: flex;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 0.85em;
  }
  .seg button {
    padding: 4px 10px;
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .seg button:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--text);
  }
  .count {
    color: var(--text-faint);
    font-size: 0.85em;
  }
  .gallery {
    display: grid;
    gap: 14px;
  }
  h4 {
    margin: 0 0 6px;
    color: var(--text-muted);
    font-size: 0.75em;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .card:hover {
    background: var(--surface-hover);
  }
  .card.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .card:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  .card input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 2px;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .faint {
    color: var(--text-faint);
    font-size: 0.85em;
  }
  .name :global(svg) {
    flex-shrink: 0;
    color: var(--accent-text);
  }
  .system {
    display: grid;
    grid-template-columns: minmax(150px, 220px) 1fr;
    align-items: center;
    gap: 14px;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .pickers {
    display: grid;
    gap: 8px;
    font-size: 0.88em;
  }
  .pickers label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
  }
  .pickers span {
    width: 9rem;
    flex-shrink: 0;
  }
  select {
    height: 28px;
    min-width: 0;
    max-width: 14rem;
    flex: 1;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
  }
  .none {
    padding: 14px 0;
    color: var(--text-muted);
    text-align: center;
  }
</style>
