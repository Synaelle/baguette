// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
// Chomp's window. The work happens in Rust (invoke = a command, listen = an event).
// Each tab is one site: the PC side on the left, the server on the right.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const appWindow = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
const byName = (a, b) => (a.dir !== b.dir ? (a.dir ? -1 : 1) : collator.compare(a.name, b.name));

function formatSize(v) {
  if (v >= 1073741824) return (v / 1073741824).toFixed(2) + " GB";
  if (v >= 1048576) return (v / 1048576).toFixed(1) + " MB";
  if (v >= 1024) return (v / 1024).toFixed(1) + " KB";
  return v + " B";
}
// "YYYYMMDDhhmmss" -> "2026-01-05 12:30"
function formatStamp(s) {
  if (!s || s.length < 12) return "";
  return `${s.slice(0, 4)}-${s.slice(4, 6)}-${s.slice(6, 8)} ${s.slice(8, 10)}:${s.slice(10, 12)}`;
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}
function store(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}
function recall(key, fallback) {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
}

const ICON = {
  dir: '<svg class="ico dir" viewBox="0 0 16 16"><path d="M1.5 4c0-.8.7-1.5 1.5-1.5h3.2l1.5 1.5H13c.8 0 1.5.7 1.5 1.5v6.5c0 .8-.7 1.5-1.5 1.5H3c-.8 0-1.5-.7-1.5-1.5z"/></svg>',
  file: '<svg class="ico file" viewBox="0 0 16 16"><path d="M4 1.5h5L12.5 5v9c0 .3-.2.5-.5.5H4c-.3 0-.5-.2-.5-.5V2c0-.3.2-.5.5-.5zM9 1.5V5h3.5"/></svg>',
  drive: '<svg class="ico drive" viewBox="0 0 16 16"><path d="M2 9.5h12v3c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1zM2 9.5 4 3.5h8l2 6M11.5 11.5h.01"/></svg>',
};
const PENCIL = '<svg viewBox="0 0 16 16"><path d="M10.5 2.5l3 3-8 8H2.5v-3z"/></svg>';
const CROSS = '<svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6"/></svg>';

// ---------------------------------------------------------------------------
// state

let sites = [];          // saved sites (no passwords)
const tabs = new Map();  // id -> tab
let active = null;       // the tab on screen
let settings = null;     // Send to PC
let activePane = null;   // the pane the toolbar acts on
let draggingOut = false;

function newPaneState(path) {
  return { path, shown: null, entries: [], selected: new Set(), anchor: -1, focus: 0, selectNext: "" };
}
const tab = () => (active ? tabs.get(active) : null);

// ---------------------------------------------------------------------------
// a list of files: the PC side and the server side work the same way

class Pane {
  constructor(el, kind) {
    this.el = el;
    this.kind = kind; // "local" | "remote"
    this.list = el.querySelector(".list");
    this.rows = el.querySelector(".rows");
    this.pathInput = el.querySelector(".path");
    this.st = newPaneState("");
    el.querySelector(".up").onclick = () => this.up();
    this.pathInput.addEventListener("keydown", (ev) => {
      ev.stopPropagation();
      if (ev.key === "Enter") this.typedPath();
      if (ev.key === "Escape") {
        this.pathInput.value = this.st.path;
        this.list.focus();
      }
    });
    this.pathInput.addEventListener("blur", () => (this.pathInput.value = this.st.path || (this.kind === "local" ? "This PC" : "")));
    this.wire();
  }

  // paths
  isRoot(p) {
    return this.kind === "local" ? p === "" : p === "/";
  }
  parent(p) {
    if (this.kind === "local") {
      const t = p.replace(/\\+$/, "");
      const i = t.lastIndexOf("\\");
      if (i < 0) return "";
      const head = t.slice(0, i);
      return head.endsWith(":") ? head + "\\" : head;
    }
    const i = p.lastIndexOf("/");
    return i <= 0 ? "/" : p.slice(0, i);
  }
  join(d, n) {
    if (this.kind === "local") return d === "" ? n : d.endsWith("\\") ? d + n : d + "\\" + n;
    return d === "/" ? "/" + n : d + "/" + n;
  }

  // rows: an optional ".." then the entries; row index -> entry (null for "..")
  hasParent() {
    return !this.isRoot(this.st.path);
  }
  entryAt(i) {
    if (this.hasParent()) return i === 0 ? null : this.st.entries[i - 1];
    return this.st.entries[i];
  }
  rowCount() {
    return this.st.entries.length + (this.hasParent() ? 1 : 0);
  }
  rowOf(entry) {
    return this.st.entries.indexOf(entry) + (this.hasParent() ? 1 : 0);
  }
  selectedEntries() {
    return this.st.entries.filter((e) => this.st.selected.has(e.name));
  }

  icon(e) {
    if (this.kind === "local" && this.st.path === "") return ICON.drive;
    return e.dir ? ICON.dir : ICON.file;
  }

  render() {
    const out = [];
    if (this.hasParent()) out.push(`<div class="row-e" data-i="0"><span class="name">${ICON.dir}<span>..</span></span><span class="size"></span><span class="date"></span></div>`);
    this.st.entries.forEach((e, k) => {
      const i = k + (this.hasParent() ? 1 : 0);
      out.push(
        `<div class="row-e${this.st.selected.has(e.name) ? " sel" : ""}${i === this.st.focus ? " focus" : ""}" data-i="${i}">` +
          `<span class="name">${this.icon(e)}<span>${escapeHtml(e.name)}</span></span>` +
          `<span class="size">${e.dir ? "" : formatSize(e.size)}</span><span class="date">${formatStamp(e.modified)}</span></div>`
      );
    });
    this.rows.innerHTML = out.join("");
    this.pathInput.value = this.st.path || (this.kind === "local" ? "This PC" : "");
  }

  setSelection(names) {
    this.st.selected = new Set(names);
    for (const r of this.rows.children) {
      const e = this.entryAt(+r.dataset.i);
      r.classList.toggle("sel", !!e && this.st.selected.has(e.name));
      r.classList.toggle("focus", +r.dataset.i === this.st.focus);
    }
    updateToolbar();
  }

  // the tab's listing state (the server side follows the tab on screen)
  attach(st) {
    this.st = st;
    this.render();
  }

  navigate(path) {
    const st = this.st;
    st.path = path;
    if (path !== st.shown) {
      // never leave the previous folder clickable under the new path
      st.entries = [];
      st.shown = null;
      st.selected = new Set();
      st.focus = 0;
      this.render();
    }
    this.load(path);
  }
  refresh() {
    this.navigate(this.st.path);
  }

  async load(path) {
    if (this.kind === "local") {
      const r = await invoke("local_list", { path });
      this.show(r);
      if (!r.ok) setMessage("Can't open " + path + ": " + r.error);
      else if (this.st.path) invoke("remember_local", { dir: this.st.path });
    } else if (tab()) {
      invoke("navigate", { tab: active, path });
    }
  }

