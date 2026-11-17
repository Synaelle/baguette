<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The transfer running on the tab on screen. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { invoke } from '../lib/tauri'

  const p = $derived(app.tab?.progress)
</script>

{#if app.tab && p?.active}
  <div id="queue">
    <div class="q-line">
      <span class="h">Queue</span>
      <span id="qName" class="q-name"
        >{p.name ? (p.up ? '↑ ' : '↓ ') + p.name : app.tab.status.part1}</span
      >
      <span id="qDetail" class="dim">{p.detail}</span>
      <button
        id="btnCancel"
        class="pill small"
        onclick={() => invoke('cancel', { tab: app.active })}>Cancel</button
      >
    </div>
    <div
      class="bar"
      role="progressbar"
      aria-label="Transfer"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={Math.round(p.permille / 10)}
    >
      <div id="qFill" style:width="{p.permille / 10}%"></div>
    </div>
  </div>
{/if}
