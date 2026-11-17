<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The 3DS tools, when the tab is on Cement: battery, SD card, Send to PC,
     its login, updating it, and the CIA files waiting to be installed. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { ask, dialogs, tell } from '../lib/dialogs.svelte'
  import { openSendSettings } from '../lib/sendDraft.svelte'
  import { invoke } from '../lib/tauri'

  const c = $derived(app.cement)
  let updating = $state(false)

  /** what this Cement's protocol means next to this Baguette */
  const hint = $derived.by(() => {
    if (!c) return ''
    if (c.protocol < app.protocol) return 'Update Cement to use everything this Baguette can do'
    if (c.protocol > app.protocol)
      return 'This Cement is newer: update Baguette for its new features'
    return ''
  })

  async function updateCement() {
    if (!c) return
    const go = await ask(
      `Cement ${c.update} downloads from GitHub, goes to /cias on the 3DS and installs there. Restart Cement on the 3DS once it's installed.`,
      'Update',
      'Not now',
      'Update Cement',
    )
    if (!go) return
    updating = true
    invoke('update_cement', { tab: app.active })
      .catch((e) => void tell(e))
      .finally(() => (updating = false))
  }
</script>

{#if c}
  <aside id="cement" aria-label="3DS tools">
    <div class="card strong">
      <div class="card-title">
        <svg class="bite" viewBox="0 0 12 12" aria-hidden="true"
          ><circle cx="6" cy="6" r="5.5" fill="var(--accent)" /><circle
            cx="11"
            cy="2"
            r="2.6"
            fill="var(--surface)"
          /></svg
        >Cement
      </div>
      {#if c.battery}
        <div>
          <div class="kv">
            <span>Battery</span><span id="cBattery" class:low={c.low}
              >{c.battery}{c.charging ? ', charging' : c.plugged ? ', plugged in' : ''}</span
            >
          </div>
          <div class="bar thin">
            <div class="soft" style:width="{Math.max(0, c.battery_percent)}%"></div>
          </div>
        </div>
      {/if}
      {#if c.sd_free}
        <div>
          <div class="kv"><span>SD card</span><span id="cSd">{c.sd_free} free</span></div>
          <div class="bar thin"><div style:width="{c.sd_permille / 10}%"></div></div>
          <div id="cSdTotal" class="dim small">of {c.sd_total}</div>
        </div>
      {/if}
      {#if app.sendToPc}
        <button
          id="btnSend"
          class="pill accent wide"
          title="Send to PC (Ctrl+S)"
          onclick={() => invoke('send_to_pc', { tab: app.active })}>Send to PC</button
        >
        <button id="btnSendSettings" class="link" onclick={openSendSettings}
          >Choose what to send...</button
        >
      {/if}
      <div class="kv">
        <span>Login</span><button
          id="btnCementLogin"
          class="link"
          onclick={() => (dialogs.open = 'cementLogin')}
          >{c.login ? 'On, change...' : 'Off, set one...'}</button
        >
      </div>
      {#if c.update}
        <button
          id="btnCementUpdate"
          class="pill accent wide"
          disabled={updating}
          onclick={updateCement}
          >{updating ? 'Updating Cement...' : `Update Cement to ${c.update}`}</button
        >
      {/if}
      <div id="cVersion" class="card-foot">
        {c.version ? `Cement ${c.version}` : 'Cement'}, protocol {c.protocol}
        {#if hint}<span class="hint">{hint}</span>{/if}
      </div>
    </div>
    <div class="card">
      <div class="card-title light">CIAs on the 3DS</div>
      {#if c.installing}
        <div>
          <div class="kv"><span>Installing</span><span>{c.installing.name}</span></div>
          <div class="bar thin"><div style:width="{c.installing.permille / 10}%"></div></div>
        </div>
      {/if}
      <div id="cCias">
        {#each c.cias as x (x.path)}
          <div class="cia">
            <span title={x.path}>{x.name}</span><span class="size">{x.size}</span>
            <button
              class="pill accent"
              disabled={!!c.installing}
              onclick={() =>
                invoke('install_cia', { tab: app.active, path: x.path }).catch((e) => void tell(e))}
              >Install</button
            >
          </div>
        {:else}
          <div class="dim">None on the 3DS</div>
        {/each}
      </div>
      <div class="dim small">Or press A on the 3DS</div>
    </div>
  </aside>
{/if}
