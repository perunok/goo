<script lang="ts">
  /** Settings > goo cloud: sign in with a code approved in the browser, link the open folder, see sync problems. */
  import { backend } from '../../app/backend'
  import { ui } from '../../app/ui.svelte'
  import { workspace } from '../../app/workspace.svelte'
  import { errorMessage, type CloudWorkspace } from '../../backend/types'
  import Button from '../../components/ui/Button.svelte'
  import { copyText } from '../../lib/clipboard'
  import { relativeTo } from '../../lib/paths'
  import { ago, cloud } from './cloud.svelte'

  let serverUrl = $state('')
  let busy = $state(false)
  let remote = $state<CloudWorkspace[] | null>(null)
  let choice = $state('')
  let newName = $state('')

  const s = $derived(cloud.status)
  const link = $derived(cloud.current)
  const folder = $derived(workspace.workspace)

  $effect(() => {
    if (!serverUrl && s.account) serverUrl = s.account.serverUrl
  })

  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    busy = true
    try {
      return await action()
    } catch (e) {
      ui.toast(errorMessage(e), 'error')
      return undefined
    } finally {
      busy = false
    }
  }

  async function signIn(e: SubmitEvent) {
    e.preventDefault()
    const st = await run(() => backend.cloudSignIn(serverUrl))
    if (st) cloud.set(st)
  }

  async function loadRemote() {
    remote = (await run(() => backend.cloudWorkspaces())) ?? null
    choice = ''
    newName = folder?.name ?? ''
  }

  async function linkFolder() {
    const st = await run(() => backend.cloudLink(choice === 'new' ? { newName } : { workspaceId: choice }))
    if (st) {
      cloud.set(st)
      remote = null
    }
  }

  async function unlink(root: string) {
    const answer = await ui.confirm({
      title: 'Stop syncing this folder?',
      message: 'The notes stay in this folder and in goo cloud; changes just stop being exchanged.',
      actions: [
        { id: 'unlink', label: 'Stop syncing', kind: 'danger' },
        { id: 'cancel', label: 'Cancel' },
      ],
    })
    if (answer !== 'unlink') return
    const st = await run(() => backend.cloudUnlink(root))
    if (st) cloud.set(st)
  }

  async function signOut() {
    const answer = await ui.confirm({
      title: 'Sign out of goo cloud?',
      message: 'Every folder stops syncing on this computer. Your notes stay here and on the server.',
      actions: [
        { id: 'out', label: 'Sign out', kind: 'danger' },
        { id: 'cancel', label: 'Cancel' },
      ],
    })
    if (answer !== 'out') return
    const st = await run(() => backend.cloudSignOut())
    if (st) cloud.set(st)
  }

  const STATE_LABEL: Record<string, string> = {
    idle: 'Up to date',
    syncing: 'Syncing…',
    offline: 'Offline',
    error: 'Sync problem',
    signed_out: 'Signed out',
  }
  const rel = (p: string) => (folder ? relativeTo(folder.root, p) : p)
</script>

