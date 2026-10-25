<script lang="ts">
  /**
   * A thumbnail of goo (sidebar tree, a note with a heading, text, code and a diagram) drawn with a theme's own
   * tokens: the element carries `data-theme` (and `data-accent`), so every var() below resolves to that palette, not the
   * active one.
   */
  interface Props {
    theme: string
    /** null = the theme's own accent */
    accent?: string | null
  }

  let { theme, accent = null }: Props = $props()

  // [token, width] segments of the code line
  const code: [string, number][] = [['syn-keyword', 5], ['syn-attr', 6], ['syn-punct', 1], ['syn-string', 7]]
</script>

<span class="preview" data-theme={theme} data-accent={accent ?? undefined} aria-hidden="true">
  <span class="side">
    <span class="bar muted" style:width="60%"></span>
    <span class="row selected"><span class="bar text" style:flex="1"></span></span>
    <span class="row"><span class="bar faint" style:width="65%"></span></span>
    <span class="row"><span class="bar faint" style:width="50%"></span></span>
  </span>
  <span class="page">
    <span class="bar heading" style:width="55%"></span>
    <span class="bar muted" style:width="92%"></span>
    <span class="bar muted" style:width="70%"></span>
    <span class="codeline">
      {#each code as [token, w], i (i)}<span class="bar" style:width="{w * 2.6}px" style:background="var(--{token})"></span>{/each}
    </span>
    <span class="diagram">
      <span class="box"></span><span class="arrow"></span><span class="box solid"></span>
    </span>
  </span>
</span>

<style>
  .preview {
    display: flex;
    width: 100%;
    height: 4.6rem;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 28%;
    padding: 4px;
    border-right: 1px solid var(--border);
    background: var(--bg);
  }
  .page {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
    padding: 5px 6px;
    background: var(--surface);
  }
  .bar {
    display: block;
    height: 3px;
    border-radius: 2px;
  }
  .bar.muted {
    background: var(--text-muted);
    opacity: 0.7;
  }
  .bar.faint {
    background: var(--text-faint);
  }
  .bar.text {
    background: var(--text);
  }
  .bar.heading {
    height: 5px;
    background: var(--syn-keyword);
  }
  .row {
    display: flex;
    align-items: center;
    height: 8px;
    padding: 0 2px;
    border-radius: 2px;
  }
  .row.selected {
    background: var(--accent-soft);
  }
  .codeline {
    display: flex;
    gap: 2px;
    padding: 2px 3px;
    border-radius: 2px;
    background: var(--surface-raised);
  }
  .diagram {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: auto;
  }
  .box {
    width: 11px;
    height: 7px;
    border: 1px solid var(--accent);
    border-radius: 2px;
    background: var(--accent-soft);
  }
  .box.solid {
    background: var(--accent);
  }
  .arrow {
    width: 8px;
    height: 1px;
    background: var(--text-muted);
  }
</style>
