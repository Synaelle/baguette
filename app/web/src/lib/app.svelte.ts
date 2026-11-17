// SPDX-License-Identifier: AGPL-3.0-or-later
// Baguette's window state. Each tab is one site: the PC side on the left, the
// server on the right. The PC side is shared by all tabs, the server side
// follows the tab on screen.

import { SvelteSet } from 'svelte/reactivity'
import { applyLook, DEFAULT_ACCENT } from './colour'
import { ask, tell } from './dialogs.svelte'
import { byName, recall, RUNS, store } from './format'
import { inside, isRoot, join, parent, type Side } from './paths'
import { invoke } from './tauri'
import type {
  Cement,
  ConnState,
  Entry,
  Init,
  LocalListing,
  Listing,
  Look,
  Progress,
  Refused,
  SendSettings,
  Site,
  TabInfo,
  Update,
} from './types'

/** One side's listing: the folder, its entries, the selection and the focused row. */
export class PaneData {
  path = $state('')
  /** the folder the entries are of, null while it loads */
  shown = $state<string | null>(null)
  entries = $state.raw<Entry[]>([])
  selected = $state(new SvelteSet<string>())
  anchor = $state(-1)
  focus = $state(0)
  /** selected once the next listing comes (a new or renamed item) */
  selectNext = ''
  /** the entry being renamed in place */
  renaming = $state<string | null>(null)

  constructor(path: string) {
    this.path = path
  }
}

export interface Tab {
  id: number
  saved_id: number
  name: string
  host: string
  protocol: string
  state: ConnState
  cement: Cement | null
  remote: PaneData
  status: { part0: string; part1: string }
  progress: Progress
  refused: Refused | null
  /** the PC side lists its folder again once the download job is done */
  refreshLocal: boolean
}

class AppState {
  ready = $state(false)
  version = $state('')
  protocol = $state(0)
  linux = $state(false)
  canDragOut = $state(true)
  windowsDir = $state<string | null>(null)
  protectedPaths: string[] = []
  freePaths: string[] = []

  protect3ds = $state(true)
  checkUpdates = $state(true)
  bundles = $state(false)
  bmpToPng = $state(false)
  sendToPc = $state(false)
  localStart = $state('home')
  send = $state<SendSettings>({ send_dir: '', send_items: [], send_replace: false })

  accent = $state(DEFAULT_ACCENT)
  look = $state<Look>('evil-twin')

  sites = $state<Site[]>([])
  tabs = $state<Tab[]>([])
  active = $state<number | null>(null)
  local = new PaneData('')
  activeSide = $state<Side>('local')
  /** the server side when no tab is open */
  private blank = new PaneData('/')
  /** shown when no tab is open */
  message = $state('')
  update = $state<Update | null>(null)
  updating = $state(false)

  tab = $derived(this.tabs.find((t) => t.id === this.active) ?? null)
  remote = $derived(this.tab ? this.tab.remote : this.blank)
  connected = $derived(this.tab?.state === 'connected')
  cement = $derived(this.connected ? (this.tab?.cement ?? null) : null)

  binName = $derived(this.linux ? 'the trash' : 'the Recycle Bin')

  pane(side: Side): PaneData {
    return side === 'local' ? this.local : this.remote
  }
  parent(side: Side, p: string): string {
    return parent(side, p, this.linux)
  }
  join(side: Side, d: string, n: string): string {
    return join(side, d, n, this.linux)
  }

  async start(): Promise<void> {
    const init = await invoke<Init>('init')
    this.version = init.version
    this.protocol = init.protocol
    this.send = init.settings
    this.accent = init.accent
    this.look = init.look
    applyLook(this.look, this.accent)
    this.sites = init.sites
    this.localStart = init.local_start
    this.protect3ds = init.protect_3ds
    this.protectedPaths = init.protected
    this.freePaths = init.protected_free
    this.windowsDir = init.windows_dir
    this.linux = init.linux
    this.canDragOut = init.drag_out
    this.checkUpdates = init.check_updates
    this.bundles = init.bundles
    this.bmpToPng = init.bmp_to_png
    this.sendToPc = init.send_to_pc
    this.ready = true
    if (this.checkUpdates) setTimeout(() => void this.lookForUpdate(), 2500) // after the window is up
    void this.navigate('local', init.local_dir)
    // the sites that were open last time
    const only = this.sites.length === 1 ? this.sites[0] : undefined
    let reopen = recall<number[]>('openSites', only ? [only.id] : [])
    reopen = reopen.filter((id) => this.sites.some((s) => s.id === id))
    for (const id of reopen) await this.openSite(id)
  }

