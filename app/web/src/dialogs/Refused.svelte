<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The server refused: log in, or trust its certificate or host key. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { cancelers, dialogs } from '../lib/dialogs.svelte'
  import { refused } from '../lib/refused.svelte'
  import { invoke } from '../lib/tauri'

  const t = refused.tab
  const r = t?.refused
  const trust = !!r?.fingerprint
  const changed = trust && /changed/i.test(r?.message ?? '')
  const site = app.sites.find((s) => s.id === t?.saved_id)
  let user = $state(site?.user ?? '')
  let pass = $state('')
  let save = $state(true)

  const title = !r
    ? ''
    : trust
      ? changed
        ? 'The host key changed'
        : r.protocol === 'sftp'
          ? 'New SSH server'
          : 'Certificate not trusted'
      : `Log in to ${r.host}`

  const close = () => (dialogs.open = null)
  cancelers.refused = close

  function ok() {
    if (!t || !r) return
    close()
    t.refused = null
    if (trust) void invoke('trust', { tab: t.id, fingerprint: r.fingerprint })
    else void invoke('login', { tab: t.id, user, pass, save: save && !!t.saved_id })
  }

  function focusFirst(node: HTMLElement) {
    node.focus()
  }
</script>

{#if t && r}
  <div id="refusedDlg" class="modal">
    <div
      class="dialog"
      style:width="480px"
      role="dialog"
      aria-modal="true"
      aria-labelledby="rfTitle"
    >
      <h2 id="rfTitle">{title}</h2>
      <p id="rfText">
        {trust
          ? r.message +
            (r.protocol === 'sftp'
              ? '\n\nIts host key fingerprint:'
              : '\n\nIts certificate fingerprint (SHA-256):')
          : r.message}
      </p>
      {#if trust}
        <p id="rfFinger" class="mono">{r.fingerprint}</p>
      {:else}
        <div class="form">
          <label for="rfUser">User</label><input id="rfUser" spellcheck="false" bind:value={user} />
          <label for="rfPass">{site?.key ? 'Passphrase' : 'Password'}</label>
          <input
            id="rfPass"
            type="password"
            bind:value={pass}
            use:focusFirst
            onkeydown={(ev) => ev.key === 'Enter' && ok()}
          />
        </div>
        {#if t.saved_id}
          <label class="check"
            ><input id="rfSave" type="checkbox" bind:checked={save} /> Save it with the site</label
          >
        {/if}
      {/if}
      <div class="buttons">
        <div class="grow"></div>
        {#if trust}
          <button id="rfCancel" class="pill" use:focusFirst onclick={close}>Cancel</button>
        {:else}
          <button id="rfCancel" class="pill" onclick={close}>Cancel</button>
        {/if}
        <button id="rfOk" class="pill accent" class:danger={changed} onclick={ok}
          >{trust ? (changed ? 'Trust the new key' : 'Trust and connect') : 'Connect'}</button
        >
      </div>
    </div>
  </div>
{/if}
