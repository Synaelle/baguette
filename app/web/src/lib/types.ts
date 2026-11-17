// SPDX-License-Identifier: AGPL-3.0-or-later
// What the Rust side sends and takes (app/src/main.rs, commands.rs, local.rs).

export interface Entry {
  name: string
  dir: boolean
  size: number
  modified: string // "YYYYMMDDhhmmss", empty if unknown
}

export interface Site {
  id: number
  name: string
  protocol: Protocol
  host: string
  port: number
  user: string
  has_pass: boolean
  key: string
  has_passphrase: boolean
  start_dir: string
  needs_password: boolean
}

export type Protocol = 'ftp' | 'sftp' | 'ftps' | 'ftps-implicit'

/** A site from the editor; pass and passphrase null keep the saved ones. */
export interface SiteIn {
  id: number
  name: string
  protocol: Protocol
  host: string
  port: number
  user: string
  pass: string | null
  key: string
  passphrase: string | null
  start_dir: string
}

export interface SendSettings {
  send_dir: string
  send_items: string[]
  send_replace: boolean
}

export interface Init {
  version: string
  protocol: number
  accent: string
  look: Look
  local_start: string
  local_dir: string
  windows_dir: string | null
  protect_3ds: boolean
  protected: string[]
  protected_free: string[]
  check_updates: boolean
  bundles: boolean
  bmp_to_png: boolean
  send_to_pc: boolean
  mode: Mode | ''
  mode_arg: Mode | null
  settings: SendSettings
  sites: Site[]
  home: string
  linux: boolean
  drag_out: boolean
}

export type Look = 'evil-twin' | 'ficelle' | 'ficelle-light'
export type Mode = 'standard' | 'lite'

export interface TabInfo {
  id: number
  saved_id: number
  name: string
  protocol: Protocol
  host: string
  start: string
}

export interface Cement {
  battery: string
  battery_percent: number
  charging: boolean
  plugged: boolean
  low: boolean
  sd_free: string
  sd_total: string
  sd_permille: number
  cias: { path: string; name: string; size: string }[]
  installing: { name: string; permille: number } | null
  version: string
  protocol: number
  login: boolean
  update: string
}

export type ConnState = 'waiting' | 'connected' | 'refused'

export interface Status {
  tab: number
  part0: string
  part1: string
}

export interface Progress {
  active: boolean
  permille: number
  up: boolean
  name: string
  detail: string
}

export interface Listing {
  tab: number
  notify: number // 1: the 3DS picker's listing
  path: string
  ok: boolean
  entries: Entry[]
}

export interface LocalListing {
  path: string
  ok: boolean
  error: string
  entries: Entry[]
}

export interface Refused {
  tab: number
  message: string
  fingerprint: string // empty: a login was refused
  host: string
  protocol: Protocol
}

export interface SendBack {
  tab: number
  local: string
  remote: string
  remote_text: string
  local_text: string
  warn: string
}

export interface Update {
  version: string
  url: string
  install: boolean
}
