<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The title bar: the window is undecorated, so it carries the window buttons. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { ask, dialogs, tell } from '../lib/dialogs.svelte'
  import { appWindow, invoke } from '../lib/tauri'
  import Icon from './Icon.svelte'

  const shortVersion = $derived(app.version.replace(/-[0-9a-f]+-dev$/, ''))

  async function update() {
    const u = app.update
    if (!u) return
    const have = `Baguette ${u.version} is available (you have ${shortVersion}).`
    if (!u.install) {
      const go = await ask(
        `${have}\n\nThis opens its release page on GitHub in your browser.`,
        'Open GitHub',
        'Not now',
        'New version',
      )
      if (go) invoke('open_release', { url: u.url }).catch((e) => void tell(e))
      return
    }
    const go = await ask(
      `${have}\n\nIt downloads from GitHub, replaces this Baguette and starts again. Transfers in progress stop.`,
      'Update',
      'Not now',
      'New version',
    )
    if (!go) return
    app.updating = true
    invoke('install_update').catch((e) => {
      app.updating = false
      void tell(e)
    })
  }
</script>

<header id="titlebar" data-tauri-drag-region>
  <Icon name="logo" />
  <span class="brand" data-tauri-drag-region>baguette</span>
  {#if app.version}
    <span
      id="version"
      class="chip"
      data-tauri-drag-region
      title="Baguette v{app.version} - Cement protocol {app.protocol}">{shortVersion}</span
    >
  {/if}
  {#if app.update}
    <button
      id="updateChip"
      class="chip update"
      title="A new Baguette is on GitHub"
      disabled={app.updating}
      onclick={update}>{app.updating ? 'Updating...' : `${app.update.version} is out`}</button
    >
  {/if}
  <div class="grow" data-tauri-drag-region></div>
  <button
    id="btnSettings"
    class="win"
    aria-label="Settings"
    title="Settings"
    onclick={() => (dialogs.open = 'prefs')}><Icon name="settings" /></button
  >
  <button id="winMin" class="win" aria-label="Minimize" onclick={() => appWindow.minimize()}
    ><Icon name="minimize" /></button
  >
  <button id="winMax" class="win" aria-label="Maximize" onclick={() => appWindow.toggleMaximize()}
    ><Icon name="maximize" /></button
  >
  <button id="winClose" class="win close" aria-label="Close" onclick={() => appWindow.close()}
    ><Icon name="close" /></button
  >
</header>
