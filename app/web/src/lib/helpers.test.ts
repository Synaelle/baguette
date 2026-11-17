// SPDX-License-Identifier: AGPL-3.0-or-later

import { describe, expect, it } from 'vitest'
import {
  ACCENTS,
  DEFAULT_ACCENT,
  fitAccent,
  luma,
  paletteFor,
  pickedAccent,
  shownAccent,
} from './colour'
import { byName, formatSize, formatStamp, siteLine } from './format'
import { inside, join, parent } from './paths'

describe('paths', () => {
  it('goes up on each side', () => {
    expect(parent('remote', '/a/b', false)).toBe('/a')
    expect(parent('remote', '/a', false)).toBe('/')
    expect(parent('local', 'C:\\Users\\me', false)).toBe('C:\\Users')
    expect(parent('local', 'C:\\Users', false)).toBe('C:\\')
    expect(parent('local', 'C:\\', false)).toBe('')
    expect(parent('local', '/home/me', true)).toBe('/home')
    expect(parent('local', '/', true)).toBe('')
  })
  it('joins on each side', () => {
    expect(join('remote', '/', 'a', false)).toBe('/a')
    expect(join('remote', '/a', 'b', false)).toBe('/a/b')
    expect(join('local', 'C:\\', 'a', false)).toBe('C:\\a')
    expect(join('local', '/home', 'a', true)).toBe('/home/a')
    expect(join('local', '', 'C:\\', false)).toBe('C:\\')
  })
  it('knows what is inside a 3DS folder, in any case', () => {
    expect(inside('/Nintendo 3DS/x', '/nintendo 3ds')).toBe(true)
    expect(inside('/Nintendo 3DS games', '/Nintendo 3DS')).toBe(false)
  })
})

describe('format', () => {
  it('shows sizes, times and sites', () => {
    expect(formatSize(512)).toBe('512 B')
    expect(formatSize(1536)).toBe('1.5 KB')
    expect(formatStamp('20260105123000')).toBe('2026-01-05 12:30')
    expect(formatStamp('')).toBe('')
    expect(siteLine('sftp', 'pi.local')).toBe('SFTP · pi.local')
    expect(siteLine('ftps-implicit', 'nas')).toBe('FTPS · nas')
  })
  it('sorts folders first, numbers as a person would', () => {
    const e = (name: string, dir = false) => ({ name, dir, size: 0, modified: '' })
    const names = [e('file 10'), e('b', true), e('file 2'), e('a', true)]
      .sort(byName)
      .map((x) => x.name)
    expect(names).toEqual(['a', 'b', 'file 2', 'file 10'])
  })
})

describe('colour', () => {
  const hex = (h: string) => [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
  it('makes any colour as bright as the red', () => {
    const red = luma(hex(DEFAULT_ACCENT))
    for (const c of ['00ff00', '0000ff', '808080'])
      expect(luma(hex(fitAccent(c)))).toBeCloseTo(red, 2)
  })
  it('keeps the presets as they are', () => {
    for (const [p] of ACCENTS) expect(pickedAccent('#' + p.toUpperCase())).toBe(p)
  })
  it('uses the hand-picked palette for the red, and derives one for other colours', () => {
    expect(paletteFor(DEFAULT_ACCENT, 'evil-twin')).toBeNull()
    const green = paletteFor('2b9349', 'ficelle')
    expect(green?.['--accent']).toBe('#2b9349')
    expect(Object.keys(green ?? {})).toContain('--sel')
    expect(shownAccent(DEFAULT_ACCENT, 'ficelle-light')).toBe('c1271b')
  })
})
