// SPDX-License-Identifier: AGPL-3.0-or-later
// A file opened from the server was saved on the PC: send it back or not.

import { dialogs } from './dialogs.svelte'
import type { SendBack } from './types'

export const sendBack = $state({ data: null as SendBack | null })

export function askSendBack(d: SendBack): void {
  sendBack.data = d
  dialogs.open = 'sendback'
}
