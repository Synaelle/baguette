// SPDX-License-Identifier: AGPL-3.0-or-later
// A refused connection: a login to give, or a certificate or host key to trust.

import type { Tab } from './app.svelte'
import { dialogs } from './dialogs.svelte'

export const refused = $state({ tab: null as Tab | null })

export function showRefused(t: Tab): void {
  if (!t.refused) return
  refused.tab = t
  dialogs.open = 'refused'
}
