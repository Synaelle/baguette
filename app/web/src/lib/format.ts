// SPDX-License-Identifier: AGPL-3.0-or-later

import type { Entry } from './types'

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

/** Folders first, then names as a person sorts them ("file 2" before "file 10"). */
export const byName = (a: Entry, b: Entry): number =>
  a.dir !== b.dir ? (a.dir ? -1 : 1) : collator.compare(a.name, b.name)

export function formatSize(v: number): string {
  if (v >= 1073741824) return (v / 1073741824).toFixed(2) + ' GB'
  if (v >= 1048576) return (v / 1048576).toFixed(1) + ' MB'
  if (v >= 1024) return (v / 1024).toFixed(1) + ' KB'
  return v + ' B'
}

/** "YYYYMMDDhhmmss" to "2026-01-05 12:30" */
export function formatStamp(s: string): string {
  if (!s || s.length < 12) return ''
  return `${s.slice(0, 4)}-${s.slice(4, 6)}-${s.slice(6, 8)} ${s.slice(8, 10)}:${s.slice(10, 12)}`
}

export function store(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // a webview without storage just forgets
  }
}

export function recall<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key)
    return v === null ? fallback : (JSON.parse(v) as T)
  } catch {
    return fallback
  }
}

/** File types the system runs when they are opened. */
export const RUNS =
  /\.(exe|com|bat|cmd|msi|msp|lnk|url|scr|pif|cpl|hta|jar|js|jse|vbs|vbe|wsf|wsh|ps1|psm1|reg|appimage|desktop|sh|run)$/i

const PROTO: Record<string, string> = {
  ftp: 'FTP',
  sftp: 'SFTP',
  ftps: 'FTPS',
  'ftps-implicit': 'FTPS',
}

/** A site's protocol and host, under its name: "SFTP · pi.local". */
export function siteLine(protocol: string, host: string): string {
  return `${PROTO[protocol] ?? protocol} · ${host}`
}
