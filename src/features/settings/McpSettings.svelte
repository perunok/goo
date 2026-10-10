<script lang="ts">
  /** Settings > AI assistants (MCP): turn access on, connect Claude Code, copy configs for other assistants. */
  import { onMount } from 'svelte'
  import { backend } from '../../app/backend'
  import { ui } from '../../app/ui.svelte'
  import { errorMessage, type McpStatus } from '../../backend/types'
  import Button from '../../components/ui/Button.svelte'
  import { copyText } from '../../lib/clipboard'
  import { mcpSnippets } from './mcpSnippets'

  let status = $state<McpStatus | null>(null)
  let busy = $state(false)
  let shown = $state('json')

  onMount(() => {
    void run(() => backend.mcpStatus())
  })

  async function run(action: () => Promise<McpStatus>) {
    busy = true
    try {
      status = await action()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    } finally {
      busy = false
    }
  }

  async function copy(text: string, what: string) {
    try {
      await copyText(text)
      ui.toast(`${what} copied`)
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
    }
  }

  const snippets = $derived(status ? mcpSnippets(status) : [])
  const snippet = $derived(snippets.find((s) => s.id === shown) ?? snippets[0])
</script>

<section>
  <h3>AI assistants (MCP)</h3>
  <p class="lead">
    Let assistants such as Claude read, search and edit your notes. Only folders you opened in goo are reachable,
    notes with unsaved edits are never overwritten, and nothing is deleted. Changes show up here at once.
  </p>

  {#if status}
    <label class="check">
      <input type="checkbox" checked={status.enabled} disabled={busy} onchange={(e) => run(() => backend.mcpSetEnabled(e.currentTarget.checked))} />
      Allow AI assistants to use goo
    </label>

    {#if status.enabled}
      <p class="state" class:bad={!!status.error}>
        {#if status.error}
          Could not start: {status.error}
        {:else if status.running}
          Running on port {status.port}. Assistants start goo when it is closed.
        {:else}
          Starting…
        {/if}
      </p>

      <div class="row">
        <span class="label">Claude Code</span>
        {#if !status.claudeCode.found}
          <span class="muted">not found on this computer — use the command below</span>
        {:else if status.claudeCode.connected === 'yes'}
          <span class="ok">connected</span>
          <Button kind="ghost" disabled={busy} onclick={() => run(() => backend.mcpDisconnectClaudeCode())}>Disconnect</Button>
        {:else}
          {#if status.claudeCode.connected === 'outdated'}<span class="muted">set up for another copy of goo</span>{/if}
          <Button kind="primary" disabled={busy} onclick={() => run(() => backend.mcpConnectClaudeCode())}>
            {status.claudeCode.connected === 'outdated' ? 'Update' : 'Connect'}
          </Button>
        {/if}
      </div>

      <div class="snippets">
        <div class="tabs" role="tablist">
          {#each snippets as s (s.id)}
            <button type="button" role="tab" aria-selected={shown === s.id} class:active={shown === s.id} onclick={() => (shown = s.id)}>{s.label}</button>
          {/each}
        </div>
        {#if snippet}
          <p class="hint">{snippet.hint}</p>
          <pre><code>{snippet.text}</code></pre>
          <div class="actions">
            <Button onclick={() => copy(snippet.text, snippet.label)}>Copy</Button>
            {#if snippet.id === 'http'}
              <Button kind="ghost" disabled={busy} onclick={() => run(() => backend.mcpNewToken())}>New token</Button>
            {/if}
          </div>
        {/if}
      </div>
      {#if status.devBuild}
        <p class="muted small">Development build: assistants can only start goo while <code>npm run app:dev</code> is running.</p>
      {/if}
    {/if}
  {/if}
</section>

<style>
  h3 {
    margin: 0 0 6px;
    font-size: 0.95em;
  }
  .lead {
    margin: 0 0 8px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0;
  }
  .check input {
    accent-color: var(--accent);
  }
  .state {
    margin: 4px 0 10px;
    color: var(--success);
  }
  .state.bad {
    color: var(--danger);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
  }
  .label {
    font-weight: 600;
    min-width: 96px;
  }
  .ok {
    color: var(--success);
  }
  .muted {
    color: var(--text-faint);
  }
  .small {
    font-size: 0.85em;
  }
  .snippets {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--border);
    background: var(--surface-raised);
  }
  .tabs button {
    padding: 6px 10px;
    border: 0;
    background: none;
    color: var(--text-muted);
    font-size: 0.88em;
    cursor: pointer;
    box-shadow: inset 0 -2px 0 transparent;
  }
  .tabs button.active {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .hint {
    margin: 8px 10px 0;
    color: var(--text-muted);
    font-size: 0.88em;
  }
  pre {
    margin: 8px 10px;
    padding: 8px 10px;
    max-height: 180px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--bg);
    font-family: var(--font-mono);
    font-size: 0.85em;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: 8px;
    padding: 0 10px 10px;
  }
</style>
