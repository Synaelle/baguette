// SPDX-License-Identifier: AGPL-3.0-or-later
// Send to PC's settings while they are edited: kept while the 3DS picker is
// open over them, saved with the dialog's Save.

import { app } from './app.svelte'
import { dialogs } from './dialogs.svelte'

export const sendDraft = $state({ dir: '', items: [] as string[], replace: false })

export function openSendSettings(): void {
  sendDraft.dir = app.send.send_dir
  sendDraft.items = [...app.send.send_items]
  sendDraft.replace = app.send.send_replace
  dialogs.open = 'send'
}

export function addSendItem(p: string): void {
  if (!sendDraft.items.includes(p)) sendDraft.items.push(p)
}
