// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Chomp - a Windows file explorer for Cement, the FTP server on the 3DS.
//!
//! The window is HTML/CSS (ui/) shown by WebView2; everything else is Rust:
//! two FTP connections (chomp_core::workers), one for browsing and one for
//! transfers, both reconnecting forever and resuming interrupted transfers.
//! Chomp is portable: chomp.ini, temp files and WebView2's data stay next to the exe.

#![windows_subsystem = "windows"]

mod dragout;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chomp_core::config::{Config, exe_dir};
use chomp_core::ftp::Entry;
use chomp_core::status::battery_text;
use chomp_core::util::{format_size, normalize_remote, parent_remote, stamp_text};
use chomp_core::workers::{Client, Event, Job, Kind, Sink};
use chomp_core::{REQUIRED_PROTOCOL, VERSION, win};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// a file opened from the 3DS: watched so that saved edits can be sent back
struct Opened {
    local: String,
    remote: String,
    remote_stamp: String, // 3DS modification time when downloaded/sent
    baseline: u64,        // local last-write time already handled
    seen: u64,            // last-write time seen while waiting for the save to settle
    changed_at: Option<Instant>,
    asking: bool,
}

#[derive(Default)]
struct Status {
    part0: String, // connection
    part1: String, // transfer progress or the last message
    part2: String, // SD card space and battery
    part2_short: String,
    last_msg: String,
}

pub struct AppState {
    client: Mutex<Option<Client>>,
    config: Mutex<Config>,
    opened: Mutex<Vec<Opened>>,
    status: Mutex<Status>,
    handle: OnceLock<AppHandle>,
}

type St<'a> = State<'a, Arc<AppState>>;

impl AppState {
    fn with_client<T>(&self, f: impl FnOnce(&Client) -> T) -> Option<T> {
        self.client.lock().unwrap().as_ref().map(f)
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(h) = self.handle.get() {
            let _ = h.emit(event, payload);
        }
    }

    /// the line under the list: a message, unless a transfer shows its progress
    fn message(&self, text: String) {
        let active = self.with_client(|c| c.shared.progress.lock().unwrap().active).unwrap_or(false);
        let mut st = self.status.lock().unwrap();
        st.last_msg = text.clone();
        if !active {
            st.part1 = text;
            drop(st);
            self.emit_status();
        }
    }

    fn emit_status(&self) {
        let st = self.status.lock().unwrap();
        self.emit(
            "status",
            StatusOut { part0: st.part0.clone(), part1: st.part1.clone(), part2: st.part2.clone(), part2_short: st.part2_short.clone() },
        );
    }

    /// "Connected to ...", SD space and battery
    fn update_conn_state(&self) {
        let Some((connected, info)) =
            self.with_client(|c| (c.shared.browse.connected(), c.shared.server_info.lock().unwrap().clone()))
        else {
            return;
        };
        let (host, port) = {
            let c = self.config.lock().unwrap();
            (c.host.clone(), c.port)
        };
        let addr = format!("{host}:{port}");
        let mut part0 = format!("Waiting for 3DS at {addr}...");
        let (mut right, mut short) = (String::new(), String::new());
        if connected {
            part0 = format!("Connected to {addr}");
            if !info.version.is_empty() {
                part0 += &format!("  -  3DS app v{}", info.version);
            }
            // only when this PC app needs a newer 3DS app, not for every new build
            if info.protocol < REQUIRED_PROTOCOL {
                part0 += "  -  update the 3DS app: install the new CIA";
            }
            if info.has_space {
                short = format!("SD: {} free", format_size(info.sd_free));
                right = format!("{short} of {}", format_size(info.sd_total));
            }
            if info.battery.known {
                let bat = format!("Battery: {}{}", battery_text(&info.battery), if info.battery.low { " (low)" } else { "" });
                for s in [&mut right, &mut short] {
                    if !s.is_empty() {
                        s.push_str("   ");
                    }
                    s.push_str(&bat);
                }
            }
        }
        {
            let mut st = self.status.lock().unwrap();
            st.part0 = part0;
            st.part2 = right;
            st.part2_short = short;
        }
        self.emit("connected", connected);
        self.emit_status();
    }
}

// ---------------------------------------------------------------------------
// what the window receives