  show(r) {
    const st = this.st;
    if (r.path !== st.path) return;
    if (!r.ok) {
      if (!this.isRoot(st.path)) this.navigate(this.parent(st.path));
      return;
    }
    const keep = st.selectNext ? new Set([st.selectNext]) : r.path === st.shown ? st.selected : new Set();
    st.selectNext = "";
    st.entries = r.entries.sort(byName);
    st.shown = r.path;
    st.selected = new Set([...keep].filter((n) => st.entries.some((e) => e.name === n)));
    this.render();
    updateToolbar();
  }

  typedPath() {
    let p = this.pathInput.value.trim();
    if (this.kind === "local") {
      if (p.toLowerCase() === "this pc") p = "";
      if (/^[a-z]:$/i.test(p)) p += "\\";
      this.navigate(p);
    } else {
      invoke("normalize", { path: p }).then((n) => this.navigate(n || "/"));
    }
    this.list.focus();
  }

  up() {
    if (!this.isRoot(this.st.path)) this.navigate(this.parent(this.st.path));
  }

  openRow(i) {
    if (i < 0 || i >= this.rowCount()) return;
    const e = this.entryAt(i);
    if (!e) return this.up();
    if (e.dir) return this.navigate(this.join(this.st.path, e.name));
    if (this.kind === "local") invoke("local_open", { path: this.join(this.st.path, e.name) }).catch(tell);
    else invoke("open_file", { tab: active, dir: this.st.path, entry: e });
  }

  newFolder() {
    if (this.kind === "local" && this.st.path === "") return;
    let name = "New folder";
    for (let n = 2; this.st.entries.some((e) => e.name === name); n++) name = `New folder (${n})`;
    this.st.selectNext = name;
    const path = this.join(this.st.path, name);
    if (this.kind === "local") invoke("local_mkdir", { path }).then(() => this.refresh(), tell);
    else invoke("mkdir", { tab: active, path }).catch(tell);
  }

  async deleteSelected() {
    const sel = this.selectedEntries();
    if (!sel.length || (this.kind === "local" && this.st.path === "")) return;
    const where = this.kind === "local" ? " (to the Recycle Bin)" : "";
    const q = sel.length === 1 ? `Delete "${sel[0].name}"${sel[0].dir ? " and everything in it" : ""}${where}?` : `Delete ${sel.length} items${where}?`;
    if (!(await ask(q))) return;
    if (this.kind === "local") {
      invoke("local_delete", { paths: sel.map((e) => this.join(this.st.path, e.name)) }).then(() => this.refresh(), (e) => (tell(e), this.refresh()));
    } else invoke("delete", { tab: active, dir: this.st.path, entries: sel }).catch(tell);
  }

  startRename() {
    const sel = this.selectedEntries();
    if (sel.length !== 1 || (this.kind === "local" && this.st.path === "")) {
      setMessage("Select one file or folder to rename");
      return;
    }
    const e = sel[0];
    const row = this.rows.children[this.rowOf(e)];
    const cell = row?.querySelector(".name");
    if (!cell) return;
    cell.innerHTML = this.icon(e) + `<input class="rename" spellcheck="false">`;
    const input = cell.querySelector("input");
    input.value = e.name;
    input.focus();
    const dot = e.dir ? -1 : e.name.lastIndexOf(".");
    input.setSelectionRange(0, dot > 0 ? dot : e.name.length);
    let done = false;
    const finish = (commit) => {
      if (done) return;
      done = true;
      const name = input.value.trim();
      this.render();
      this.list.focus();
      if (!commit || !name || name === e.name || /[\\/:*?"<>|]/.test(name) && this.kind === "local" || /[\\/]/.test(name)) return;
      this.st.selectNext = name;
      const from = this.join(this.st.path, e.name), to = this.join(this.st.path, name);
      if (this.kind === "local") invoke("local_rename", { from, to }).then(() => this.refresh(), tell);
      else invoke("rename", { tab: active, from, to }).catch(tell);
    };
    input.addEventListener("keydown", (ev) => {
      ev.stopPropagation();
      if (ev.key === "Enter") finish(true);
      if (ev.key === "Escape") finish(false);
    });
    input.addEventListener("mousedown", (ev) => ev.stopPropagation());
    input.addEventListener("blur", () => finish(true));
  }

  // clicks, multi-selection, keyboard
  wire() {
    const rowIndex = (ev) => {
      const r = ev.target.closest(".row-e");
      return r && this.rows.contains(r) ? +r.dataset.i : -1;
    };
    this.el.addEventListener("mousedown", () => setActivePane(this));
    this.rows.addEventListener("mousedown", (ev) => {
      if (ev.target.tagName === "INPUT") return;
      const i = rowIndex(ev);
      if (i < 0) return;
      const e = this.entryAt(i);
      const st = this.st;
      st.focus = i;
      if (ev.button === 2 && e && st.selected.has(e.name)) return; // right-click keeps the selection
      if (ev.shiftKey && st.anchor >= 0) {
        const [a, b] = [Math.min(st.anchor, i), Math.max(st.anchor, i)];
        const names = [];
        for (let k = a; k <= b; k++) if (this.entryAt(k)) names.push(this.entryAt(k).name);
        this.setSelection(names);
      } else if (ev.ctrlKey) {
        if (e) {
          const s = new Set(st.selected);
          s.has(e.name) ? s.delete(e.name) : s.add(e.name);
          this.setSelection([...s]);
        }
        st.anchor = i;
      } else if (!(e && st.selected.has(e.name) && ev.button === 0)) {
        this.setSelection(e ? [e.name] : []);
        st.anchor = i;
      } else this.setSelection([...st.selected]);
      if (ev.button === 0 && e && st.selected.has(e.name) && !(this.kind === "local" && st.path === "")) startDrag(this, ev);
    });
    this.rows.addEventListener("click", (ev) => {
      if (ev.ctrlKey || ev.shiftKey || ev.target.tagName === "INPUT") return;
      const i = rowIndex(ev);
      const e = i >= 0 ? this.entryAt(i) : null;
      if (e && this.st.selected.size > 1 && !dragMoved) this.setSelection([e.name]); // a plain click on a multi-selection
    });
    this.rows.addEventListener("dblclick", (ev) => {
      const i = rowIndex(ev);
      if (i >= 0 && ev.target.tagName !== "INPUT") this.openRow(i);
    });
    this.list.addEventListener("mousedown", (ev) => {
      if (ev.target === this.list || ev.target === this.rows) this.setSelection([]);
    });
    this.list.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      setActivePane(this);
      showMenu(this, ev.clientX, ev.clientY);
    });
    this.list.addEventListener("keydown", (ev) => {
      if (anyModal()) return;
      const n = this.rowCount();
      const st = this.st;
      const move = (i) => {
        st.focus = Math.max(0, Math.min(n - 1, i));
        const e = this.entryAt(st.focus);
        this.setSelection(e ? [e.name] : []);
        st.anchor = st.focus;
        this.rows.children[st.focus]?.scrollIntoView({ block: "nearest" });
      };
      if (ev.key === "ArrowDown") move(st.focus + 1);
      else if (ev.key === "ArrowUp") move(st.focus - 1);
      else if (ev.key === "Home") move(0);
      else if (ev.key === "End") move(n - 1);
      else if (ev.key === "Enter") this.openRow(st.focus);
      else if (ev.key === "Backspace") this.up();
      else if (ev.key === "Delete") this.deleteSelected();
      else if (ev.key === "F2") this.startRename();
      else if (ev.key === "F5") this.refresh();
      else if (ev.key === "Tab") (this === localPane ? remotePane : localPane).focus();
      else if (ev.key === "a" && ev.ctrlKey) this.setSelection(st.entries.map((e) => e.name));
      else if (ev.key === "ContextMenu") showMenu(this, this.el.getBoundingClientRect().left + 40, this.el.getBoundingClientRect().top + 80);
      else return;
      ev.preventDefault();
    });
    this.list.addEventListener("focus", () => setActivePane(this));
  }

