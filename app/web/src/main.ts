// SPDX-License-Identifier: AGPL-3.0-or-later

import { mount } from 'svelte'
import App from './App.svelte'
import { app } from './lib/app.svelte'
import { ask } from './lib/dialogs.svelte'
import { listenToRust } from './lib/events'
import { appWindow, invoke } from './lib/tauri'
import './styles/app.css'

listenToRust()
mount(App, { target: document.getElementById('app') ?? document.body })
void app.start()

// a transfer stopped by closing could leave a cut file behind: ask, then let the
// transfers stop and clean up before the window goes
void appWindow.onCloseRequested(async (ev) => {
  if (!app.tabs.some((t) => t.progress.active)) return
  ev.preventDefault()
  const go = await ask('A transfer is still running. Closing Baguette stops it.', 'Close', 'Cancel')
  if (!go) return
  await invoke('close_all_tabs').catch(() => {})
  await appWindow.destroy()
})
