<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- A tab's connection state: grey off, pulsing while it connects, green on, red on Cement. -->
<script lang="ts">
  import type { Tab } from '../lib/app.svelte'

  let { tab }: { tab: Tab | null | undefined } = $props()

  const state = $derived.by(() => {
    if (!tab) return { cls: 'off', label: 'Not open' }
    if (tab.state === 'connected')
      return tab.cement
        ? { cls: 'cement', label: 'Connected to Cement' }
        : { cls: 'on', label: 'Connected' }
    if (tab.state === 'refused') return { cls: 'off', label: 'Refused' }
    return { cls: 'wait', label: 'Connecting' }
  })
</script>

<span class="dot {state.cls}" role="img" aria-label={state.label} title={state.label}></span>