  focus() {
    this.list.focus();
    setActivePane(this);
  }
}

const localPane = new Pane($("paneLocal"), "local");
const remotePane = new Pane($("paneRemote"), "remote");
activePane = localPane;

function setActivePane(p) {
  if (activePane === p) return;
  activePane = p;
  localPane.el.classList.toggle("active", p === localPane);
  remotePane.el.classList.toggle("active", p === remotePane);
  updateToolbar();
}

// ---------------------------------------------------------------------------
// moving files: the toolbar, and dragging between the sides or out to Explorer

function uploadSelected(to) {
  const t = tab();
  const sel = localPane.selectedEntries();
  if (!t || !sel.length || localPane.st.path === "") return;
  invoke("upload", { tab: active, paths: sel.map((e) => localPane.join(localPane.st.path, e.name)), dir: to ?? remotePane.st.path }).catch(tell);
}
function downloadSelected(to) {
  const t = tab();
  const sel = remotePane.selectedEntries();
  if (!t || !sel.length) return;
  const dest = to ?? localPane.st.path;
  if (dest === "") invoke("download_to", { tab: active, dir: remotePane.st.path, entries: sel });
  else invoke("download", { tab: active, dir: remotePane.st.path, entries: sel, to: dest }).catch(tell);
  t.refreshLocal = true; // the worker lists the server folder again once the job is done
}

let dragMoved = false;
let dropTarget = null; // { pane, row }
function startDrag(from, down) {
  dragMoved = false;
  const move = (ev) => {
    if (!dragMoved && Math.abs(ev.clientX - down.clientX) + Math.abs(ev.clientY - down.clientY) < 6) return;
    dragMoved = true;
    document.body.style.cursor = "copy";
    // past the window's edge with server items: Windows' own drag takes over
    // (Explorer). The page keeps getting moves while the button is held, so the
    // position says it, not a mouseout.
    const r = 2;
    if (ev.clientX < r || ev.clientY < r || ev.clientX >= innerWidth - r || ev.clientY >= innerHeight - r) return out();
    const over = paneAt(ev.clientX, ev.clientY);
    setDropTarget(over && over.pane !== from ? over : null);
  };
  const out = () => {
    if (from.kind !== "remote") return;
    stop();
    draggingOut = true;
    invoke("drag_out", { tab: active, dir: from.st.path, entries: from.selectedEntries() });
  };
  const leave = (ev) => {
    if (dragMoved && !ev.relatedTarget) out();
  };
  const up = () => {
    const target = dropTarget;
    stop();
    if (!dragMoved || !target) return;
    const into = target.row ? target.pane.join(target.pane.st.path, target.row) : target.pane.st.path;
    if (from.kind === "local") uploadSelected(into);
    else downloadSelected(into);
  };
  const stop = () => {
    setDropTarget(null);
    document.body.style.cursor = "";
    removeEventListener("mousemove", move);
    removeEventListener("mouseup", up);
    document.removeEventListener("mouseout", leave);
  };
  addEventListener("mousemove", move);
  addEventListener("mouseup", up);
  document.addEventListener("mouseout", leave);
}
listen("dragout-done", () => (draggingOut = false));

// the pane (and folder row) under a point
function paneAt(x, y) {
  const el = document.elementFromPoint(x, y);
  const pane = [localPane, remotePane].find((p) => p.el.contains(el));
  if (!pane || (pane === remotePane && !tab()) || (pane === localPane && pane.st.path === "")) return null;
  const r = el.closest?.(".row-e");
  let row = null;
  if (r && pane.rows.contains(r)) {
    const e = pane.entryAt(+r.dataset.i);
    if (e && e.dir) row = e.name;
  }
  return { pane, row, rowEl: row ? r : null };
}
function setDropTarget(t) {
  if (dropTarget?.pane === t?.pane && dropTarget?.row === t?.row) return;
  dropTarget?.pane.el.classList.remove("dropping");
  dropTarget?.rowEl?.classList.remove("drop");
  dropTarget = t;
  t?.pane.el.classList.add("dropping");
  t?.rowEl?.classList.add("drop");
}

// in from Explorer: onto the server side uploads (into a folder row when dropped on one)
window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  if (draggingOut || anyModal()) return;
  const at = () => {
    const r = window.devicePixelRatio || 1;
    const t = paneAt(p.position.x / r, p.position.y / r);
    return t && t.pane === remotePane ? t : null;
  };
  if (p.type === "over" || p.type === "enter") setDropTarget(at());
  else if (p.type === "leave") setDropTarget(null);
  else if (p.type === "drop") {
    const t = at();
    setDropTarget(null);
    if (!t) {
      if (tab()) setMessage("Drop files on the server side to upload them");
      return;
    }
    invoke("upload", { tab: active, paths: p.paths, dir: t.row ? remotePane.join(remotePane.st.path, t.row) : remotePane.st.path }).catch(tell);
  }
});

// ---------------------------------------------------------------------------
// toolbar, right-click menu

// the protected 3DS paths (read-only on Cement while the protection is on)
let protectedPaths = [];
let freePaths = []; // inside a protected folder, still free (Luma's screenshots and crash dumps)
const inside = (a, b) => {
  const x = a.replace(/\/+$/, "").toLowerCase(), y = b.replace(/\/+$/, "").toLowerCase();
  return x === y || x.startsWith(y + "/");
};
function readOnly(path) {
  const t = tab();
  return protect3ds && !!t?.cement && protectedPaths.some((p) => inside(path, p)) && !freePaths.some((p) => inside(path, p));
}
// the Windows folder: read-only on the PC side (null when Chomp was started to allow it)
let windowsDir = null;
const winNorm = (p) => p.replaceAll("/", "\\").replace(/\\+$/, "").toLowerCase();
function localReadOnly(path) {
  if (!windowsDir || !path) return false;
  const a = winNorm(path), w = winNorm(windowsDir);
  return a === w || a.startsWith(w + "\\");
}
function localTouchesWindows(pane) {
  if (pane !== localPane || !windowsDir) return false;
  const w = winNorm(windowsDir);
  return pane.selectedEntries().some((e) => {
    const a = winNorm(pane.join(pane.st.path, e.name));
    return a === w || a.startsWith(w + "\\") || w.startsWith(a + "\\");
  });
}

// something selected that is, or holds, a protected path
function touchesProtected(pane) {
  if (pane !== remotePane) return false;
  return pane.selectedEntries().some((e) => {
    const p = pane.join(pane.st.path, e.name);
    return readOnly(p) || (protect3ds && tab()?.cement && protectedPaths.some((x) => inside(x, p)));
  });
}

