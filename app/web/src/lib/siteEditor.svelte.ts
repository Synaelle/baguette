// SPDX-License-Identifier: AGPL-3.0-or-later
// The site editor's state, opened from the sidebar and the empty screen.

import { dialogs } from './dialogs.svelte'
import type { Site } from './types'

export const siteEditor = $state({ site: null as Site | null })

/** Opens the editor on a saved site, or on a new one (null). */
export function editSite(s: Site | null): void {
  siteEditor.site = s
  dialogs.open = 'site'
}
