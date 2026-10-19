// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
// Chomp's window. The work happens in Rust (invoke = a command, listen = an event).

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let cwd = "/";
let shown = "";          // folder whose listing is on screen
let entries = [];        // what the list shows
let selected = new Set(); // selected names
let anchor = -1;         // shift-click start (row index)
let focusRow = 0;
let selectNext = "";     // a renamed item to select in the next listing
let connected = false;
let config = null;
let draggingOut = false;

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function formatSize(v) {
  if (v >= 1073741824) return (v / 1073741824).toFixed(2) + " GB";
  if (v >= 1048576) return (v / 1048576).toFixed(1) + " MB";
  if (v >= 1024) return (v / 1024).toFixed(1) + " KB";
  return v + " B";
}
const join = (dir, name) => (dir === "/" ? "/" + name : dir + "/" + name);
const parent = (dir) => {
  const i = dir.lastIndexOf("/");
  return i <= 0 ? "/" : dir.slice(0, i);
};

const ICON_DIR = '<svg class="icon" viewBox="0 0 16 16"><path fill="#e8b84a" d="M1 3.5C1 2.7 1.7 2 2.5 2h3.6l1.5 1.5h5.9c.8 0 1.5.7 1.5 1.5v7.5c0 .8-.7 1.5-1.5 1.5h-11C1.7 14 1 13.3 1 12.5z"/></svg>';
const ICON_FILE = '<svg class="icon" viewBox="0 0 16 16"><path fill="#9aa4b2" d="M3 1h6.5L13 4.5V14c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1V2c0-.6.4-1 1-1z"/><path fill="#c9d0da" d="M9.5 1v3.5H13z"/></svg>';