function updateToolbar() {
  const t = tab();
  const on = t && t.state === "connected";
  const ro = readOnly(remotePane.st.path);
  const lro = localReadOnly(localPane.st.path);
  $("btnUpload").disabled = !on || ro || !localPane.selectedEntries().length || localPane.st.path === "";
  $("btnDownload").disabled = !on || lro || !remotePane.selectedEntries().length;
  const p = activePane;
  const ready = p === localPane ? p.st.path !== "" && !lro : on && !ro;
  const locked = touchesProtected(p) || localTouchesWindows(p);
  $("btnMkdir").disabled = !ready;
  $("btnRename").disabled = !ready || p.selectedEntries().length !== 1 || locked;
  $("btnDelete").disabled = !ready || !p.selectedEntries().length || locked;
  $("roTag").hidden = !ro;
  $("roTagLocal").hidden = !lro;
}
$("btnUpload").onclick = () => uploadSelected();
$("btnDownload").onclick = () => downloadSelected();
$("btnMkdir").onclick = () => activePane.newFolder();
$("btnRename").onclick = () => activePane.startRename();
$("btnDelete").onclick = () => activePane.deleteSelected();
$("btnRefresh").onclick = () => activePane.refresh();
$("btnReconnect").onclick = () => tab() && invoke("reconnect", { tab: active });
$("btnCancel").onclick = () => tab() && invoke("cancel", { tab: active });

function showMenu(pane, x, y) {
  const sel = pane.selectedEntries();
  const items = [];
  const t = tab();
  const local = pane.kind === "local";
  if (sel.length === 1) items.push(["Open", "Enter", () => pane.openRow(pane.rowOf(sel[0]))]);
  if (sel.length && !(local && pane.st.path === "")) {
    if (local && t) items.push(["Upload", "", () => uploadSelected()]);
    if (!local && sel.length === 1 && !sel[0].dir && /\.cia$/i.test(sel[0].name) && t?.cement?.can_install)
      items.push(["Install on the 3DS", "", () => invoke("install_cia", { tab: active, path: pane.join(pane.st.path, sel[0].name) }).catch(tell)]);
    if (!local) {
      items.push(["Download", "", () => downloadSelected()]);
      items.push(["Download to...", "", () => invoke("download_to", { tab: active, dir: pane.st.path, entries: sel })]);
    }
    items.push(["Rename", "F2", () => pane.startRename(), sel.length !== 1]);
    items.push(["Delete", "Del", () => pane.deleteSelected()]);
    items.push(null);
  }
  if (!(local && pane.st.path === "")) items.push(["New folder", "", () => pane.newFolder()]);
  items.push(["Refresh", "F5", () => pane.refresh()]);
  const m = $("menu");
  m.innerHTML = "";
  for (const it of items) {
    if (!it) {
      m.appendChild(document.createElement("hr"));
      continue;
    }
    const d = document.createElement("div");
    d.innerHTML = `<span>${it[0]}</span><span class="key">${it[1]}</span>`;
    if (it[3]) d.className = "off";
    d.onclick = () => {
      hideMenu();
      it[2]();
    };
    m.appendChild(d);
  }
  m.hidden = false;
  m.style.left = Math.min(x, innerWidth - m.offsetWidth - 4) + "px";
  m.style.top = Math.min(y, innerHeight - m.offsetHeight - 4) + "px";
}
const hideMenu = () => ($("menu").hidden = true);
document.addEventListener("contextmenu", (ev) => {
  if (ev.target.tagName !== "INPUT") ev.preventDefault();
});
document.addEventListener("mousedown", (ev) => {
  if (!$("menu").contains(ev.target)) hideMenu();
});
document.addEventListener("keydown", (ev) => {
  if (ev.key === "F5" || (ev.ctrlKey && ["r", "p", "f", "g", "u", "j"].includes(ev.key))) ev.preventDefault(); // no browser shortcuts
  if (ev.ctrlKey && ev.key === "s" && !anyModal()) {
    ev.preventDefault();
    if (tab()?.cement) $("btnSend").click();
  }
  if (ev.ctrlKey && ev.key === "w" && !anyModal() && active) {
    ev.preventDefault();
    closeTab(active);
  }
});

// ---------------------------------------------------------------------------
// the colour: every red of the palette follows the accent's hue

const DEFAULT_ACCENT = "b8283c";
const PALETTE = {
  "--accent-hi": "#cc3a48", "--hi": "#d8676a", "--soft": "#e9a9a3", "--bg": "#0e0a0a", "--blob": "#1a1112",
  "--surface": "#1d1414", "--well": "#120c0c", "--quick": "#171010", "--track": "#2a1516", "--sel": "#3d1418", "--edge": "#4d1d21",
  "--line": "#2e1f1f", "--input": "#3a2828", "--text": "#f2e7e4", "--dim": "#a8928e", "--mute": "#594544",
};
const ACCENTS = ["b8283c", "951d2e", "d0503a", "c27a1e", "3f9a5a", "1f8f8a", "2f6fd0", "7a4cc4", "b8388f"];
function toHsl(hex) {
  const n = parseInt(hex.replace("#", ""), 16);
  const r = (n >> 16) / 255, g = ((n >> 8) & 255) / 255, b = (n & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b), l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min, s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s, l];
}
const hsl = (h, s, l) => `hsl(${h.toFixed(1)} ${(s * 100).toFixed(1)}% ${(l * 100).toFixed(1)}%)`;
let accent = DEFAULT_ACCENT;
function applyAccent(hex) {
  const root = document.documentElement.style;
  if (hex === DEFAULT_ACCENT) {
    // the hand-picked palette exactly
    for (const name of ["--accent", ...Object.keys(PALETTE)]) root.removeProperty(name);
    return;
  }
  const [h, sat] = toHsl(hex);
  root.setProperty("--accent", "#" + hex);
  for (const [name, value] of Object.entries(PALETTE)) {
    const [h0, s0, l0] = toHsl(value);
    // the default palette's lightness on the accent's hue; a greyer accent gives
    // greyer surfaces
    root.setProperty(name, hsl(h, s0 * Math.min(1, sat / 0.64), l0));
  }
}

// ---------------------------------------------------------------------------
// title bar

$("winMin").onclick = () => appWindow.minimize();
$("btnSettings").onclick = openPrefs;

let pickedAccent = accent;
function drawSwatches() {
  const box = $("accents");
  box.innerHTML = "";
  for (const a of ACCENTS) {
    const b = document.createElement("button");
    b.className = "swatch";
    b.style.background = "#" + a;
    b.setAttribute("role", "radio");
    b.setAttribute("aria-label", "#" + a);
    b.setAttribute("aria-checked", String(a === pickedAccent));
    b.onclick = () => pickAccent(a);
    box.appendChild(b);
  }
  $("accentCustom").value = "#" + pickedAccent;
  $("accentHex").textContent = "#" + pickedAccent;
}
function pickAccent(a) {
  pickedAccent = a.replace("#", "").toLowerCase();
  applyAccent(pickedAccent); // shown at once, kept on Save
  drawSwatches();
}
let localStart = "home";
function syncLocalStart() {
  $("localDirRow").hidden = $("localStart").value !== "dir";
}
$("localStart").onchange = syncLocalStart;
$("localBrowse").onclick = async () => {
  const d = await invoke("browse_folder", { title: "Folder the PC side opens in" });
  if (d) $("localDir").value = d;
};
let protect3ds = true;
let checkUpdates = true;
let bundles = false;

