<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- First launch: Standard or Lite, over the window below the title bar. -->
<script lang="ts">
  import liteImg from '../assets/preview-lite.png'
  import standardImg from '../assets/preview-standard.png'
  import { app } from '../lib/app.svelte'
  import { tell } from '../lib/dialogs.svelte'
  import { invoke } from '../lib/tauri'
  import type { Mode } from '../lib/types'

  let pick = $state<Mode>('standard')

  const cards: { mode: Mode; name: string; img: string; alt: string; text: string }[] = [
    {
      mode: 'standard',
      name: 'Standard',
      img: standardImg,
      alt: 'Standard: your PC and the server side by side',
      text: 'Your PC and the server side by side, saved sites, tabs, and the 3DS tools: battery, SD card, CIA installs.',
    },
    {
      mode: 'lite',
      name: 'Lite',
      img: liteImg,
      alt: "Lite: one window with the server's files",
      text: 'Just the files on your 3DS or server. Drag files in to send them, drag them out to save them.',
    },
  ]

  async function start() {
    await invoke('set_mode', { mode: pick }).catch((e) => void tell(e))
    app.setup = false
    app.setMode(pick)
  }
</script>

<div id="setup" class="modal">
  <div class="setup-in" role="dialog" aria-modal="true" aria-labelledby="setupTitle">
    <h1 id="setupTitle">Welcome to Baguette</h1>
    <p>How do you want it? You can switch any time in the settings.</p>
    <div class="setup-cards">
      {#each cards as c (c.mode)}
        <button
          class="setup-card"
          data-mode={c.mode}
          aria-pressed={pick === c.mode}
          onclick={() => (pick = c.mode)}
        >
          <img src={c.img} alt={c.alt} />
          <span class="setup-name"><span class="radio" aria-hidden="true"></span>{c.name}</span>
          <span class="setup-text">{c.text}</span>
        </button>
      {/each}
    </div>
    <button id="setupGo" class="pill accent" onclick={start}
      >Start with {pick === 'lite' ? 'Lite' : 'Standard'}</button
    >
  </div>
</div>