  // ---------------------------------------------------------------------------
  // tabs

  addTab(info: TabInfo): void {
    const t: Tab = {
      id: info.id,
      saved_id: info.saved_id,
      name: info.name,
      host: info.host,
      protocol: info.protocol,
      state: 'waiting',
      cement: null,
      remote: new PaneData(info.start || ''),
      status: { part0: `Connecting to ${info.host}...`, part1: '' },
      progress: { active: false, permille: 0, up: false, name: '', detail: '' },
      refused: null,
      refreshLocal: false,
    }
    this.tabs.push(t)
    this.activate(t.id)
    this.rememberTabs()
  }

  async openSite(id: number): Promise<void> {
    try {
      this.addTab(await invoke<TabInfo>('open_site', { id }))
    } catch (e) {
      void tell(e)
    }
  }

  /** "sftp://user@host", "192.168.1.10:5000"... */
  async openUrl(url: string): Promise<boolean> {
    try {
      this.addTab(await invoke<TabInfo>('open_url', { url, pass: '' }))
      return true
    } catch (e) {
      void tell(e)
      return false
    }
  }

  closeTab(id: number): void {
    const i = this.tabs.findIndex((t) => t.id === id)
    if (i < 0) return
    void invoke('close_tab', { tab: id })
    this.tabs.splice(i, 1)
    if (this.active === id) {
      this.active = null
      const last = this.tabs[this.tabs.length - 1]
      if (last) this.activate(last.id)
    }
    this.rememberTabs()
  }

  activate(id: number): void {
    this.active = id
    const t = this.tab
    if (t && t.state === 'connected' && t.remote.shown === null && t.remote.path) {
      void this.navigate('remote', t.remote.path)
    }
  }

  rememberTabs(): void {
    store(
      'openSites',
      this.tabs.filter((t) => t.saved_id).map((t) => t.saved_id),
    )
  }

  tabOfSite(id: number): Tab | undefined {
    return this.tabs.find((t) => t.saved_id === id)
  }

  /** a message in the status bar of the tab on screen */
  setMessage(text: string): void {
    if (this.active !== null) void invoke('status_message', { tab: this.active, text })
    else this.message = text
  }

  // ---------------------------------------------------------------------------
  // listings

  async navigate(side: Side, path: string): Promise<void> {
    const st = this.pane(side)
    st.path = path
    if (path !== st.shown) {
      // never leave the previous folder clickable under the new path
      st.entries = []
      st.shown = null
      st.selected = new SvelteSet()
      st.focus = 0
    }
    if (side === 'local') {
      const r = await invoke<LocalListing>('local_list', { path })
      this.show('local', r)
      if (!r.ok) this.setMessage(`Can't open ${path}: ${r.error}`)
      else if (this.local.path)
        invoke('remember_local', { dir: this.local.path }).catch((e) => void tell(e))
    } else if (this.active !== null) {
      void invoke('navigate', { tab: this.active, path })
    }
  }

  refresh(side: Side): void {
    void this.navigate(side, this.pane(side).path)
  }

  up(side: Side): void {
    const st = this.pane(side)
    if (!isRoot(side, st.path)) void this.navigate(side, this.parent(side, st.path))
  }

  show(side: Side, r: LocalListing | Listing, st = this.pane(side)): void {
    if (r.path !== st.path) return
    if (!r.ok) {
      if (!isRoot(side, st.path)) void this.navigate(side, this.parent(side, st.path))
      return
    }
    const keep = st.selectNext ? [st.selectNext] : r.path === st.shown ? [...st.selected] : []
    st.selectNext = ''
    st.entries = [...r.entries].sort(byName)
    st.shown = r.path
    st.selected = new SvelteSet(keep.filter((n) => st.entries.some((e) => e.name === n)))
  }

