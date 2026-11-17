<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The tab's state and its last message (which can be copied). -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import StatusDot from './StatusDot.svelte'

  const c = $derived(app.cement)
</script>

<footer id="statusbar">
  {#if app.tab}<StatusDot tab={app.tab} />{/if}
  <span id="st0">{app.tab ? app.tab.status.part0 : 'Not connected'}</span>
  <span id="st1" class="dim">{app.tab ? app.tab.status.part1 : app.message}</span>
  {#if app.lite && c?.sd_free}
    <span id="stSd" class="dim">SD card: {c.sd_free} free of {c.sd_total}</span>
  {/if}
</footer>