async function lookForUpdate() {
  const u = await invoke("check_update").catch(() => null);
  if (!u) return;
  const chip = $("updateChip");
  chip.textContent = `${u.version} is out`;
  chip.title = "A new Chomp is on GitHub";
  chip.hidden = false;
  chip.onclick = async () => {
    const go = await ask(
      `Chomp ${u.version} is available (you have ${$("version").textContent}).\n\nThis opens its release page on GitHub in your browser.`,
      "Open GitHub", "Not now", "New version"
    );
    if (go) invoke("open_release", { url: u.url }).catch(tell);
  };
}
$("protect3ds").addEventListener("change", async () => {
  if ($("protect3ds").checked) return;
  $("prefs").hidden = true;
  const ok = await ask(
    "Without it, Chomp lets you delete, rename or replace the 3DS files that keep it working: " +
      "/Nintendo 3DS (installed games and saves), /boot.firm (what the console boots), " +
      "/luma, /gm9 (NAND backups) and /boot.3dsx." +
      "\n\nA mistake there can lose your games and saves, " +
      "or stop the 3DS from booting into its custom firmware. Turn the protection off?",
    "Turn it off", "Keep it on", "Protect important 3DS files"
  );
  $("prefs").hidden = false;
  $("protect3ds").checked = !ok;
});
function showPrefsTab(name) {
  for (const b of document.querySelectorAll("#prefs .seg button")) b.setAttribute("aria-selected", String(b.dataset.tab === name));
  for (const p of document.querySelectorAll("#prefs .prefs-tab")) p.hidden = p.dataset.tab !== name;
}
for (const b of document.querySelectorAll("#prefs .seg button")) b.onclick = () => showPrefsTab(b.dataset.tab);
function openPrefs() {
  showPrefsTab("design");
  pickedAccent = accent;
  $("protect3ds").checked = protect3ds;
  $("checkUpdates").checked = checkUpdates;
  $("bundles").checked = bundles;
  drawSwatches();
  const custom = localStart !== "home" && localStart !== "last";
  $("localStart").value = custom ? "dir" : localStart;
  $("localDir").value = custom ? localStart : localPane.st.path;
  syncLocalStart();
  $("prefs").hidden = false;
}
$("accentCustom").addEventListener("input", (ev) => pickAccent(ev.target.value));
$("prefsCancel").onclick = () => {
  applyAccent(accent);
  $("prefs").hidden = true;
};
$("prefsOk").onclick = async () => {
  const start = $("localStart").value === "dir" ? $("localDir").value.trim() : $("localStart").value;
  try {
    await invoke("set_local_start", { start });
    localStart = start;
  } catch (e) {
    $("prefs").hidden = true;
    await tell(e);
    $("prefs").hidden = false;
    return;
  }
  protect3ds = $("protect3ds").checked;
  invoke("set_protect_3ds", { on: protect3ds });
  updateToolbar();
  if ($("bundles").checked !== bundles) {
    bundles = $("bundles").checked;
    invoke("set_bundles", { on: bundles });
  }
  if ($("checkUpdates").checked !== checkUpdates) {
    checkUpdates = $("checkUpdates").checked;
    invoke("set_check_updates", { on: checkUpdates });
    if (checkUpdates) lookForUpdate();
    else $("updateChip").hidden = true;
  }
  try {
    await invoke("set_accent", { accent: pickedAccent });
    accent = pickedAccent;
  } catch (e) {
    applyAccent(accent);
    tell(e);
  }
  $("prefs").hidden = true;
};
$("winMax").onclick = () => appWindow.toggleMaximize();
$("winClose").onclick = () => appWindow.close();

// ---------------------------------------------------------------------------
// sites and tabs

const PROTO = { ftp: "FTP", sftp: "SFTP", ftps: "FTPS", "ftps-implicit": "FTPS" };

function tabOfSite(id) {
  return [...tabs.values()].find((t) => t.saved_id === id);
}

function dotClass(t) {
  if (!t) return "off";
  if (t.state === "connected") return t.cement ? "cement" : "on";
  if (t.state === "refused") return "off";
  return "wait";
}

function renderSites() {
  const list = $("siteList");
  list.innerHTML = "";
  for (const s of sites) {
    const t = tabOfSite(s.id);
    const d = document.createElement("div");
    d.className = "site" + (t && t.id === active ? " open" : "");
    d.innerHTML =
      `<span class="dot ${dotClass(t)}"></span>` +
      `<span class="txt"><b>${escapeHtml(s.name)}</b><span class="sub">${PROTO[s.protocol] || s.protocol} · ${escapeHtml(s.host)}</span></span>` +
      (t?.cement ? '<span class="tag">3DS</span>' : "") +
      `<button class="edit" title="Edit" aria-label="Edit ${escapeHtml(s.name)}">${PENCIL}</button>`;
    d.onclick = (ev) => {
      if (ev.target.closest(".edit")) return editSite(s);
      const open = tabOfSite(s.id);
      if (open) activate(open.id);
      else openSite(s.id);
    };
    list.appendChild(d);
  }
}

function renderTabs() {
  const bar = $("tabs");
  bar.innerHTML = "";
  for (const t of tabs.values()) {
    const d = document.createElement("div");
    d.className = "tab" + (t.id === active ? " active" : "");
    d.innerHTML = `<span class="dot ${dotClass(t)}"></span><span>${escapeHtml(t.name)}</span><button class="x" title="Close (Ctrl+W)" aria-label="Close">${CROSS}</button>`;
    d.onclick = (ev) => (ev.target.closest(".x") ? closeTab(t.id) : activate(t.id));
    d.onauxclick = (ev) => ev.button === 1 && closeTab(t.id);
    bar.appendChild(d);
  }
  $("work").classList.toggle("notabs", tabs.size === 0);
  $("empty").hidden = tabs.size !== 0;
}

function addTab(info) {
  const t = {
    id: info.id,
    saved_id: info.saved_id,
    name: info.name,
    host: info.host,
    protocol: info.protocol,
    state: "waiting",
    cement: null,
    remote: newPaneState(info.start || ""),
    status: { part0: `Connecting to ${info.host}...`, part1: "" },
    progress: { active: false, permille: 0, name: "", detail: "" },
    refused: null,
  };
  tabs.set(t.id, t);
  activate(t.id);
  rememberTabs();
}

async function openSite(id) {
  try {
    addTab(await invoke("open_site", { id }));
  } catch (e) {
    tell(e);
  }
}

function closeTab(id) {
  const t = tabs.get(id);
  if (!t) return;
  invoke("close_tab", { tab: id });
  tabs.delete(id);
  if (active === id) {
    const rest = [...tabs.keys()];
    active = null;
    if (rest.length) activate(rest[rest.length - 1]);
    else showTab();
  } else {
    renderTabs();
    renderSites();
  }
  rememberTabs();
}