#[derive(Serialize, Clone)]
struct StatusOut {
    part0: String,
    part1: String,
    part2: String,
    part2_short: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct EntryOut {
    name: String,
    dir: bool,
    size: u64,
}

impl From<&Entry> for EntryOut {
    fn from(e: &Entry) -> EntryOut {
        EntryOut { name: e.name.clone(), dir: e.dir, size: e.size }
    }
}

impl From<EntryOut> for Entry {
    fn from(e: EntryOut) -> Entry {
        Entry { name: e.name, dir: e.dir, size: e.size }
    }
}

#[derive(Serialize, Clone)]
struct ListOut {
    notify: u32,
    path: String,
    ok: bool,
    entries: Vec<EntryOut>,
}

#[derive(Serialize, Clone)]
struct ProgressOut {
    active: bool,
    permille: u64,
}

#[derive(Serialize, Clone)]
struct SendBackOut {
    local: String,
    remote: String,
    remote_text: String,
    local_text: String,
    warn: String,
}

/// forwards the workers' events to the window
struct Bridge(Arc<AppState>);

impl Sink for Bridge {
    fn emit(&self, e: Event) {
        let app = &self.0;
        match e {
            Event::State => app.update_conn_state(),
            Event::List(l) => app.emit(
                "list",
                ListOut { notify: l.notify, path: l.path, ok: l.ok, entries: l.entries.iter().map(EntryOut::from).collect() },
            ),
            Event::Message(m) => app.message(m),
            Event::Opened(f) => {
                let baseline = win::last_write(&f.local).map_or(0, |w| w.0);
                app.opened.lock().unwrap().push(Opened {
                    local: f.local.clone(),
                    remote: f.remote,
                    remote_stamp: f.remote_stamp,
                    baseline,
                    seen: 0,
                    changed_at: None,
                    asking: false,
                });
                // Windows' file associations want the UI thread
                if let Some(h) = app.handle.get() {
                    let a = app.clone();
                    let _ = h.run_on_main_thread(move || {
                        if let Err(e) = win::launch_file(&f.local) {
                            a.message(e);
                        }
                    });
                }
            }
            Event::Sent(f) => {
                for o in app.opened.lock().unwrap().iter_mut().filter(|o| o.local == f.local) {
                    o.remote_stamp = f.remote_stamp.clone(); // the 3DS now has this version
                }
                app.message(format!("Sent your changes to {}", f.remote));
            }
            Event::Info(r) => {
                let mut opened = app.opened.lock().unwrap();
                let Some(o) = opened.iter_mut().find(|o| o.local == r.local) else { return };
                let Some((write, size)) = win::last_write(&o.local) else {
                    o.asking = false;
                    return;
                };
                o.seen = write; // the save being asked about
                let changed = r.exists && !o.remote_stamp.is_empty() && !r.stamp.is_empty() && r.stamp != o.remote_stamp;
                let warn = if !r.exists {
                    "The file was deleted or moved on the 3DS. Sending creates it again."
                } else if changed {
                    "The file on the 3DS changed since you opened it. Sending replaces that version."
                } else {
                    ""
                };
                app.emit(
                    "sendback",
                    SendBackOut {
                        local: o.local.clone(),
                        remote: o.remote.clone(),
                        remote_text: if r.exists {
                            format!("{}   {}", stamp_text(&r.stamp), format_size(r.size))
                        } else {
                            "not there anymore".into()
                        },
                        local_text: format!("{}   {}", win::local_time_text(write), format_size(size)),
                        warn: warn.into(),
                    },
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// progress and edited files, 4 times a second

fn ticker(app: Arc<AppState>) {
    let (mut was_active, mut last_done, mut last_tick, mut rate) = (false, 0u64, Instant::now(), 0f64);
    let mut last_sent: Option<(bool, u64, String)> = None;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let Some((p, connected)) = app.with_client(|c| {
            (c.shared.progress.lock().unwrap().clone(), c.shared.browse.connected() || c.shared.xfer.connected())
        }) else {
            continue;
        };
        if p.active != was_active {
            was_active = p.active;
            rate = 0.0;
            last_done = p.done;
            last_tick = Instant::now();
        }
        if !p.active {
            // the last message (one that came while the transfer still ran shows now)
            let mut st = app.status.lock().unwrap();
            st.part1 = st.last_msg.clone();
        }
        let mut text = None;
        if p.active {
            let ms = last_tick.elapsed().as_millis() as f64;
            if ms >= 500.0 {
                let inst = if p.done >= last_done { (p.done - last_done) as f64 * 1000.0 / ms } else { 0.0 };
                rate = if rate == 0.0 { inst } else { rate * 0.6 + inst * 0.4 };
                last_done = p.done;
                last_tick = Instant::now();
            }
            text = Some(if !connected {
                "Paused, waiting for 3DS...".to_string()
            } else if p.file_count != 0 && p.del {
                format!("Deleting {}/{}: {}", p.file_index, p.file_count, p.name)
            } else if p.file_count != 0 {
                let mut t = format!(
                    "{} {}/{}: {}   {} / {}   {}/s",
                    if p.up { "Uploading" } else { "Downloading" },
                    p.file_index,
                    p.file_count,
                    p.name,
                    format_size(p.done),
                    format_size(p.total),
                    format_size(rate as u64)
                );
                if rate > 1.0 && p.total > p.done {
                    let eta = ((p.total - p.done) as f64 / rate) as u64;
                    t += &format!("   {}:{:02} left", eta / 60, eta % 60);
                }
                t
            } else {
                "Preparing...".to_string()
            });
        }
        if let Some(t) = &text {
            app.status.lock().unwrap().part1 = t.clone();
        }
        let permille = if p.total != 0 { p.done * 1000 / p.total } else { 0 };
        let now = (p.active, permille, app.status.lock().unwrap().part1.clone());
        if last_sent.as_ref() != Some(&now) {
            app.emit("progress", ProgressOut { active: p.active, permille });
            app.emit_status();
            last_sent = Some(now);
        }
        check_opened(&app);
    }
}

/// a save is handled once it has not changed for 1.5 s (editors often write a
/// file in several steps): then the 3DS side is fetched and the user asked
fn check_opened(app: &AppState) {
    let mut ask = Vec::new();
    for o in app.opened.lock().unwrap().iter_mut() {
        let Some((ft, _)) = win::last_write(&o.local) else { continue };
        if o.asking || ft == o.baseline {
            continue;
        }
        if o.changed_at.is_none() || ft != o.seen {
            o.seen = ft;
            o.changed_at = Some(Instant::now());
            continue;
        }
        if o.changed_at.unwrap().elapsed() < Duration::from_millis(1500) {
            continue;
        }
        o.asking = true;
        ask.push((o.remote.clone(), o.local.clone()));
    }
    for (remote, local) in ask {
        let mut j = Job::new(Kind::Info);
        j.path = remote;
        j.path2 = local;
        app.with_client(|c| c.shared.browse.push(j));
    }
}

// ---------------------------------------------------------------------------
// commands from the window

#[derive(Serialize)]
struct InitOut {
    version: String,
    configured: bool,
    config: SettingsOut,
}

#[derive(Serialize, Deserialize, Clone)]
struct SettingsOut {
    host: String,
    port: u16,
    dark: bool,
    send_dir: String,
    send_items: Vec<String>,
    send_replace: bool,
}

impl From<&Config> for SettingsOut {
    fn from(c: &Config) -> SettingsOut {
        SettingsOut {
            host: c.host.clone(),
            port: c.port,
            dark: c.dark,
            send_dir: c.send_dir.clone(),
            send_items: c.send_items.clone(),
            send_replace: c.send_replace,
        }
    }
}

#[tauri::command]
fn init(state: St) -> InitOut {
    let c = state.config.lock().unwrap();
    InitOut { version: VERSION.into(), configured: !c.host.is_empty(), config: SettingsOut::from(&*c) }
}

/// Err(text) when the settings can't be used
#[tauri::command]
fn save_settings(state: St, s: SettingsOut) -> Result<(), String> {
    let host = s.host.trim().to_string();
    if host.is_empty() || s.port == 0 {
        return Err("Enter the IP address and port shown on the 3DS.".into());
    }
    if s.send_replace && !s.send_dir.trim().is_empty() && win::unsafe_replace_target(&s.send_dir) {
        return Err("\"Replace PC folder contents\" can't be used with a drive or with your Desktop, Documents, \
                    Downloads, user, Windows or Program Files folder. Choose a dedicated folder."
            .into());
    }
    let moved = {
        let mut c = state.config.lock().unwrap();
        let moved = c.host != host || c.port != s.port;
        *c = Config {
            host: host.clone(),
            port: s.port,
            dark: s.dark,
            send_dir: s.send_dir.trim().to_string(),
            send_items: s.send_items,
            send_replace: s.send_replace,
        };
        c.save();
        moved
    };
    let mut client = state.client.lock().unwrap();
    match client.as_ref() {
        None => *client = Some(Client::start(&host, s.port, Box::new(Bridge(state.inner().clone())))),
        Some(c) if moved => {
            *c.shared.host.lock().unwrap() = (host, s.port);
            *c.shared.cwd.lock().unwrap() = "/".into();
            c.shared.browse.reconnect();
            c.shared.xfer.reconnect();
        }
        _ => {}
    }
    drop(client);
    state.update_conn_state();
    Ok(())
}

#[tauri::command]
fn navigate(state: St, path: String) {
    state.with_client(|c| {
        *c.shared.cwd.lock().unwrap() = path.clone();
        let mut j = Job::new(Kind::List);
        j.path = path;
        c.shared.browse.push(j);
    });
}

/// a listing for the 3DS picker in Settings
#[tauri::command]
fn picker_list(state: St, path: String) {
    state.with_client(|c| {
        let mut j = Job::new(Kind::List);
        j.path = path;
        j.notify = 1;
        c.shared.browse.push(j);
    });
}

#[tauri::command]
fn mkdir(state: St, path: String) {
    state.with_client(|c| {
        let mut j = Job::new(Kind::Mkdir);
        j.path = path;
        c.shared.browse.push(j);
    });
}

#[tauri::command]
fn rename(state: St, from: String, to: String) {
    state.with_client(|c| {
        let mut j = Job::new(Kind::Rename);
        j.path = from;
        j.path2 = to;
        c.shared.browse.push(j);
    });
}

/// on the transfer connection: tracked with the progress bar, cancellable, resumable
#[tauri::command]
fn delete(state: St, entries: Vec<EntryOut>) {
    state.with_client(|c| {
        let mut j = Job::new(Kind::Delete);
        j.path = c.shared.cwd();
        j.remotes = entries.into_iter().map(Entry::from).collect();
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
}

#[tauri::command]
fn upload(state: St, paths: Vec<String>, dir: String) {
    if paths.is_empty() {
        return;
    }
    state.with_client(|c| {
        let mut j = Job::new(Kind::Upload);
        j.path = dir;
        j.locals = paths;
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
}

fn start_download(state: &AppState, entries: Vec<Entry>, dir: String, open_after: bool) -> Option<chomp_core::workers::JobPtr> {
    state.with_client(|c| {
        let mut j = Job::new(Kind::Download);
        j.path = c.shared.cwd();
        j.path2 = dir;
        j.remotes = entries;
        j.open_after = open_after;
        c.shared.start_progress();
        c.shared.xfer.push(j)
    })
}

fn pick_folder(app: &AppHandle, title: &str) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app.dialog().file().set_title(title);
    if let Some(w) = app.get_webview_window("main") {
        d = d.set_parent(&w);
    }
    d.blocking_pick_folder().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
async fn download_to(app: AppHandle, state: St<'_>, entries: Vec<EntryOut>) -> Result<(), ()> {
    if let Some(dir) = pick_folder(&app, "Download to") {
        start_download(&state, entries.into_iter().map(Entry::from).collect(), dir, false);
    }
    Ok(())
}

#[tauri::command]
async fn browse_folder(app: AppHandle, title: String) -> Result<Option<String>, ()> {
    Ok(pick_folder(&app, &title))
}

/// a copy for viewing and editing: saved edits are offered back to the 3DS
#[tauri::command]
fn open_file(state: St, entry: EntryOut) {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let dir = format!("{}\\open\\{stamp}", win::temp_root());
    let _ = std::fs::create_dir_all(&dir);
    start_download(&state, vec![entry.into()], dir, true);
}

#[tauri::command]
fn send_back(state: St, local: String, send: bool) {
    let mut opened = state.opened.lock().unwrap();
    let Some(o) = opened.iter_mut().find(|o| o.local == local) else { return };
    // either way this save is handled; the next one asks again
    o.baseline = o.seen;
    o.changed_at = None;
    o.asking = false;
    if !send {
        return;
    }
    let (remote, local) = (o.remote.clone(), o.local.clone());
    drop(opened);
    state.with_client(|c| {
        let mut j = Job::new(Kind::Upload);
        j.path = parent_remote(&remote);
        j.locals = vec![local.clone()];
        j.track_local = local;
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
}

/// one click: copy every 3DS item listed in Settings directly into the PC folder
/// (/openmw/logs -> <folder>\logs)
#[tauri::command]
async fn send_to_pc(app: AppHandle, state: St<'_>) -> Result<(), ()> {
    let (items, mut dir, replace) = {
        let c = state.config.lock().unwrap();
        (c.send_items.clone(), c.send_dir.clone(), c.send_replace)
    };
    if items.is_empty() {
        state.message("Add the 3DS folders/files to send in Settings first".into());
        return Ok(());
    }
    if dir.is_empty() {
        let Some(d) = pick_folder(&app, "Choose the Send to PC folder (saved in Settings)") else { return Ok(()) };
        dir = d;
        let mut c = state.config.lock().unwrap();
        c.send_dir = dir.clone();
        c.save();
    }
    let _ = std::fs::create_dir_all(&dir);
    if !std::path::Path::new(&dir).is_dir() {
        state.message(format!("Can't use the Send to PC folder: {dir}"));
        return Ok(());
    }
    state.with_client(|c| {
        let mut j = Job::new(Kind::SendToPc);
        j.path2 = dir;
        j.send_paths = items;
        j.replace = replace;
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
    Ok(())
}

#[tauri::command]
fn cancel(state: St) {
    state.with_client(|c| {
        c.shared.cancel.store(true, Ordering::Relaxed);
        c.shared.xfer.clear_pending();
    });
}

#[tauri::command]
fn reconnect(state: St) {
    state.with_client(|c| {
        c.shared.browse.reconnect();
        c.shared.xfer.reconnect();
    });
}

#[tauri::command]
fn status_message(state: St, text: String) {
    state.message(text);
}

#[tauri::command]
fn normalize(path: String) -> String {
    normalize_remote(&path)
}

/// starts dragging the selected 3DS items out to Explorer; they are downloaded
/// when dropped
#[tauri::command]
fn drag_out(app: AppHandle, state: St, entries: Vec<EntryOut>) {
    let st = state.inner().clone();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        dragout::drag_out(&st, entries.into_iter().map(Entry::from).collect());
        let _ = a.emit("dragout-done", ());
    });
}

fn main() {
    // leftovers from previous drag-out downloads and opened files
    win::delete_tree(&win::temp_root());

    let (config, configured) = Config::load();
    let state = Arc::new(AppState {
        client: Mutex::new(None),
        config: Mutex::new(config.clone()),
        opened: Mutex::new(Vec::new()),
        status: Mutex::new(Status::default()),
        handle: OnceLock::new(),
    });
    if configured {
        *state.client.lock().unwrap() = Some(Client::start(&config.host, config.port, Box::new(Bridge(state.clone()))));
    }

    let s = state.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .setup(move |app| {
            let _ = s.handle.set(app.handle().clone());
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title(format!("Chomp v{VERSION}"))
                .inner_size(760.0, 520.0)
                .min_inner_size(740.0, 300.0)
                .theme(Some(if config.dark { tauri::Theme::Dark } else { tauri::Theme::Light }))
                // portable: WebView2 keeps its data next to the exe, not in AppData
                .data_directory(format!("{}\\webview", exe_dir()).into())
                .build()?;
            let t = s.clone();
            std::thread::Builder::new().name("ticker".into()).spawn(move || ticker(t))?;
            s.update_conn_state();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            init,
            save_settings,
            navigate,
            picker_list,
            mkdir,
            rename,
            delete,
            upload,
            download_to,
            browse_folder,
            open_file,
            send_back,
            send_to_pc,
            cancel,
            reconnect,
            status_message,
            normalize,
            drag_out,
            set_dark,
        ])
        .run(tauri::generate_context!())
        .expect("Chomp");

    if let Some(mut c) = state.client.lock().unwrap().take() {
        c.stop();
    }
}

/// the title bar follows the theme
#[tauri::command]
fn set_dark(app: AppHandle, dark: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_theme(Some(if dark { tauri::Theme::Dark } else { tauri::Theme::Light }));
    }
}
