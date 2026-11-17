// SPDX-License-Identifier: AGPL-3.0-or-later
// Send to PC > Add from 3DS: a small browser over the tab's own connection.

import { app } from './app.svelte'
import { byName } from './format'
import { invoke } from './tauri'
import type { Entry, Listing } from './types'

export const picker = $state({
  tab: null as number | null,
  cwd: '/',
  entries: [] as Entry[],
  sel: [] as number[],
})

export function pickNavigate(path: string): void {
  Object.assign(picker, { tab: app.active, cwd: path, entries: [], sel: [] })
  void invoke('picker_list', { tab: app.active, path })
}

/** a listing the picker asked for (notify 1) */
export function pickerListed(r: Listing): void {
  if (r.tab !== picker.tab || r.path !== picker.cwd) return
  picker.entries = [...r.entries].sort(byName)
}
