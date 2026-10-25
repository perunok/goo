<script lang="ts">
  /** Accent colour, independent of the theme. Each swatch is drawn with the active theme so it shows the real colour. */
  import { settings } from '../../app/settings.svelte'
  import Icon from '../../components/ui/Icon.svelte'
  import { ACCENTS, THEME_DEFAULT_ACCENT } from '../../lib/themes'

  const choices = [{ id: THEME_DEFAULT_ACCENT, label: 'Theme default' }, ...ACCENTS]
</script>

<div role="radiogroup" aria-labelledby="accent-heading" class="grid">
  {#each choices as a (a.id)}
    {@const selected = settings.appearance.accent === a.id}
    <label class:selected>
      <input type="radio" name="accent" value={a.id} checked={selected} onchange={() => settings.setAppearance({ accent: a.id })} />
      <span class="swatch" data-theme={settings.resolvedTheme} data-accent={a.id === THEME_DEFAULT_ACCENT ? undefined : a.id} aria-hidden="true">
        {#if selected}<Icon name="check" size={13} />{/if}
      </span>
      {a.label}
    </label>
  {/each}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
    gap: 4px;
  }
  label {
    /* the hidden radio is absolutely positioned: it must belong to its label, not to the dialog */
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px 2px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--text-muted);
    font-size: 0.78em;
    line-height: 1.2;
    text-align: center;
    cursor: pointer;
  }
  label:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  label.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  label:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .swatch {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-fg);
  }
</style>
