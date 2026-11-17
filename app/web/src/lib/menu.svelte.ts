// SPDX-License-Identifier: AGPL-3.0-or-later
// The right-click menu of a side's list, also opened with the Menu key or Shift+F10.

import { app } from './app.svelte'
import { tell } from './dialogs.svelte'
import type { Side } from './paths'
import { invoke } from './tauri'

export interface MenuItem {
  label: string
  key: string
  run: () => void
  off?: boolean
}

export const menu = $state({
  side: null as Side | null,
  x: 0,
  y: 0,
  /** null between groups */
  items: [] as (MenuItem | null)[],
  fromKeyboard: false,
})

export function showMenu(side: Side, x: number, y: number, fromKeyboard = false): void {
  const st = app.pane(side)
  const sel = app.selectedEntries(side)
  const items: (MenuItem | null)[] = []
  const local = side === 'local'
  const first = sel[0]
  if (sel.length === 1 && first)
    items.push({ label: 'Open', key: 'Enter', run: () => void app.openEntry(side, first) })
  if (sel.length && !(local && st.path === '')) {
    if (local && app.tab) items.push({ label: 'Upload', key: '', run: () => app.upload() })
    if (
      !local &&
      first &&
      sel.length === 1 &&
      !first.dir &&
      /\.cia$/i.test(first.name) &&
      app.tab?.cement
    )
      items.push({
        label: 'Install on the 3DS',
        key: '',
        run: () =>
          invoke('install_cia', {
            tab: app.active,
            path: app.join('remote', st.path, first.name),
          }).catch((e) => void tell(e)),
      })
    if (!local) {
      items.push({ label: 'Download', key: '', run: () => void app.download() })
      items.push({
        label: 'Download to...',
        key: '',
        run: () => void app.download(undefined, true),
      })
    }
    items.push({
      label: 'Rename',
      key: 'F2',
      run: () => app.startRename(side),
      off: sel.length !== 1,
    })
    items.push({ label: 'Delete', key: 'Del', run: () => void app.deleteSelected(side) })
    items.push(null)
  }
  if (!(local && st.path === ''))
    items.push({ label: 'New folder', key: '', run: () => app.newFolder(side) })
  items.push({ label: 'Refresh', key: 'F5', run: () => app.refresh(side) })
  Object.assign(menu, { side, x, y, items, fromKeyboard })
}

export function hideMenu(): void {
  menu.side = null
}
