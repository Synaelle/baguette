// SPDX-License-Identifier: AGPL-3.0-or-later
// The colour: every colour of the palette takes the accent's hue, and keeps the
// brightness of the hand-picked red one so everything stays as easy to read.
// PALETTE and LIGHT_PALETTE are the values of styles/app.css, which must match.

import type { Look } from './types'

export const DEFAULT_ACCENT = 'e44c3e'

const PALETTE: Record<string, string> = {
  '--accent': '#e44c3e',
  '--accent-hi': '#ec6457',
  '--hi': '#ec7366',
  '--soft': '#e8b4ab',
  '--bg': '#0e0d0d',
  '--blob': '#131111',
  '--surface': '#161414',
  '--well': '#110f0f',
  '--quick': '#131111',
  '--track': '#1f1c1b',
  '--sel': '#3a1915',
  '--edge': '#423b38',
  '--line': '#2e2a28',
  '--input': '#6a615c',
  '--text': '#e6dfd8',
  '--dim': '#9a918a',
  '--mute': '#8a817a',
  '--tag': '#ec7366',
  '--focus': '#7a716b',
}

// Ficelle's light room: the same names, the lights on
const LIGHT_ACCENT = 'c1271b'
const LIGHT_PALETTE: Record<string, string> = {
  '--accent': '#c1271b',
  '--accent-hi': '#ad2117',
  '--hi': '#c1271b',
  '--soft': '#7a2a20',
  '--bg': '#f1efea',
  '--blob': '#e9e6e0',
  '--surface': '#fbfaf7',
  '--well': '#f1efea',
  '--quick': '#f6f4ef',
  '--track': '#e5e2db',
  '--sel': '#f5d5ce',
  '--edge': '#a9a298',
  '--line': '#cbc6bc',
  '--input': '#8f887f',
  '--text': '#161413',
  '--dim': '#5e5853',
  '--mute': '#6e6761',
  '--tag': '#a81f15',
  '--focus': '#7a736b',
}

/** The same list as in Cement, already as bright as the red (dark text on them reads at 5:1). */
export const ACCENTS: [string, string][] = [
  ['e44c3e', 'Red'],
  ['ce622d', 'Orange'],
  ['b6721c', 'Amber'],
  ['2b9349', 'Green'],
  ['20908a', 'Teal'],
  ['4981d5', 'Blue'],
  ['946dd5', 'Violet'],
  ['cd53a6', 'Pink'],
]

type Rgb = [number, number, number]

const rgbOf = (hex: string): Rgb => {
  const n = parseInt(hex.replace('#', ''), 16)
  return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}
const hexOf = (c: number[]): string =>
  c
    .map((x) =>
      Math.round(Math.min(1, Math.max(0, x)) * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')

function toHsl([r, g, b]: Rgb): Rgb {
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  if (max === min) return [0, 0, l]
  const d = max - min
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  const h =
    max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return [h / 6, s, l]
}

function fromHsl(h: number, s: number, l: number): number[] {
  const k = (n: number) => (n + h * 12) % 12
  const a = s * Math.min(l, 1 - l)
  return [0, 8, 4].map((n) => l - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1)))
}

/** relative luminance, as in contrast ratios */
export function luma(c: number[]): number {
  const [r = 0, g = 0, b = 0] = c.map((x) =>
    x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4,
  )
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** the colour of hue h and saturation s that is exactly as bright as `like` */
function asBright(h: number, s: number, like: string): string {
  const target = luma(rgbOf(like))
  let lo = 0
  let hi = 1
  for (let i = 0; i < 30; i++) {
    const m = (lo + hi) / 2
    if (luma(fromHsl(h, s, m)) < target) lo = m
    else hi = m
  }
  return hexOf(fromHsl(h, s, (lo + hi) / 2))
}

/** any picked colour, made as bright as the red (dark text on it stays readable) */
export function fitAccent(hex: string): string {
  const [h, s] = toHsl(rgbOf(hex))
  return asBright(h, s, DEFAULT_ACCENT)
}

/** a preset as it is, any other colour made as bright as the red */
export function pickedAccent(hex: string): string {
  const h = hex.replace('#', '').toLowerCase()
  return ACCENTS.some(([p]) => p === h) ? h : fitAccent(h)
}

/** the accent as shown: the light room wants it darker, as dark as its own red */
export function shownAccent(hex: string, look: Look): string {
  if (look !== 'ficelle-light') return hex
  const [h, s] = toHsl(rgbOf(hex))
  return hex === DEFAULT_ACCENT ? LIGHT_ACCENT : asBright(h, s, LIGHT_ACCENT)
}

/** The palette's colours for this accent and look, or null for the hand-picked palette itself. */
export function paletteFor(hex: string, look: Look): Record<string, string> | null {
  if (hex === DEFAULT_ACCENT) return null
  const base = look === 'ficelle-light' ? LIGHT_PALETTE : PALETTE
  const shown = shownAccent(hex, look)
  const [h, sat] = toHsl(rgbOf(shown))
  const s0 = toHsl(rgbOf(base['--accent'] ?? ''))[1]
  const out: Record<string, string> = { '--accent': '#' + shown }
  for (const [name, value] of Object.entries(base)) {
    if (name === '--accent') continue
    // a greyer accent gives greyer surfaces
    out[name] = '#' + asBright(h, toHsl(rgbOf(value))[1] * Math.min(1, sat / s0), value)
  }
  return out
}

/** Evil Twin, Ficelle or Ficelle light: classes on the page, then the colour for that room. */
export function applyLook(look: Look, hex: string): void {
  const root = document.documentElement
  root.classList.toggle('ficelle', look.startsWith('ficelle'))
  root.classList.toggle('light', look === 'ficelle-light')
  const palette = paletteFor(hex, look)
  for (const name of Object.keys(PALETTE)) root.style.removeProperty(name)
  if (palette)
    for (const [name, value] of Object.entries(palette)) root.style.setProperty(name, value)
}
