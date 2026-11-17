// SPDX-License-Identifier: AGPL-3.0-or-later
// The icons the window uses, from Lucide (ISC licence), bundled as svg text at
// build time. Only the inner shapes are kept: Icon.svelte draws the svg around them.

import arrowDown from 'lucide-static/icons/arrow-down.svg?raw'
import arrowUp from 'lucide-static/icons/arrow-up.svg?raw'
import file from 'lucide-static/icons/file.svg?raw'
import folder from 'lucide-static/icons/folder.svg?raw'
import hardDrive from 'lucide-static/icons/hard-drive.svg?raw'
import layoutGrid from 'lucide-static/icons/layout-grid.svg?raw'
import list from 'lucide-static/icons/list.svg?raw'
import lock from 'lucide-static/icons/lock.svg?raw'
import minus from 'lucide-static/icons/minus.svg?raw'
import pencil from 'lucide-static/icons/pencil.svg?raw'
import rotateCw from 'lucide-static/icons/rotate-cw.svg?raw'
import settings from 'lucide-static/icons/settings.svg?raw'
import square from 'lucide-static/icons/square.svg?raw'
import up from 'lucide-static/icons/corner-left-up.svg?raw'
import x from 'lucide-static/icons/x.svg?raw'

const inner = (svg: string): string =>
  svg.replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '')

export const ICONS = {
  download: inner(arrowDown),
  upload: inner(arrowUp),
  file: inner(file),
  folder: inner(folder),
  drive: inner(hardDrive),
  tiles: inner(layoutGrid),
  list: inner(list),
  lock: inner(lock),
  minimize: inner(minus),
  edit: inner(pencil),
  refresh: inner(rotateCw),
  settings: inner(settings),
  maximize: inner(square),
  up: inner(up),
  close: inner(x),
}

export type IconName = keyof typeof ICONS