function escapeHtml(s) {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

// ---------------------------------------------------------------------------
// the list

// rows: optional ".." then entries; row index -> entry (null for "..")
function rowEntry(i) {
  if (cwd !== "/") {
    if (i === 0) return null;
    return entries[i - 1];
  }
  return entries[i];
}
const rowCount = () => entries.length + (cwd !== "/" ? 1 : 0);
const isParentRow = (i) => cwd !== "/" && i === 0;

function render() {
  const rows = [];
  if (cwd !== "/") rows.push(`<tr data-i="0"><td class="name">${ICON_DIR}..</td><td class="size"></td></tr>`);
  entries.forEach((e, k) => {
    const i = k + (cwd !== "/" ? 1 : 0);
    rows.push(
      `<tr data-i="${i}" class="${selected.has(e.name) ? "sel" : ""}"><td class="name">${e.dir ? ICON_DIR : ICON_FILE}${escapeHtml(e.name)}</td>` +
        `<td class="size">${e.dir ? "" : formatSize(e.size)}</td></tr>`
    );
  });
  $("rows").innerHTML = rows.join("");
}

function selectedEntries() {
  return entries.filter((e) => selected.has(e.name));
}

function setSelection(names) {
  selected = new Set(names);
  for (const tr of $("rows").children) {
    const e = rowEntry(+tr.dataset.i);
    tr.classList.toggle("sel", !!e && selected.has(e.name));
  }
}

function navigate(path) {
  cwd = path;
  $("path").value = path;
  if (path !== shown) {
    // never leave the previous folder clickable under the new path: while the
    // 3DS is unreachable that turned /luma into /luma/luma/luma...
    entries = [];
    shown = "";
    selected = new Set();
    focusRow = 0;
    render();
  }
  invoke("navigate", { path });
}
const refresh = () => navigate(cwd);

function showList(r) {
  if (r.path !== cwd) return;
  if (!r.ok) {
    invoke("status_message", { text: "Can't open " + r.path });
    if (cwd !== "/") navigate(parent(cwd));
    return;
  }
  const keep = selectNext ? new Set([selectNext]) : r.path === shown ? selected : new Set();
  selectNext = "";
  entries = r.entries.sort((a, b) => (a.dir !== b.dir ? (a.dir ? -1 : 1) : collator.compare(a.name, b.name)));
  shown = cwd;
  selected = new Set([...keep].filter((n) => entries.some((e) => e.name === n)));
  render();
}

function openRow(i) {
  if (i < 0 || i >= rowCount()) return;
  if (isParentRow(i)) return navigate(parent(cwd));
  const e = rowEntry(i);
  if (e.dir) navigate(join(cwd, e.name));
  else invoke("open_file", { entry: e });
}

function goUp() {
  if (cwd !== "/") navigate(parent(cwd));
}

function newFolder() {
  let name = "New folder";
  for (let n = 2; entries.some((e) => e.name === name); n++) name = `New folder (${n})`;
  invoke("mkdir", { path: join(cwd, name) });
}

async function deleteSelected() {
  const sel = selectedEntries();
  if (!sel.length) return;
  const q =
    sel.length === 1
      ? `Delete "${sel[0].name}"${sel[0].dir ? " and everything in it" : ""}?`
      : `Delete ${sel.length} items?`;
  if (await ask(q)) invoke("delete", { entries: sel });
}

function startRename() {
  const sel = selectedEntries();
  if (sel.length !== 1) {
    invoke("status_message", { text: "Select one file or folder to rename" });
    return;
  }
  const e = sel[0];
  const k = entries.indexOf(e) + (cwd !== "/" ? 1 : 0);
  const td = $("rows").children[k]?.firstElementChild;
  if (!td) return;
  td.innerHTML = (e.dir ? ICON_DIR : ICON_FILE) + `<input class="rename">`;
  const input = td.querySelector("input");
  input.value = e.name;
  input.focus();
  const dot = e.dir ? -1 : e.name.lastIndexOf(".");
  input.setSelectionRange(0, dot > 0 ? dot : e.name.length);
  let done = false;
  const finish = (commit) => {
    if (done) return;
    done = true;
    const name = input.value.trim();
    render();
    $("listWrap").focus();
    if (!commit || !name || name === e.name || /[\\/]/.test(name)) return;
    selectNext = name;
    invoke("rename", { from: join(cwd, e.name), to: join(cwd, name) });
  };
  input.addEventListener("keydown", (ev) => {
    ev.stopPropagation();
    if (ev.key === "Enter") finish(true);
    if (ev.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
}

// clicks, multi-selection, keyboard
$("rows").addEventListener("mousedown", (ev) => {
  const tr = ev.target.closest("tr");
  if (!tr || ev.target.tagName === "INPUT") return;
  const i = +tr.dataset.i;
  const e = rowEntry(i);
  focusRow = i;
  if (ev.button === 2 && e && selected.has(e.name)) return; // right-click keeps the selection
  if (ev.shiftKey && anchor >= 0) {
    const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
    const names = [];
    for (let k = a; k <= b; k++) if (rowEntry(k)) names.push(rowEntry(k).name);
    setSelection(names);
  } else if (ev.ctrlKey) {
    if (e) {
      const s = new Set(selected);
      s.has(e.name) ? s.delete(e.name) : s.add(e.name);
      setSelection([...s]);
    }
    anchor = i;
  } else if (!(e && selected.has(e.name) && ev.button === 0)) {
    setSelection(e ? [e.name] : []);
    anchor = i;
  }
  if (ev.button === 0 && e && selected.has(e.name)) startDragWatch(ev);
});
$("rows").addEventListener("click", (ev) => {
  const tr = ev.target.closest("tr");
  if (!tr || ev.ctrlKey || ev.shiftKey || ev.target.tagName === "INPUT") return;
  const e = rowEntry(+tr.dataset.i);
  if (e && selected.size > 1) setSelection([e.name]); // a plain click on a multi-selection
});
$("rows").addEventListener("dblclick", (ev) => {
  const tr = ev.target.closest("tr");
  if (tr && ev.target.tagName !== "INPUT") openRow(+tr.dataset.i);
});
$("listWrap").addEventListener("mousedown", (ev) => {
  if (ev.target === $("listWrap")) setSelection([]);
});

$("listWrap").addEventListener("keydown", (ev) => {
  if (anyModal()) return;
  const n = rowCount();
  const move = (i) => {
    focusRow = Math.max(0, Math.min(n - 1, i));
    const e = rowEntry(focusRow);
    setSelection(e ? [e.name] : []);
    anchor = focusRow;
    $("rows").children[focusRow]?.scrollIntoView({ block: "nearest" });
  };
  if (ev.key === "ArrowDown") move(focusRow + 1);
  else if (ev.key === "ArrowUp") move(focusRow - 1);
  else if (ev.key === "Home") move(0);
  else if (ev.key === "End") move(n - 1);
  else if (ev.key === "Enter") openRow(focusRow);
  else if (ev.key === "Backspace") goUp();
  else if (ev.key === "Delete") deleteSelected();
  else if (ev.key === "F2") startRename();
  else if (ev.key === "F5") refresh();
  else if (ev.key === "a" && ev.ctrlKey) setSelection(entries.map((e) => e.name));
  else if (ev.key === "s" && ev.ctrlKey) invoke("send_to_pc");
  else if (ev.key === "ContextMenu") showMenu(40, 60);
  else return;
  ev.preventDefault();
});
document.addEventListener("keydown", (ev) => {
  if (ev.key === "F5" || (ev.ctrlKey && ["r", "p", "f", "g", "u"].includes(ev.key))) ev.preventDefault(); // no browser shortcuts
});

// ---------------------------------------------------------------------------
// right-click menu

function showMenu(x, y) {
  const sel = selectedEntries();
  const items = [];
  if (sel.length === 1) items.push(["Open", "", () => openRow(entries.indexOf(sel[0]) + (cwd !== "/" ? 1 : 0))]);
  if (sel.length) {
    items.push(["Download to...", "", () => invoke("download_to", { entries: sel })]);
    items.push(["Rename", "F2", startRename, sel.length !== 1]);
    items.push(["Delete", "Del", deleteSelected]);
    items.push(null);
  }
  items.push(["New folder", "", newFolder]);
  items.push(["Refresh", "F5", refresh]);
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
$("listWrap").addEventListener("contextmenu", (ev) => {
  ev.preventDefault();
  showMenu(ev.clientX, ev.clientY);
});
document.addEventListener("contextmenu", (ev) => ev.preventDefault());
document.addEventListener("mousedown", (ev) => {
  if (!$("menu").contains(ev.target)) hideMenu();
});

// ---------------------------------------------------------------------------
// drag and drop

// out to Explorer: the drag itself is Windows' (the files download on drop)
function startDragWatch(down) {
  const move = (ev) => {
    if (Math.abs(ev.clientX - down.clientX) + Math.abs(ev.clientY - down.clientY) < 6) return;
    stop();
    draggingOut = true;
    invoke("drag_out", { entries: selectedEntries() });
  };
  const stop = () => {
    removeEventListener("mousemove", move);
    removeEventListener("mouseup", stop);
  };
  addEventListener("mousemove", move);
  addEventListener("mouseup", stop);
}
listen("dragout-done", () => (draggingOut = false));

// in from Explorer: onto a folder row uploads into that folder
let hot = null;
function dropRow(position) {
  const r = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(position.x / r, position.y / r);
  const tr = el?.closest?.("#rows tr");
  if (!tr) return null;
  const i = +tr.dataset.i;
  const e = rowEntry(i);
  return isParentRow(i) || (e && e.dir) ? tr : null;
}
function highlight(tr) {
  if (hot === tr) return;
  hot?.classList.remove("drop");
  tr?.classList.add("drop");
  hot = tr;
}
window.__TAURI__.webview.getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  if (draggingOut || anyModal()) return;
  if (p.type === "over" || p.type === "enter") highlight(dropRow(p.position));
  else if (p.type === "leave") highlight(null);
  else if (p.type === "drop") {
    const tr = dropRow(p.position);
    highlight(null);
    let dir = cwd;
    if (tr) {
      const i = +tr.dataset.i;
      dir = isParentRow(i) ? parent(cwd) : join(cwd, rowEntry(i).name);
    }
    invoke("upload", { paths: p.paths, dir });
  }
});

// ---------------------------------------------------------------------------
// dialogs

const anyModal = () => [...document.querySelectorAll(".modal")].some((m) => !m.hidden);

function ask(text, yes = "Yes", no = "No", title = "Chomp") {
  return new Promise((resolve) => {
    $("askTitle").textContent = title;
    $("askText").textContent = text;
    $("askYes").textContent = yes;
    $("askNo").textContent = no;
    $("askNo").hidden = !no;
    $("ask").hidden = false;
    $("askYes").focus();
    const done = (v) => {
      $("ask").hidden = true;
      $("askYes").onclick = $("askNo").onclick = null;
      $("listWrap").focus();
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
  else if (!$("sendback").hidden) $("sbKeep").click();
  else if (!$("settings").hidden) $("settingsCancel").click();
});

// Settings
let firstRun = false;
function updateSendSection() {
  for (const id of ["sendItems", "btnAddPick", "btnRemoveItem", "newPath", "btnAddPath"]) $(id).disabled = !connected;
  $("sendNote").textContent = connected ? "" : "Connect to the 3DS to change this list.";
}
function addSendItem(p) {
  const list = $("sendItems");
  if (![...list.options].some((o) => o.value === p)) list.add(new Option(p, p));
}
function openSettings() {
  $("host").value = config.host;
  $("port").value = config.port;
  $("sendDir").value = config.send_dir;
  $("replace").checked = config.send_replace;
  $("dark").checked = config.dark;
  $("sendItems").innerHTML = "";
  config.send_items.forEach(addSendItem);
  updateSendSection();
  $("settings").hidden = false;
  $("host").focus();
}
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
    host: $("host").value.trim(),
    port: parseInt($("port").value, 10) || 0,
    dark: $("dark").checked,
    send_dir: $("sendDir").value.trim(),
    send_items: [...$("sendItems").options].map((o) => o.value),
    send_replace: $("replace").checked,
  };
  if (!s.host || s.port < 1 || s.port > 65535) {
    $("settings").hidden = true;
    await tell("Enter the IP address and port shown on the 3DS.");
    $("settings").hidden = false;
    return;
  }
  const moved = !config.host || s.host !== config.host || s.port !== config.port;
  try {
    await invoke("save_settings", { s });
  } catch (err) {
    $("settings").hidden = true;
    await tell(String(err));
    $("settings").hidden = false;
    return;
  }
  config = s;
  applyTheme();
  $("settings").hidden = true;
  firstRun = false;
  $("listWrap").focus();
  if (moved) navigate("/");
};
$("settingsCancel").onclick = () => {
  $("settings").hidden = true;
  if (firstRun) window.__TAURI__.window.getCurrentWindow().close(); // never configured: nothing to show
  $("listWrap").focus();
};

// 3DS picker (Settings > Add from 3DS...)
let pick = { cwd: "/", entries: [], sel: new Set() };
function pickNavigate(path) {
  pick = { cwd: path, entries: [], sel: new Set() };
  $("pickPath").value = path;
  $("pickRows").innerHTML = "";
  invoke("picker_list", { path });
}
function pickRender() {
  const rows = [];
  if (pick.cwd !== "/") rows.push(`<tr data-i="-1"><td>${ICON_DIR}..</td></tr>`);
  pick.entries.forEach((e, i) =>
    rows.push(`<tr data-i="${i}" class="${pick.sel.has(i) ? "sel" : ""}"><td>${e.dir ? ICON_DIR : ICON_FILE}${escapeHtml(e.name)}</td></tr>`)
  );
  $("pickRows").innerHTML = rows.join("");
}
$("pickRows").addEventListener("mousedown", (ev) => {
  const tr = ev.target.closest("tr");
  if (!tr) return;
  const i = +tr.dataset.i;
  if (i < 0) return;
  if (ev.ctrlKey) pick.sel.has(i) ? pick.sel.delete(i) : pick.sel.add(i);
  else pick.sel = new Set([i]);
  pickRender();
});
$("pickRows").addEventListener("dblclick", (ev) => {
  const tr = ev.target.closest("tr");
  if (!tr) return;
  const i = +tr.dataset.i;
  if (i < 0) pickNavigate(parent(pick.cwd));
  else if (pick.entries[i].dir) pickNavigate(join(pick.cwd, pick.entries[i].name));
});
$("pickUp").onclick = () => pick.cwd !== "/" && pickNavigate(parent(pick.cwd));
$("btnAddPick").onclick = () => {
  $("settings").hidden = true;
  $("picker").hidden = false;
  pickNavigate("/");
};
$("pickOk").onclick = () => {
  const out = [...pick.sel].map((i) => join(pick.cwd, pick.entries[i].name));
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
  $("sbPath").textContent = d.remote;
  $("sbRemote").textContent = d.remote_text;
  $("sbLocal").textContent = d.local_text;
  $("sbWarn").textContent = d.warn;
  $("sendback").hidden = false;
  const done = (send) => {
    $("sendback").hidden = true;
    invoke("send_back", { local: d.local, send });
    $("listWrap").focus();
  };
  $("sbSend").onclick = () => done(true);
  $("sbKeep").onclick = () => done(false);
});

// ---------------------------------------------------------------------------
// toolbar, status, progress

$("btnUp").onclick = goUp;
$("btnRefresh").onclick = refresh;
$("btnMkdir").onclick = newFolder;
$("btnRename").onclick = startRename;
$("btnSend").onclick = () => invoke("send_to_pc");
$("btnReconnect").onclick = () => invoke("reconnect");
$("btnSettings").onclick = openSettings;
$("btnCancel").onclick = () => invoke("cancel");

let status = { part0: "", part1: "", part2: "", part2_short: "" };
function drawStatus() {
  $("st0").textContent = status.part0;
  $("st1").textContent = status.part1;
  // the short form (no SD total) when the long one leaves little room for messages
  $("st2").textContent = status.part2;
  if ($("st1").clientWidth < 160 && status.part2_short) $("st2").textContent = status.part2_short;
}
addEventListener("resize", drawStatus);
listen("status", (ev) => {
  status = ev.payload;
  drawStatus();
});
listen("connected", (ev) => {
  connected = ev.payload;
  updateSendSection();
});
listen("progress", (ev) => {
  $("progressRow").hidden = !ev.payload.active;
  $("progressFill").style.width = ev.payload.permille / 10 + "%";
});
listen("list", (ev) => {
  const r = ev.payload;
  if (r.notify === 1) {
    if (r.path !== pick.cwd) return;
    pick.entries = r.entries.sort((a, b) => (a.dir !== b.dir ? (a.dir ? -1 : 1) : collator.compare(a.name, b.name)));
    pickRender();
  } else showList(r);
});

function applyTheme() {
  document.body.classList.toggle("dark", config.dark);
  invoke("set_dark", { dark: config.dark });
}

(async () => {
  const init = await invoke("init");
  config = init.config;
  applyTheme();
  $("listWrap").focus();
  if (!init.configured) {
    // ask for the server only once
    firstRun = true;
    openSettings();
  } else navigate("/");
})();
