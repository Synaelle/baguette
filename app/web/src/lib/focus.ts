// SPDX-License-Identifier: AGPL-3.0-or-later
// The two lists, so one side can hand the keyboard to the other.

import { app } from './app.svelte'
import type { Side } from './paths'

const lists: Record<Side, HTMLElement | null> = { local: null, remote: null }

export function registerList(side: Side, el: HTMLElement | null): void {
  lists[side] = el
}

export function focusSide(side: Side): void {
  lists[side]?.focus()
  app.activeSide = side
}

/** back to the list in use, after a dialog or the menu */
export function focusActive(): void {
  lists[app.activeSide]?.focus()
}