<section>
  <h3>goo cloud</h3>
  <p class="lead">Keep this folder in sync between your computers and share it with others, through your goo cloud server.</p>

  {#if !s.account}
    {#if s.signIn}
      <div class="code-box">
        <p>Approve this sign-in in your browser. The code is:</p>
        <div class="code">{s.signIn.userCode}</div>
        <div class="actions">
          <Button onclick={() => void backend.openExternal(s.signIn!.verificationUriComplete)}>Open the browser again</Button>
          <Button kind="ghost" onclick={() => void copyText(s.signIn!.verificationUri).then(() => ui.toast('Link copied'))}>Copy link</Button>
          <Button kind="ghost" onclick={async () => cloud.set(await backend.cloudCancelSignIn())}>Cancel</Button>
        </div>
      </div>
    {:else}
      <form class="signin" onsubmit={signIn}>
        <input bind:value={serverUrl} placeholder="https://goo.example.com" spellcheck="false" autocomplete="url" aria-label="Server address" />
        <Button kind="primary" type="submit" disabled={busy || !serverUrl.trim()}>Sign in</Button>
      </form>
      {#if s.signInError}<p class="bad">{s.signInError}</p>{/if}
    {/if}
  {:else}
    <div class="row">
      <span>Signed in as <strong>{s.account.email}</strong> on {s.account.serverUrl.replace(/^https?:\/\//, '')}</span>
      <Button kind="ghost" disabled={busy} onclick={signOut}>Sign out</Button>
    </div>

    {#if !folder}
      <p class="muted">Open a folder to sync it.</p>
    {:else if link}
      <div class="linked">
        <div class="row">
          <span>
            <strong>{folder.name}</strong> syncs with <strong>{link.workspaceName}</strong>
            {#if link.role === 'viewer'}<span class="tag">read-only</span>{/if}
          </span>
          <span class="state {link.status.state}">{STATE_LABEL[link.status.state] ?? link.status.state}</span>
        </div>
        <p class="muted small">Last synced {ago(link.status.lastSyncedAt)}{#if link.status.message} · {link.status.message}{/if}</p>
        {#if link.status.conflicts.length}
          <div class="issues">
            <p><strong>Edited on two devices.</strong> Both versions are kept; compare them and delete the copy you don't need:</p>
            <ul>
              {#each link.status.conflicts as c (c.conflict_path)}
                <li>
                  <button type="button" class="link" onclick={() => workspace.openFile(`${link.root}/${c.path}`)}>{c.path}</button>
                  and
                  <button type="button" class="link" onclick={() => workspace.openFile(`${link.root}/${c.conflict_path}`)}>{c.conflict_path}</button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        {#if link.status.quarantined.length}
          <div class="issues">
            <p><strong>Not synced:</strong></p>
            <ul>
              {#each link.status.quarantined as q (q.path)}<li>{q.path}: {q.message}</li>{/each}
            </ul>
          </div>
        {/if}
        {#if link.status.deferred.length}
          <p class="muted small">Waiting until you save: {link.status.deferred.map(rel).join(', ')}</p>
        {/if}
        {#if link.status.notUploaded.length}
          <p class="muted small">Read-only workspace; not uploaded: {link.status.notUploaded.join(', ')}</p>
        {/if}
        <div class="actions">
          <Button disabled={busy || link.status.state === 'syncing'} onclick={() => void backend.cloudSyncNow(link.root)}>Sync now</Button>
          <Button kind="ghost" disabled={busy} onclick={() => unlink(link.root)}>Stop syncing</Button>
        </div>
      </div>
    {:else if remote}
      <div class="linkform">
        <label>
          Sync <strong>{folder.name}</strong> with
          <select bind:value={choice}>
            <option value="" disabled>Choose…</option>
            <option value="new">A new workspace</option>
            {#each remote as w (w.id)}<option value={w.id}>{w.name} ({w.role})</option>{/each}
          </select>
        </label>
        {#if choice === 'new'}
          <input bind:value={newName} placeholder="Workspace name" aria-label="New workspace name" />
        {:else if choice}
          <p class="muted small">Notes from the workspace are added to this folder; files already here are kept (a same-named different file is renamed "(local)").</p>
        {/if}
        <div class="actions">
          <Button kind="primary" disabled={busy || !choice || (choice === 'new' && !newName.trim())} onclick={linkFolder}>Start syncing</Button>
          <Button kind="ghost" onclick={() => (remote = null)}>Cancel</Button>
        </div>
      </div>
    {:else}
      <div class="row">
        <span class="muted"><strong>{folder.name}</strong> is not synced.</span>
        <Button kind="primary" disabled={busy} onclick={loadRemote}>Sync this folder…</Button>
      </div>
    {/if}
  {/if}
</section>

<style>
  h3 {
    margin: 0 0 6px;
    font-size: 0.95em;
  }
  .lead {
    margin: 0 0 10px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }
  .signin {
    display: flex;
    gap: 8px;
  }
  .signin input {
    flex: 1;
  }
  .code-box {
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
  }
  .code-box p {
    margin: 0 0 6px;
  }
  .code {
    margin: 4px 0 10px;
    font-family: var(--font-mono);
    font-size: 1.6em;
    letter-spacing: 0.12em;
    color: var(--accent-text);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .linked,
  .linkform {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .linkform label {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .linkform input {
    width: 100%;
  }
  select {
    height: 28px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
  }
  .state {
    white-space: nowrap;
    color: var(--success);
  }
  .state.syncing {
    color: var(--accent-text);
  }
  .state.offline,
  .state.signed_out {
    color: var(--text-muted);
  }
  .state.error,
  .bad {
    color: var(--danger);
  }
  .tag {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 10px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 0.8em;
  }
  .issues {
    margin: 8px 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--warning-soft);
  }
  .issues p {
    margin: 0 0 4px;
  }
  .issues ul {
    margin: 0;
    padding-left: 1.2em;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    text-decoration: underline;
    cursor: pointer;
    font: inherit;
  }
  .muted {
    color: var(--text-faint);
  }
  .small {
    font-size: 0.88em;
    margin: 4px 0;
  }
</style>
