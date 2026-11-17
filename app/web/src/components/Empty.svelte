<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- No tab open: the way to a site. Lite mode lists the saved sites here, with an address field. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { siteAddress } from '../lib/format'
  import { editSite } from '../lib/siteEditor.svelte'
  import Mark from './Mark.svelte'
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
      <Mark size={76} />
      <h1>Servers</h1>
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
                ><b>{s.name}</b><span class="sub">{siteAddress(s.protocol, s.host, s.port)}</span
                ></span
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
    </div>
  {:else}
    <div class="empty-card">
      <Mark size={56} />
      <h1>No site open</h1>
      <p>Open a saved site, add one, or type an address in Quick connect.</p>
      <button id="btnEmptyNew" class="pill accent" onclick={() => editSite(null)}>+ New site</button
      >
    </div>
  {/if}
</div>
