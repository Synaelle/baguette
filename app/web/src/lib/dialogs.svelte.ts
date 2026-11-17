// SPDX-License-Identifier: AGPL-3.0-or-later
// Baguette's own yes/no and message box (the dialog plugin's confirm is async,
// so `if (confirm(...))` would always pass), and which dialog is open.

export type DialogName =
  'site' | 'prefs' | 'send' | 'picker' | 'sendback' | 'refused' | 'cementLogin'

interface Question {
  title: string
  text: string
  yes: string
  no: string
  resolve: (ok: boolean) => void
}

export const dialogs = $state({
  open: null as DialogName | null,
  /** the yes/no box, over any open dialog */
  question: null as Question | null,
})

/** Asks a yes/no question; true for yes. `no` empty: a message with one button. */
export function ask(text: string, yes = 'Yes', no = 'No', title = 'Baguette'): Promise<boolean> {
  return new Promise((resolve) => {
    dialogs.question = { title, text: String(text), yes, no, resolve }
  })
}

/** Shows a message, an error from Rust usually. */
export const tell = (text: unknown): Promise<boolean> => ask(String(text), 'OK', '')

export function answer(ok: boolean): void {
  const q = dialogs.question
  dialogs.question = null
  q?.resolve(ok)
}

export const anyModal = (): boolean => dialogs.open !== null || dialogs.question !== null

/** what Escape or Cancel does in each dialog, set by the dialog itself */
export const cancelers: Partial<Record<DialogName, () => void>> = {}

/** Escape: the yes/no box first (its No, or its one button), then the dialog open. */
export function escape(): void {
  const q = dialogs.question
  if (q) return answer(!q.no)
  const open = dialogs.open
  if (!open) return
  const cancel = cancelers[open]
  if (cancel) cancel()
  else dialogs.open = null
}