function rememberTabs() {
  store("openSites", [...tabs.values()].filter((t) => t.saved_id).map((t) => t.saved_id));
}

function activate(id) {
  active = id;
  showTab();
}

// everything that follows the tab on screen
function showTab() {
  const t = tab();
  renderTabs();
  renderSites();
  $("remoteTitle").textContent = t ? t.name : "Server";
  if (t) {
    remotePane.attach(t.remote);
    if (t.state === "connected" && t.remote.shown === null && t.remote.path) remotePane.refresh();
  } else remotePane.attach(newPaneState("/"));
  drawOffline();
  drawStatus();
  drawProgress();
  drawCement();
  updateToolbar();
}

function drawOffline() {
  const t = tab();
  const box = remotePane.el.querySelector(".offline");
  if (!t || t.state === "connected") {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  const text = box.querySelector(".offline-text");
  if (t.state === "refused") {
    text.innerHTML = `${escapeHtml(t.refused?.message || "The server refused the connection.")}<br><br><button class="pill small accent" id="offRetry">Try again</button>`;
    $("offRetry").onclick = () => (t.refused ? showRefused(t) : invoke("reconnect", { tab: t.id }));
  } else {
    const cementHint = t.protocol === "ftp" ? "<br>For a 3DS: start Cement, then check the IP and port it shows." : "";
    text.innerHTML = `<span><span class="dot wait"></span>&nbsp; Connecting to ${escapeHtml(t.host)}...${cementHint}</span>`;
  }
}

// quick connect
$("qc").addEventListener("keydown", async (ev) => {
  if (ev.key !== "Enter") return;
  const url = $("qc").value.trim();
  if (!url) return;
  try {
    addTab(await invoke("open_url", { url, pass: "" }));
    $("qc").value = "";
  } catch (e) {
    tell(e);
  }
});

// ---------------------------------------------------------------------------
// the site editor

let editing = null;
let passDirty = false, phraseDirty = false;
function syncProto() {
  const p = $("sProto").value;
  const def = { ftp: 21, sftp: 22, ftps: 21, "ftps-implicit": 990 }[p];
  $("sPort").placeholder = def;
  for (const el of document.querySelectorAll(".sftp-only")) el.hidden = p !== "sftp";
  $("sUser").placeholder = p === "ftp" ? "empty: anonymous (Cement)" : "";
}
$("sProto").onchange = syncProto;
function editSite(s) {
  editing = s || null;
  $("siteDlgTitle").textContent = s ? "Edit site" : "New site";
  $("sName").value = s?.name || "";
  $("sProto").value = s?.protocol || "ftp";
  $("sHost").value = s?.host || "";
  $("sPort").value = s ? s.port : "";
  $("sUser").value = s?.user || "";
  $("sPass").value = "";
  $("sPass").placeholder = s?.has_pass ? "saved (type to change)" : s?.needs_password ? "type it again on this PC" : "";
  $("sKey").value = s?.key || "";
  $("sPassphrase").value = "";
  $("sPassphrase").placeholder = s?.has_passphrase ? "saved (type to change)" : "";
  $("sStart").value = s?.start_dir || "";
  passDirty = phraseDirty = false;
  $("siteDelete").hidden = !s;
  syncProto();
  if (!s) $("sPort").value = "";
  $("siteDlg").hidden = false;
  $("sName").focus();
}
$("sPass").oninput = () => (passDirty = true);
$("sPassphrase").oninput = () => (phraseDirty = true);
$("sKeyBrowse").onclick = async () => {
  const f = await invoke("browse_file", { title: "Private key file" });
  if (f) $("sKey").value = f;
};
async function saveSite(connect) {
  const s = {
    id: editing?.id || 0,
    name: $("sName").value,
    protocol: $("sProto").value,
    host: $("sHost").value,
    port: parseInt($("sPort").value, 10) || 0,
    user: $("sUser").value,
    pass: passDirty || !editing ? $("sPass").value : null,
    key: $("sProto").value === "sftp" ? $("sKey").value : "",
    passphrase: phraseDirty || !editing ? $("sPassphrase").value : null,
    start_dir: $("sStart").value,
  };
  // Cement listens on 5000: a plain FTP site without a port is most likely the 3DS
  if (!s.port && s.protocol === "ftp" && !s.user.trim()) s.port = 5000;
  let list;
  try {
    list = await invoke("site_save", { s });
  } catch (e) {
    $("siteDlg").hidden = true;
    await tell(e);
    $("siteDlg").hidden = false;
    return;
  }
  const isNew = !editing;
  const before = new Set(sites.map((x) => x.id));
  sites = list;
  $("siteDlg").hidden = true;
  const id = isNew ? sites.find((x) => !before.has(x.id))?.id : editing.id;
  renderSites();
  if (connect && id) {
    const open = tabOfSite(id);
    if (open) {
      closeTab(open.id);
    }
    openSite(id);
  }
}
$("siteSave").onclick = () => saveSite(false);
$("siteConnect").onclick = () => saveSite(true);
$("siteCancel").onclick = () => ($("siteDlg").hidden = true);
$("siteDelete").onclick = async () => {
  const s = editing;
  $("siteDlg").hidden = true;
  if (!(await ask(`Delete the site "${s.name}"?`))) {
    $("siteDlg").hidden = false;
    return;
  }
  sites = await invoke("site_delete", { id: s.id });
  renderSites();
};
$("btnNewSite").onclick = () => editSite(null);
$("btnEmptyNew").onclick = () => editSite(null);
for (const id of ["sName", "sHost", "sPort", "sUser", "sPass", "sKey", "sPassphrase", "sStart"])
  $(id).addEventListener("keydown", (ev) => ev.key === "Enter" && saveSite(true));

// ---------------------------------------------------------------------------
// refused connections: password, certificate, host key

function showRefused(t) {
  const r = t.refused;
  if (!r) return;
  const trust = !!r.fingerprint;
  const changed = trust && /changed/i.test(r.message);
  $("rfTitle").textContent = trust ? (changed ? "The host key changed" : r.protocol === "sftp" ? "New SSH server" : "Certificate not trusted") : `Log in to ${r.host}`;
  $("rfText").textContent = trust
    ? r.message + (r.protocol === "sftp" ? "\n\nIts host key fingerprint:" : "\n\nIts certificate fingerprint (SHA-256):")
    : r.message;
  $("rfFinger").hidden = !trust;
  $("rfFinger").textContent = r.fingerprint;
  $("rfLogin").hidden = trust;
  $("rfSaveRow").hidden = trust || !t.saved_id;
  const site = sites.find((s) => s.id === t.saved_id);
  $("rfUser").value = site?.user || "";
  $("rfPass").value = "";
  $("rfPassLabel").textContent = site?.key ? "Passphrase" : "Password";
  $("rfOk").textContent = trust ? (changed ? "Trust the new key" : "Trust and connect") : "Connect";
  $("rfOk").classList.toggle("danger", changed);
  $("refusedDlg").hidden = false;
  (trust ? $("rfCancel") : $("rfPass")).focus();
  $("rfOk").onclick = () => {
    $("refusedDlg").hidden = true;
    t.refused = null;
    if (trust) invoke("trust", { tab: t.id, fingerprint: r.fingerprint });
    else invoke("login", { tab: t.id, user: $("rfUser").value, pass: $("rfPass").value, save: $("rfSave").checked && !!t.saved_id });
  };
  $("rfCancel").onclick = () => ($("refusedDlg").hidden = true);
}
$("rfPass").addEventListener("keydown", (ev) => ev.key === "Enter" && $("rfOk").click());
listen("refused", (ev) => {
  const r = ev.payload;
  const t = tabs.get(r.tab);
  if (!t) return;
  t.refused = r;
  if (r.tab !== active) activate(r.tab);
  drawOffline();
  showRefused(t);
});

// ---------------------------------------------------------------------------
// dialogs

const anyModal = () => [...document.querySelectorAll(".modal")].some((m) => !m.hidden);

function ask(text, yes = "Yes", no = "No", title = "Chomp") {
  return new Promise((resolve) => {
    $("askTitle").textContent = title;
    $("askText").textContent = String(text);
    $("askYes").textContent = yes;
    $("askNo").textContent = no;
    $("askNo").hidden = !no;
    $("ask").hidden = false;
    $("askYes").focus();
    const done = (v) => {
      $("ask").hidden = true;
      $("askYes").onclick = $("askNo").onclick = null;
      activePane.list.focus();
      resolve(v);
    };
    $("askYes").onclick = () => done(true);
    $("askNo").onclick = () => done(false);
  });
}
const tell = (text) => ask(text, "OK", "");

document.addEventListener("keydown", (ev) => {
  if (ev.key !== "Escape") return;
  if (!$("ask").hidden) $("askNo").hidden ? $("askYes").click() : $("askNo").click();
  else if (!$("picker").hidden) $("pickCancel").click();
  else if (!$("refusedDlg").hidden) $("rfCancel").click();
  else if (!$("sendback").hidden) $("sbKeep").click();
  else if (!$("settings").hidden) $("settingsCancel").click();
  else if (!$("prefs").hidden) $("prefsCancel").click();
  else if (!$("cementLogin").hidden) $("clCancel").click();
  else if (!$("siteDlg").hidden) $("siteCancel").click();
});

// Send to PC settings
function updateSendSection() {
  const on = tab()?.cement && tab().state === "connected";
  for (const id of ["btnAddPick"]) $(id).disabled = !on;
  $("sendNote").textContent = on ? "" : "Connect to Cement to pick from the 3DS.";
}
function addSendItem(p) {
  const list = $("sendItems");
  if (![...list.options].some((o) => o.value === p)) list.add(new Option(p, p));
}
function openSettings() {
  $("sendDir").value = settings.send_dir;
  $("replace").checked = settings.send_replace;
  $("sendItems").innerHTML = "";
  settings.send_items.forEach(addSendItem);
  updateSendSection();
  $("settings").hidden = false;
}
$("btnSendSettings").onclick = openSettings;
listen("open-send-settings", openSettings);
$("btnAddPath").onclick = async () => {
  const text = $("newPath").value;
  const p = await invoke("normalize", { path: text });
  if (text.trim() && p.length > 1) {
    addSendItem(p);
    $("newPath").value = "";
  }
};
$("newPath").addEventListener("keydown", (ev) => ev.key === "Enter" && $("btnAddPath").click());
$("btnRemoveItem").onclick = () => {
  for (const o of [...$("sendItems").selectedOptions]) o.remove();
};
$("btnBrowse").onclick = async () => {
  const d = await invoke("browse_folder", { title: "Send to PC folder" });
  if (d) $("sendDir").value = d;
};
$("replace").addEventListener("change", async () => {
  if (!$("replace").checked) return;
  $("settings").hidden = true;
  const ok = await ask(
    "Each Send to PC will first empty the PC folder (its current files and folders go to the Recycle Bin), " +
      "then copy the 3DS items into it.\n\nOnly use a folder dedicated to this. Turn it on?",
    "Yes", "No", "Replace PC folder contents"
  );
  $("settings").hidden = false;
  $("replace").checked = ok;
});
$("settingsOk").onclick = async () => {
  const s = {
    send_dir: $("sendDir").value.trim(),
    send_items: [...$("sendItems").options].map((o) => o.value),
    send_replace: $("replace").checked,
  };
  try {
    await invoke("save_settings", { s });
  } catch (err) {
    $("settings").hidden = true;
    await tell(err);
    $("settings").hidden = false;
    return;
  }
  settings = s;
  $("settings").hidden = true;
};
$("settingsCancel").onclick = () => ($("settings").hidden = true);

// 3DS picker (Send to PC > Add from 3DS...)
let pick = { tab: null, cwd: "/", entries: [], sel: new Set() };
const pickParent = (p) => remotePane.parent.call({ kind: "remote" }, p);
const pickJoin = (d, n) => remotePane.join.call({ kind: "remote" }, d, n);
function pickNavigate(path) {
  pick = { tab: active, cwd: path, entries: [], sel: new Set() };
  $("pickPath").value = path;
  $("pickRows").innerHTML = "";
  invoke("picker_list", { tab: active, path });
}
function pickRender() {
  const rows = [];
  if (pick.cwd !== "/") rows.push(`<div class="row-e" data-i="-1"><span class="name">${ICON.dir}<span>..</span></span></div>`);
  pick.entries.forEach((e, i) =>
    rows.push(`<div class="row-e${pick.sel.has(i) ? " sel" : ""}" data-i="${i}"><span class="name">${e.dir ? ICON.dir : ICON.file}<span>${escapeHtml(e.name)}</span></span></div>`)
  );
  $("pickRows").innerHTML = rows.join("");
}
$("pickRows").addEventListener("mousedown", (ev) => {
  const r = ev.target.closest(".row-e");
  if (!r) return;
  const i = +r.dataset.i;
  if (i < 0) return;
  if (ev.ctrlKey) pick.sel.has(i) ? pick.sel.delete(i) : pick.sel.add(i);
  else pick.sel = new Set([i]);
  pickRender();
});
$("pickRows").addEventListener("dblclick", (ev) => {
  const r = ev.target.closest(".row-e");
  if (!r) return;
  const i = +r.dataset.i;
  if (i < 0) pickNavigate(pickParent(pick.cwd));
  else if (pick.entries[i].dir) pickNavigate(pickJoin(pick.cwd, pick.entries[i].name));
});
$("pickUp").onclick = () => pick.cwd !== "/" && pickNavigate(pickParent(pick.cwd));
$("btnAddPick").onclick = () => {
  $("settings").hidden = true;
  $("picker").hidden = false;
  pickNavigate("/");
};
$("pickOk").onclick = () => {
  const out = [...pick.sel].map((i) => pickJoin(pick.cwd, pick.entries[i].name));
  if (!out.length) out.push(pick.cwd); // nothing selected: take the folder being shown
  out.forEach(addSendItem);
  $("picker").hidden = true;
  $("settings").hidden = false;
};
$("pickCancel").onclick = () => {
  $("picker").hidden = true;
  $("settings").hidden = false;
};

// sending an edited file back
listen("sendback", (ev) => {
  const d = ev.payload;
  const t = tabs.get(d.tab);
  $("sbPath").textContent = (t ? t.name + ":  " : "") + d.remote;
  $("sbRemote").textContent = d.remote_text;
  $("sbLocal").textContent = d.local_text;
  $("sbWarn").textContent = d.warn;
  $("sendback").hidden = false;
  const done = (send) => {
    $("sendback").hidden = true;
    invoke("send_back", { local: d.local, send });
    activePane.list.focus();
  };
  $("sbSend").onclick = () => done(true);
  $("sbKeep").onclick = () => done(false);
});

// ---------------------------------------------------------------------------
// status, progress and the Cement card

function setMessage(text) {
  if (active) invoke("status_message", { tab: active, text });
  else $("st1").textContent = text;
}

function drawStatus() {
  const t = tab();
  $("stDot").className = "dot " + dotClass(t);
  $("stDot").hidden = !t;
  $("st0").textContent = t ? t.status.part0 : "Not connected";
  $("st1").textContent = t ? t.status.part1 : "";
}

function drawProgress() {
  const t = tab();
  const p = t?.progress;
  $("queue").hidden = !p || !p.active;
  if (!p || !p.active) return;
  $("qFill").style.width = p.permille / 10 + "%";
  $("qName").textContent = p.name ? (p.up ? "↑ " : "↓ ") + p.name : t.status.part1;
  $("qDetail").textContent = p.detail;
}

function drawCement() {
  const t = tab();
  const c = t && t.state === "connected" ? t.cement : null;
  $("cement").hidden = !c;
  if (!c) return;
  $("cBatteryBox").hidden = !c.battery;
  $("cBattery").textContent = c.battery + (c.charging ? ", charging" : c.plugged ? ", plugged in" : "");
  $("cBattery").classList.toggle("low", c.low);
  $("cBatteryFill").style.width = Math.max(0, c.battery_percent) + "%";
  $("cSdBox").hidden = !c.sd_free;
  $("cSd").textContent = c.sd_free + " free";
  $("cSdTotal").textContent = "of " + c.sd_total;
  $("cSdFill").style.width = c.sd_permille / 10 + "%";
  $("cInstalling").hidden = !c.installing;
  if (c.installing) {
    $("cInstName").textContent = c.installing.name;
    $("cInstFill").style.width = c.installing.permille / 10 + "%";
  }
  const busy = !!c.installing;
  $("cLoginBox").hidden = !c.can_login;
  $("btnCementLogin").textContent = c.login ? "On, change..." : "Off, set one...";
  $("cCias").innerHTML = c.cias.length
    ? c.cias
        .map((x, i) =>
          `<div class="cia"><span title="${escapeHtml(x.path)}">${escapeHtml(x.name)}</span>` +
          (c.can_install
            ? `<span class="size">${escapeHtml(x.size)}</span><button class="pill accent" data-i="${i}"${busy ? " disabled" : ""}>Install</button>`
            : "") +
          `</div>`)
        .join("")
    : '<div class="dim">None on the 3DS</div>';
  for (const b of $("cCias").querySelectorAll("button")) b.onclick = () => invoke("install_cia", { tab: active, path: c.cias[+b.dataset.i].path }).catch(tell);
  $("cCiaHint").textContent = c.can_install ? "Or press A on the 3DS" : "Press A on the 3DS to install (update Cement to install from here)";
}
$("btnSend").onclick = () => tab() && invoke("send_to_pc", { tab: active });
$("btnCementLogin").onclick = () => {
  const site = sites.find((s) => s.id === tab()?.saved_id);
  $("clUser").value = site?.user || "";
  $("clPass").value = "";
  $("clRemove").hidden = !tab()?.cement?.login;
  $("cementLogin").hidden = false;
  $("clUser").focus();
};
const setCementLogin = (user, pass) => {
  $("cementLogin").hidden = true;
  invoke("set_cement_login", { tab: active, user, pass }).catch(tell);
};
$("clSave").onclick = () => setCementLogin($("clUser").value, $("clPass").value);
$("clRemove").onclick = () => setCementLogin("", "");
$("clCancel").onclick = () => ($("cementLogin").hidden = true);
$("clPass").addEventListener("keydown", (ev) => ev.key === "Enter" && $("clSave").click());
listen("sites-changed", async () => {
  sites = await invoke("sites_list");
  renderSites();
});

listen("status", (ev) => {
  const t = tabs.get(ev.payload.tab);
  if (!t) return;
  t.status = ev.payload;
  if (t.id === active) {
    drawStatus();
    if (t.progress.active && !t.progress.name) drawProgress();
  }
});
listen("progress", (ev) => {
  const t = tabs.get(ev.payload.tab);
  if (!t) return;
  const ended = t.progress.active && !ev.payload.active;
  t.progress = ev.payload;
  if (t.id === active) drawProgress();
  if (ended && localPane.st.path) localPane.refresh(); // downloads land on the PC side
});
listen("tab-state", (ev) => {
  const s = ev.payload;
  const t = tabs.get(s.tab);
  if (!t) return;
  const was = t.state;
  t.state = s.state;
  t.cement = s.cement;
  if (s.state === "connected") t.refused = null;
  if (t.id === active) {
    drawOffline();
    drawCement();
    drawStatus();
    updateToolbar();
    updateSendSection();
  }
  if (was !== s.state || !!t.cement) {
    renderTabs();
    renderSites();
  }
});
listen("list", (ev) => {
  const r = ev.payload;
  const t = tabs.get(r.tab);
  if (!t) return;
  if (r.notify === 1) {
    if (r.tab !== pick.tab || r.path !== pick.cwd) return;
    pick.entries = r.entries.sort(byName);
    pickRender();
    return;
  }
  // the first listing tells where the server's home is
  if (!t.remote.path) t.remote.path = r.path;
  if (t.refreshLocal && !t.progress.active) {
    t.refreshLocal = false;
    if (localPane.st.path) localPane.refresh();
  }
  if (t.id === active) {
    remotePane.show(r);
    if (!r.ok) setMessage("Can't open " + r.path);
  } else if (r.path === t.remote.path) {
    // a background tab keeps its listing up to date
    const st = t.remote;
    st.entries = r.entries.sort(byName);
    st.shown = r.path;
  }
});

// ---------------------------------------------------------------------------
// start

(async () => {
  const init = await invoke("init");
  $("version").textContent = init.version.replace(/\.\d+(-[0-9a-f]+-dev)?$/, "");
  $("version").title = "Chomp v" + init.version;
  settings = init.settings;
  accent = init.accent;
  applyAccent(accent);
  sites = init.sites;
  renderSites();
  showTab();
  localStart = init.local_start;
  protect3ds = init.protect_3ds;
  protectedPaths = init.protected;
  freePaths = init.protected_free;
  windowsDir = init.windows_dir;
  checkUpdates = init.check_updates;
  bundles = init.bundles;
  if (checkUpdates) setTimeout(lookForUpdate, 2500); // after the window is up, never in its way
  localPane.navigate(init.local_dir);
  localPane.focus();
  // the sites that were open last time
  for (const id of recall("openSites", sites.length === 1 ? [sites[0].id] : [])) {
    if (sites.some((s) => s.id === id)) await openSite(id);
  }
})();