  /** a listing from a tab's browse connection */
  listed(r: Listing): void {
    const t = this.tabs.find((x) => x.id === r.tab)
    if (!t) return
    // the first listing tells where the server's home is
    if (!t.remote.path) t.remote.path = r.path
    if (t.refreshLocal && !t.progress.active) {
      t.refreshLocal = false
      if (this.local.path) this.refresh('local')
    }
    if (t.id === this.active) {
      this.show('remote', r, t.remote)
      if (!r.ok) this.setMessage("Can't open " + r.path)
    } else if (r.path === t.remote.path) {
      // a background tab keeps its listing up to date
      t.remote.entries = [...r.entries].sort(byName)
      t.remote.shown = r.path
    }
  }

  selectedEntries(side: Side): Entry[] {
    const st = this.pane(side)
    return st.entries.filter((e) => st.selected.has(e.name))
  }

  select(side: Side, names: string[]): void {
    this.pane(side).selected = new SvelteSet(names)
  }

  // ---------------------------------------------------------------------------
  // what can be changed: the 3DS's protected paths, the Windows folder

  readOnly(path: string): boolean {
    return (
      this.protect3ds &&
      !!this.tab?.cement &&
      this.protectedPaths.some((p) => inside(path, p)) &&
      !this.freePaths.some((p) => inside(path, p))
    )
  }

  private winNorm(p: string): string {
    return this.linux
      ? p.replace(/\/+$/, '')
      : p.replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase()
  }
  private get localSep(): string {
    return this.linux ? '/' : '\\'
  }

  localReadOnly(path: string): boolean {
    if (!this.windowsDir || !path) return false
    const a = this.winNorm(path)
    const w = this.winNorm(this.windowsDir)
    return a === w || a.startsWith(w + this.localSep)
  }

  /** something selected that is, holds or sits in a folder Baguette keeps read-only */
  touchesLocked(side: Side): boolean {
    if (side === 'local') {
      if (!this.windowsDir) return false
      const w = this.winNorm(this.windowsDir)
      const sep = this.localSep
      return this.selectedEntries('local').some((e) => {
        const a = this.winNorm(this.join('local', this.local.path, e.name))
        return a === w || a.startsWith(w + sep) || w.startsWith(a + sep)
      })
    }
    return this.selectedEntries('remote').some((e) => {
      const p = this.join('remote', this.remote.path, e.name)
      return (
        this.readOnly(p) ||
        (this.protect3ds && !!this.tab?.cement && this.protectedPaths.some((x) => inside(x, p)))
      )
    })
  }

  // ---------------------------------------------------------------------------
  // file operations

  async openEntry(side: Side, e: Entry | null): Promise<void> {
    if (!e) return this.up(side)
    const st = this.pane(side)
    if (e.dir) return this.navigate(side, this.join(side, st.path, e.name))
    if (side === 'local') {
      invoke('local_open', { path: this.join('local', st.path, e.name) }).catch(
        (err) => void tell(err),
      )
      return
    }
    // opening a program from a server runs it on this PC
    if (
      RUNS.test(e.name) &&
      !(await ask(
        `${e.name} is a program or script: opening it runs it on this PC. Open it?`,
        'Run it',
        'Cancel',
      ))
    )
      return
    void invoke('open_file', { tab: this.active, dir: st.path, entry: e })
  }

  newFolder(side: Side): void {
    const st = this.pane(side)
    if (side === 'local' && st.path === '') return
    let name = 'New folder'
    for (let n = 2; st.entries.some((e) => e.name === name); n++) name = `New folder (${n})`
    st.selectNext = name
    const path = this.join(side, st.path, name)
    if (side === 'local')
      invoke('local_mkdir', { path }).then(
        () => this.refresh('local'),
        (e) => void tell(e),
      )
    else invoke('mkdir', { tab: this.active, path }).catch((e) => void tell(e))
  }

