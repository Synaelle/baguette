<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- One tab per open site. Middle-click or Ctrl+W closes one. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import Icon from './Icon.svelte'
  import StatusDot from './StatusDot.svelte'
</script>

<div id="tabs">
  {#each app.tabs as t (t.id)}
    {@const on = t.id === app.active}
    <div
      class="tab"
      class:active={on}
      onauxclick={(ev) => ev.button === 1 && void app.closeTab(t.id)}
      role="presentation"
    >
      <button
        class="pick"
        aria-current={on ? 'true' : undefined}
        onclick={() => app.activate(t.id)}
      >
        <StatusDot tab={t} /><span>{t.name}</span>
      </button>
      <button
        class="x"
        title="Close (Ctrl+W)"
        aria-label="Close {t.name}"
        onclick={() => void app.closeTab(t.id)}><Icon name="tabClose" /></button
      >
    </div>
  {/each}
</div>
