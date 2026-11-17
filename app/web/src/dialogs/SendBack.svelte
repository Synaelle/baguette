<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- An opened file was edited on the PC: send it back to the server, or not. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { cancelers, dialogs } from '../lib/dialogs.svelte'
  import { focusActive } from '../lib/focus'
  import { sendBack } from '../lib/sendBack.svelte'
  import { invoke } from '../lib/tauri'

  const d = sendBack.data
  const tab = app.tabs.find((t) => t.id === d?.tab)

  function done(send: boolean) {
    dialogs.open = null
    if (d) void invoke('send_back', { local: d.local, send })
    focusActive()
  }
  cancelers.sendback = () => done(false)
</script>

{#if d}
  <div id="sendback" class="modal">
    <div
      class="dialog"
      style:width="480px"
      role="dialog"
      aria-modal="true"
      aria-labelledby="sendbackTitle"
    >
      <h2 id="sendbackTitle">Edited file</h2>
      <p id="sbPath" class="mono">{tab ? `${tab.name}:  ` : ''}{d.remote}</p>
      <div class="grid">
        <span class="dim">On the server</span><span id="sbRemote">{d.remote_text}</span>
        <span class="dim">Your edited file</span><span id="sbLocal">{d.local_text}</span>
      </div>
      {#if d.warn}<p id="sbWarn" class="warn">{d.warn}</p>{/if}
      <div class="buttons">
        <div class="grow"></div>
        <button id="sbKeep" class="pill" onclick={() => done(false)}>Don't send</button>
        <button id="sbSend" class="pill accent" onclick={() => done(true)}>Send it back</button>
      </div>
    </div>
  </div>
{/if}
