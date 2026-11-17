// SPDX-License-Identifier: AGPL-3.0-or-later
// Moving files by dragging: between the two sides, out of the window to the
// file manager (server items), and in from it (onto the server side).

import { app } from './app.svelte'
import { anyModal } from './dialogs.svelte'
import type { Side } from './paths'
import { invoke, on, webview } from './tauri'

/** where a drag would drop: a side, and the folder row under the pointer if any */
export interface DropTarget {
  side: Side
  row: string | null
}

export const drag = $state({
  target: null as DropTarget | null,
  /** the pointer moved far enough to be a drag, not a click */
  moved: false,
  /** server items are being dragged out to the system's own drag */
  out: false,
})

/** the side and folder row under a point, if dropping there makes sense */
export function sideAt(x: number, y: number): DropTarget | null {
  const el = document.elementFromPoint(x, y)
  const pane = el?.closest<HTMLElement>('[data-side]')
  const side = pane?.dataset.side as Side | undefined
  if (!pane || !side) return null
  if ((side === 'remote' && !app.tab) || (side === 'local' && app.local.path === '')) return null
  const row = el?.closest<HTMLElement>('.row-e[data-dir="true"]')
  return { side, row: row && pane.contains(row) ? (row.dataset.name ?? null) : null }
}

function setTarget(t: DropTarget | null) {
  if (drag.target?.side === t?.side && drag.target?.row === t?.row) return
  drag.target = t
}

/** From a mouse press on selected rows of `from`: follows the pointer until the button is let go. */
export function startDrag(from: Side, down: MouseEvent): void {
  drag.moved = false
  const move = (ev: MouseEvent) => {
    if (
      !drag.moved &&
      Math.abs(ev.clientX - down.clientX) + Math.abs(ev.clientY - down.clientY) < 6
    )
      return
    drag.moved = true
    document.body.style.cursor = 'copy'
    // past the window's edge with server items: the system's own drag takes over
    // (Explorer or the file manager). The page keeps getting moves while the
    // button is held, so the position says it, not a mouseout.
    const r = 2
    if (
      ev.clientX < r ||
      ev.clientY < r ||
      ev.clientX >= innerWidth - r ||
      ev.clientY >= innerHeight - r
    )
      return out()
    const over = sideAt(ev.clientX, ev.clientY)
    setTarget(over && over.side !== from ? over : null)
  }
  const out = () => {
    if (from !== 'remote' || !app.canDragOut) return
    stop()
    drag.out = true
    void invoke('drag_out', {
      tab: app.active,
      dir: app.remote.path,
      entries: app.selectedEntries('remote'),
    })
  }
  const leave = (ev: MouseEvent) => {
    if (drag.moved && !ev.relatedTarget) out()
  }
  const up = () => {
    const target = drag.target
    stop()
    if (!drag.moved || !target) return
    const into = target.row
      ? app.join(target.side, app.pane(target.side).path, target.row)
      : app.pane(target.side).path
    if (from === 'local') app.upload(into)
    else void app.download(into)
  }
  const stop = () => {
    setTarget(null)
    document.body.style.cursor = ''
    removeEventListener('mousemove', move)
    removeEventListener('mouseup', up)
    document.removeEventListener('mouseout', leave)
  }
  addEventListener('mousemove', move)
  addEventListener('mouseup', up)
  document.addEventListener('mouseout', leave)
}

/** In from the system: dropped on the server side uploads (into a folder row when dropped on one). */
export function listenForDrops(): void {
  on('dragout-done', () => (drag.out = false))
  void webview.onDragDropEvent((ev) => {
    const p = ev.payload
    if (drag.out || anyModal()) return
    const at = () => {
      if (p.type === 'leave') return null
      const r = window.devicePixelRatio || 1
      const t = sideAt(p.position.x / r, p.position.y / r)
      return t && t.side === 'remote' ? t : null
    }
    if (p.type === 'over' || p.type === 'enter') setTarget(at())
    else if (p.type === 'leave') setTarget(null)
    else if (p.type === 'drop') {
      const t = at()
      setTarget(null)
      if (!t) {
        if (app.tab) app.setMessage('Drop files on the server side to upload them')
        return
      }
      const dir = t.row ? app.join('remote', app.remote.path, t.row) : app.remote.path
      void app.uploadPaths(p.paths, dir)
    }
  })
}
