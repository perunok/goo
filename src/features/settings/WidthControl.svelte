<script lang="ts">
  /** Top-bar control for the reading width: presets and a slider in a small popover. */
  import { READING_WIDTH_MAX, READING_WIDTH_MIN, READING_WIDTH_PRESETS, settings } from '../../app/settings.svelte'
  import IconButton from '../../components/ui/IconButton.svelte'

  let open = $state(false)
  let box: HTMLDivElement

  const width = $derived(settings.ui.readingWidth)
  let lastFixed = $state(820)
  $effect(() => {
    if (width > 0) lastFixed = width
  })

  function onpointerdown(e: PointerEvent) {
    if (open && !box.contains(e.target as Node)) open = false
  }

  function onkeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') open = false
  }
</script>

<svelte:window {onpointerdown} {onkeydown} />

<div class="wrap" bind:this={box}>
  <IconButton icon="width" label="Reading width" active={open} onclick={() => (open = !open)} />
  {#if open}
    <div class="pop" role="dialog" aria-label="Reading width">
      <div class="head">
        <span>Reading width</span>
        <span class="value">{width === 0 ? 'Full' : `${width}px`}</span>
      </div>
      <div class="presets">
        {#each READING_WIDTH_PRESETS as p (p.label)}
          <button type="button" class:selected={width === p.width} onclick={() => settings.setUi({ readingWidth: p.width })}>{p.label}</button>
        {/each}
      </div>
      <input
        type="range"
        min={READING_WIDTH_MIN}
        max={READING_WIDTH_MAX}
        step="20"
        value={width === 0 ? lastFixed : width}
        aria-label="Reading width in pixels"
        oninput={(e) => settings.setUi({ readingWidth: Number(e.currentTarget.value) })}
      />
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 20;
    width: 260px;
    padding: 10px 12px 12px;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) + 2px);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
  .head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 8px;
    font-weight: 600;
  }
  .value {
    font-weight: 400;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
    margin-bottom: 10px;
  }
  .presets button {
    height: 26px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text-muted);
    font-size: 0.85em;
    cursor: pointer;
  }
  .presets button:hover {
    color: var(--text);
  }
  .presets button.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  input {
    width: 100%;
    accent-color: var(--accent);
  }
</style>
