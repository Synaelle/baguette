<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The saved sites and Quick connect. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { siteAddress } from '../lib/format'
  import { editSite } from '../lib/siteEditor.svelte'
  import type { Site } from '../lib/types'
  import Icon from './Icon.svelte'
  import StatusDot from './StatusDot.svelte'

  let quick = $state('')

  function open(s: Site) {
    const t = app.tabOfSite(s.id)
    if (t) app.activate(t.id)
    else void app.openSite(s.id)
  }

  async function quickConnect(ev: KeyboardEvent) {
    if (ev.key !== 'Enter' || !quick.trim()) return
    if (await app.openUrl(quick.trim())) quick = ''
  }
</script>

<nav id="sidebar" aria-label="Sites">
  <div class="side-head">
    <span class="h">Sites</span>
    <button id="btnNewSite" class="pill accent small" onclick={() => editSite(null)}>+ New</button>
  </div>
  <div id="siteList">
    {#each app.sites as s (s.id)}
      {@const t = app.tabOfSite(s.id)}
      {@const open_ = !!t && t.id === app.active}
      <div class="site" class:open={open_}>
        <button class="pick" aria-current={open_ ? 'true' : undefined} onclick={() => open(s)}>
          <StatusDot tab={t} />
          <span class="txt"
            ><b>{s.name}</b><span class="sub">{siteAddress(s.protocol, s.host, s.port)}</span></span
          >
          {#if t?.cement}<span class="tag">3DS</span>{/if}
        </button>
        <button class="edit" title="Edit" aria-label="Edit {s.name}" onclick={() => editSite(s)}
          ><Icon name="edit" /></button
        >
      </div>
    {/each}
  </div>
  <div class="grow"></div>
  <div class="quick">
    <label class="h" for="qc">Quick connect</label>
    <input
      id="qc"
      spellcheck="false"
      placeholder="sftp://pi@pi.local"
      bind:value={quick}
      onkeydown={quickConnect}
    />
  </div>
</nav>
