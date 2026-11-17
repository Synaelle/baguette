// SPDX-License-Identifier: AGPL-3.0-or-later
// What the Rust side tells the window.

import { app } from './app.svelte'
import { applyLook } from './colour'
import { listenForDrops } from './drag.svelte'
import { pickerListed } from './picker.svelte'
import { showRefused } from './refused.svelte'
import { askSendBack } from './sendBack.svelte'
import { openSendSettings } from './sendDraft.svelte'
import { invoke, on } from './tauri'
import type { Cement, ConnState, Listing, Progress, Refused, SendBack, Site, Status } from './types'

export function listenToRust(): void {
  on<Status>('status', (s) => {
    const t = app.tabs.find((x) => x.id === s.tab)
    if (t) t.status = { part0: s.part0, part1: s.part1 }
  })
  on<Progress & { tab: number }>('progress', (p) => {
    const t = app.tabs.find((x) => x.id === p.tab)
    if (!t) return
    const ended = t.progress.active && !p.active
    t.progress = p
    if (ended && app.local.path) app.refresh('local') // downloads land on the PC side
  })
  on<{ tab: number; state: ConnState; cement: Cement | null }>('tab-state', (s) => {
    const t = app.tabs.find((x) => x.id === s.tab)
    if (!t) return
    const was = t.state
    t.state = s.state
    t.cement = s.cement
    if (s.state === 'connected') {
      t.refused = null
      if (was !== 'connected' && t.id === app.active && t.remote.shown === null && t.remote.path)
        void app.navigate('remote', t.remote.path)
    }
  })
  on<Listing>('list', (r) => (r.notify === 1 ? pickerListed(r) : app.listed(r)))
  on<Refused>('refused', (r) => {
    const t = app.tabs.find((x) => x.id === r.tab)
    if (!t) return
    t.refused = r
    if (r.tab !== app.active) app.activate(r.tab)
    showRefused(t)
  })
  on<SendBack>('sendback', askSendBack)
  // the colour was changed on a 3DS after this one
  on<string>('accent', (hex) => {
    app.accent = hex
    applyLook(app.look, hex)
  })
  on('sites-changed', async () => {
    app.sites = await invoke<Site[]>('sites_list')
  })
  on('open-send-settings', openSendSettings)
  listenForDrops()
}
