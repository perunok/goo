<script lang="ts">
  /** Renders ui.confirm() and ui.prompt() requests. */
  import { ui } from '../../app/ui.svelte'
  import Button from './Button.svelte'
  import Dialog from './Dialog.svelte'

  let value = $state('')
  let input: HTMLInputElement | undefined = $state()

  $effect(() => {
    const req = ui.promptRequest
    if (!req) return
    value = req.value
    queueMicrotask(() => {
      input?.focus()
      input?.select()
    })
  })

  function submit(e: SubmitEvent) {
    e.preventDefault()
    const v = value.trim()
    if (v) ui.promptRequest?.resolve(v)
  }
</script>

{#if ui.confirmRequest}
  {@const req = ui.confirmRequest}
  <Dialog title={req.title} onClose={() => req.resolve(null)} width="420px">
    <p class="msg">{req.message}</p>
    {#snippet footer()}
      {#each [...req.actions].reverse() as a (a.id)}
        <Button kind={a.kind ?? 'default'} onclick={() => req.resolve(a.id)}>{a.label}</Button>
      {/each}
    {/snippet}
  </Dialog>
{/if}

{#if ui.promptRequest}
  {@const req = ui.promptRequest}
  <Dialog title={req.title} onClose={() => req.resolve(null)} width="420px">
    <form id="goo-prompt" onsubmit={submit}>
      <label>
        <span>{req.label}</span>
        <input bind:this={input} bind:value spellcheck="false" autocomplete="off" />
      </label>
    </form>
    {#snippet footer()}
      <Button onclick={() => req.resolve(null)}>Cancel</Button>
      <Button kind="primary" onclick={() => (value.trim() ? req.resolve(value.trim()) : undefined)}>{req.confirmLabel}</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .msg {
    margin: 0;
    color: var(--text-muted);
    line-height: 1.5;
  }
  label {
    display: grid;
    gap: 6px;
  }
  label span {
    color: var(--text-muted);
    font-size: 0.9em;
  }
</style>
