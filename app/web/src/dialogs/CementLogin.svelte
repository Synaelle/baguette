<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Cement's own login: the user name and password it lets in, saved with the site. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { cancelers, dialogs, tell } from '../lib/dialogs.svelte'
  import { invoke } from '../lib/tauri'

  const site = app.sites.find((s) => s.id === app.tab?.saved_id)
  let user = $state(site?.user ?? '')
  let pass = $state('')

  const close = () => (dialogs.open = null)
  cancelers.cementLogin = close

  function set(u: string, p: string) {
    close()
    invoke('set_cement_login', { tab: app.active, user: u, pass: p }).catch((e) => void tell(e))
  }
</script>

<div id="cementLogin" class="modal">
  <div class="dialog" style:width="420px" role="dialog" aria-modal="true" aria-labelledby="clTitle">
    <h2 id="clTitle">3DS login</h2>
    <div class="form">
      <label for="clUser">User</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="clUser" spellcheck="false" bind:value={user} autofocus />
      <label for="clPass">Password</label>
      <input
        id="clPass"
        type="password"
        bind:value={pass}
        onkeydown={(ev) => ev.key === 'Enter' && set(user, pass)}
      />
    </div>
    <div class="buttons">
      {#if app.cement?.login}<button
          id="clRemove"
          class="pill ghost danger"
          onclick={() => set('', '')}>No login</button
        >{/if}
      <div class="grow"></div>
      <button id="clCancel" class="pill" onclick={close}>Cancel</button>
      <button id="clSave" class="pill accent" onclick={() => set(user, pass)}>Save</button>
    </div>
  </div>
</div>
