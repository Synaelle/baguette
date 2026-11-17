<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- No tab open: the way to a site. Lite mode lists the saved sites here, with an address field. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { siteLine } from '../lib/format'
  import { editSite } from '../lib/siteEditor.svelte'
  import Icon from './Icon.svelte'
  import StatusDot from './StatusDot.svelte'

  let addr = $state('')

  async function go() {
    const url = addr.trim()
    if (!url) return
    app.closeAll()
    if (await app.openUrl(url)) addr = ''
  }
</script>

<div id="empty">
  {#if app.lite}
    <div class="empty-card lite-start">
      <Icon name="logoBig" />
      <h1>Where are your files?</h1>
      <div class="start-box">
        <div id="startSites">
          {#each app.sites as s (s.id)}
            <button
              class="start-site"
              onclick={() => {
                app.closeAll()
                void app.openSite(s.id)
              }}
            >
              <StatusDot tab={app.tabOfSite(s.id)} />
              <span class="txt"
                ><b>{s.name}</b><span class="sub">{siteLine(s.protocol, s.host)}</span></span
              >
              <span class="go">Open</span>
            </button>
          {/each}
        </div>
        <div class="start-addr">
          <label for="startAddr">Address</label>
          <input
            id="startAddr"
            spellcheck="false"
            placeholder="192.168.1.42:5000"
            bind:value={addr}
            onkeydown={(ev) => ev.key === 'Enter' && go()}
          />
          <button id="startGo" class="pill" onclick={go}>Connect</button>
        </div>
      </div>
      <p class="small">The sites saved in Advanced mode show up here</p>
    </div>
  {:else}
    <div class="empty-card">
      <Icon name="biteBig" />
      <h1>Pick a site to start</h1>
      <p>
        Open one from the list, add a new one, or type an address in Quick connect.<br />Connected
        to Cement, the 3DS tools show up on the right.
      </p>
      <button id="btnEmptyNew" class="pill accent" onclick={() => editSite(null)}>+ New site</button
      >
    </div>
  {/if}
</div>
