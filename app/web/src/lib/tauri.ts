// SPDX-License-Identifier: AGPL-3.0-or-later
// The work happens in Rust: a command is invoked, events come back.

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'

export { invoke }

/** Calls `handler` with each payload of `event`, for as long as the window lives. */
export function on<T>(event: string, handler: (payload: T) => void): void {
  void listen<T>(event, (e) => handler(e.payload))
}

export const appWindow = getCurrentWindow()
export const webview = getCurrentWebview()
