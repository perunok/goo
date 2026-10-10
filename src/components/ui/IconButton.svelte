<script lang="ts">
  import Icon from './Icon.svelte'
  import type { IconName } from './icons'

  interface Props {
    icon: IconName
    label: string
    onclick: (e: MouseEvent) => void
    active?: boolean
    size?: number
  }

  let { icon, label, onclick, active = false, size = 16 }: Props = $props()
</script>

<button type="button" class="icon-btn" class:active title={label} aria-label={label} aria-pressed={active} {onclick}>
  <Icon name={icon} {size} />
</button>

<style>
  .icon-btn {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .icon-btn.active {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
</style>
