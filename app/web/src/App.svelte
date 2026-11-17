<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Baguette's window. -->
<script lang="ts">
  import { app } from './lib/app.svelte'
  import { anyModal, dialogs, escape } from './lib/dialogs.svelte'
  import CementCard from './components/CementCard.svelte'
  import ContextMenu from './components/ContextMenu.svelte'
  import Empty from './components/Empty.svelte'
  import FilePane from './components/FilePane.svelte'
  import Queue from './components/Queue.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import StatusBar from './components/StatusBar.svelte'
  import Tabs from './components/Tabs.svelte'
  import Toolbar from './components/Toolbar.svelte'
  import TitleBar from './components/TitleBar.svelte'
  import Ask from './dialogs/Ask.svelte'
  import CementLogin from './dialogs/CementLogin.svelte'
  import Picker from './dialogs/Picker.svelte'
  import Prefs from './dialogs/Prefs.svelte'
  import Refused from './dialogs/Refused.svelte'
  import SendBack from './dialogs/SendBack.svelte'
  import SendSettings from './dialogs/SendSettings.svelte'
  import SiteEditor from './dialogs/SiteEditor.svelte'
  import { focusSide } from './lib/focus'
  import { invoke } from './lib/tauri'

  $effect(() => {
    document.body.classList.toggle('notab', !app.tab)
  })

  // the PC side takes the keyboard once the window is up
  $effect(() => {
    if (app.ready) focusSide('local')
  })

  function key(ev: KeyboardEvent) {
    if (ev.key === 'Escape') return escape()
    // no browser shortcuts (reload, print, find...)
    if (ev.key === 'F5' || (ev.ctrlKey && ['r', 'p', 'f', 'g', 'u', 'j'].includes(ev.key)))
      ev.preventDefault()
    if (ev.ctrlKey && ev.key === 's' && !anyModal()) {
      ev.preventDefault()
      if (app.cement && app.sendToPc) void invoke('send_to_pc', { tab: app.active })
    }
    if (ev.ctrlKey && ev.key === 'w' && !anyModal() && app.active !== null) {
      ev.preventDefault()
      void app.closeTab(app.active)
    }
  }
</script>

<svelte:document
  onkeydown={key}
  oncontextmenu={(ev) => (ev.target as HTMLElement).tagName !== 'INPUT' && ev.preventDefault()}
/>

<div class="blob b1" aria-hidden="true"></div>
<div class="blob b2" aria-hidden="true"></div>

<TitleBar />

{#if app.ready}
  <div id="body">
    <Sidebar />
    <main id="main">
      <Tabs />
      <div id="work" class:notabs={!app.tabs.length}>
        {#if !app.tabs.length}<Empty />{/if}
        <Toolbar />
        <div id="panes">
          <FilePane side="local" />
          <FilePane side="remote" />
        </div>
        <Queue />
      </div>
      <StatusBar />
    </main>
    <CementCard />
  </div>
{/if}

<ContextMenu />

{#if dialogs.open === 'site'}<SiteEditor />
{:else if dialogs.open === 'prefs'}<Prefs />
{:else if dialogs.open === 'send'}<SendSettings />
{:else if dialogs.open === 'picker'}<Picker />
{:else if dialogs.open === 'sendback'}<SendBack />
{:else if dialogs.open === 'refused'}<Refused />
{:else if dialogs.open === 'cementLogin'}<CementLogin />
{/if}
<Ask />