  async deleteSelected(side: Side): Promise<void> {
    const st = this.pane(side)
    const sel = this.selectedEntries(side)
    const first = sel[0]
    if (!first || (side === 'local' && st.path === '')) return
    const where = side === 'local' ? ` (to ${this.binName})` : ''
    const q =
      sel.length === 1
        ? `Delete "${first.name}"${first.dir ? ' and everything in it' : ''}${where}?`
        : `Delete ${sel.length} items${where}?`
    if (!(await ask(q))) return
    if (side === 'local') {
      invoke('local_delete', { paths: sel.map((e) => this.join('local', st.path, e.name)) }).then(
        () => this.refresh('local'),
        (e) => {
          void tell(e)
          this.refresh('local')
        },
      )
    } else
      invoke('delete', { tab: this.active, dir: st.path, entries: sel }).catch((e) => void tell(e))
  }

  startRename(side: Side): void {
    const st = this.pane(side)
    const sel = this.selectedEntries(side)
    const only = sel.length === 1 ? sel[0] : undefined
    if (!only || (side === 'local' && st.path === '')) {
      this.setMessage('Select one file or folder to rename')
      return
    }
    st.renaming = only.name
  }

  finishRename(side: Side, from: string, name: string): void {
    const st = this.pane(side)
    st.renaming = null
    name = name.trim()
    const bad = side === 'local' ? /[\\/:*?"<>|]/ : /[\\/]/
    if (!name || name === from || bad.test(name)) return
    if (st.entries.some((e) => e.name === name))
      return void tell(`Can't rename ${from}: ${name} is already in this folder`)
    st.selectNext = name
    const a = this.join(side, st.path, from)
    const b = this.join(side, st.path, name)
    if (side === 'local')
      invoke('local_rename', { from: a, to: b }).then(
        () => this.refresh('local'),
        (e) => void tell(e),
      )
    else invoke('rename', { tab: this.active, from: a, to: b }).catch((e) => void tell(e))
  }

  upload(to?: string): void {
    const sel = this.selectedEntries('local')
    if (!this.tab || !sel.length || this.local.path === '') return
    const paths = sel.map((e) => this.join('local', this.local.path, e.name))
    void this.uploadPaths(paths, to ?? this.remote.path)
  }

  /** asks first when the upload would replace files on the server, as downloads do */
  async uploadPaths(paths: string[], dir: string): Promise<void> {
    const tab = this.active
    const names = paths.map((p) => p.slice(Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\')) + 1))
    try {
      const there = await invoke<string[]>('remote_existing', { tab, dir, names })
      if (there.length) {
        const what = there.length === 1 ? `${there[0]} is` : `${there.length} of these are`
        const them = there.length === 1 ? 'it' : 'them'
        if (!(await ask(`${what} already in ${dir}. Replace ${them}?`, 'Replace', 'Cancel'))) return
      }
      await invoke('upload', { tab, paths, dir })
    } catch (e) {
      void tell(e)
    }
  }

  async download(to?: string, pick = false): Promise<void> {
    const t = this.tab
    const sel = this.selectedEntries('remote')
    if (!t || !sel.length) return
    let dest: string | null = to ?? this.local.path
    if (pick || dest === '')
      dest = await invoke<string | null>('browse_folder', { title: 'Download to' })
    if (!dest) return
    const there = await invoke<string[]>('local_existing', {
      dir: dest,
      names: sel.map((e) => e.name),
    })
    if (there.length) {
      const what = there.length === 1 ? `${there[0]} is` : `${there.length} of these are`
      const them = there.length === 1 ? 'it' : 'them'
      if (!(await ask(`${what} already in ${dest}. Replace ${them}?`, 'Replace', 'Cancel'))) return
    }
    invoke('download', { tab: this.active, dir: this.remote.path, entries: sel, to: dest }).catch(
      (e) => void tell(e),
    )
    t.refreshLocal = true // the worker lists the server folder again once the job is done
  }

  // ---------------------------------------------------------------------------
  // settings that change the window

  async lookForUpdate(): Promise<void> {
    this.update = await invoke<Update | null>('check_update').catch(() => null)
    invoke('refresh_tabs').catch(() => {}) // an older Cement can be offered the release too
  }
}

export const app = new AppState()
