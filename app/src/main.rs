// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Baguette - a file transfer client for Windows: FTP, FTPS and SFTP, with the
//! 3DS tools showing up when the server is Cement, the FTP server on the 3DS.
//!
//! The window is HTML/CSS (ui/) shown by WebView2; everything else is Rust.
//! Each tab is one site with two connections (baguette_core::workers), one for
//! browsing and one for transfers, both reconnecting and resuming interrupted
//! transfers. Baguette is portable: baguette.ini, temp files and WebView2's data stay
//! next to the exe.

#![windows_subsystem = "windows"]

mod commands;
mod dragout;
mod local;
mod update;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use baguette_core::config::{Config, exe_dir};
use baguette_core::ftp::Entry;
use baguette_core::remote::Site;
use baguette_core::sites::Saved;
use baguette_core::status::battery_text;
use baguette_core::util::{base_name, format_size, stamp_text};
use baguette_core::workers::{CONNECTED, Client, Event, Job, Kind, REFUSED, Sink};
use baguette_core::{RC2_PROTOCOL, REQUIRED_PROTOCOL, VERSION, win};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State, WebviewUrl, WebviewWindowBuilder};

/// a file opened from a server: watched so that saved edits can be sent back
struct Opened {
    tab: u32,
    local: String,
    remote: String,
    remote_stamp: String, // the server's modification time when downloaded/sent
    baseline: u64,        // local last-write time already handled
    seen: u64,            // last-write time seen while waiting for the save to settle
    changed_at: Option<Instant>,
    asking: bool,
}

#[derive(Default)]
struct Status {
    part0: String, // connection
    part1: String, // transfer progress or the last message
    last_msg: String,
}

/// one open site
pub struct Tab {
    id: u32,
    saved_id: u32, // 0: quick connect, not saved
    name: String,
    site: Site,
    client: Client,
    status: Status,
    cias: Vec<String>, // .cia files sent to Cement from this tab (Cement before protocol 2)
    last_install: Option<u64>, // the install result already reported
}

pub struct AppState {
    /// kept read-only on the PC side; None only when Baguette was started with
    /// --allow-windows-folder
    windows_dir: Option<String>,
    tabs: Mutex<Vec<Tab>>,
    next_tab: AtomicU32,
    sites: Mutex<Vec<Saved>>,
    config: Mutex<Config>,
    opened: Mutex<Vec<Opened>>,
    handle: OnceLock<AppHandle>,
}

type St<'a> = State<'a, Arc<AppState>>;

impl AppState {
    /// Err(why) when `path` is the Windows folder, inside it, or (`holding`) a
    /// folder holding it
    pub fn windows_guard(&self, path: &str, holding: bool) -> Result<(), String> {
        let Some(w) = &self.windows_dir else { return Ok(()) };
        let hit = if holding { baguette_core::protect::local_hit(path, w) } else { baguette_core::protect::local_inside(path, w) };
        if hit { Err(format!("{w} is read-only in Baguette: it is the Windows folder")) } else { Ok(()) }
    }

    fn with_tab<T>(&self, tab: u32, f: impl FnOnce(&mut Tab) -> T) -> Option<T> {
        self.tabs.lock().unwrap().iter_mut().find(|t| t.id == tab).map(f)
    }

    fn with_client<T>(&self, tab: u32, f: impl FnOnce(&Client) -> T) -> Option<T> {
        self.with_tab(tab, |t| f(&t.client))
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(h) = self.handle.get() {
            let _ = h.emit(event, payload);
        }
    }

    /// the line under the lists: a message, unless a transfer shows its progress
    fn message(&self, tab: u32, text: String) {
        let shown = self.with_tab(tab, |t| {
            t.status.last_msg = text.clone();
            if !t.client.shared.progress.lock().unwrap().active {
                t.status.part1 = text;
                true
            } else {
                false
            }
        });
        if shown == Some(true) {
            self.emit_status(tab);
        }
    }

    fn emit_status(&self, tab: u32) {
        let out = self.with_tab(tab, |t| StatusOut { tab, part0: t.status.part0.clone(), part1: t.status.part1.clone() });
        if let Some(o) = out {
            self.emit("status", o);
        }
    }

    /// connection state, and the Cement card when the server is the 3DS app
    fn update_conn_state(&self, tab: u32) {
        let out = self.with_tab(tab, |t| {
            let sh = &t.client.shared;
            let state = sh.browse.state.load(Ordering::Relaxed);
            let info = sh.server_info.lock().unwrap().clone();
            let addr = format!("{}:{}", t.site.host, t.site.port);
            let proto = t.site.protocol.name().to_uppercase();
            let cement = state == CONNECTED && info.cement;
            t.status.part0 = match state {
                CONNECTED if cement => {
                    let mut s = format!("Connected to Cement at {addr}");
                    if !info.version.is_empty() {
                        s += &format!("  ·  v{}", info.version);
                    }
                    // only when this PC app needs a newer 3DS app, not for every new build
                    if info.protocol < REQUIRED_PROTOCOL {
                        s += "  ·  update the 3DS app: install the new CIA";
                    }
                    s
                }
                CONNECTED => format!("Connected to {addr} ({proto})"),
                REFUSED => format!("Not connected to {addr}"),
                _ => format!("Connecting to {addr}..."),
            };
            let mut card = None;
            let mut said = None;
            if cement {
                let rc2 = info.protocol >= RC2_PROTOCOL;
                let full = &info.full;
                // Cement lists its CIAs itself since protocol 2
                let cias: Vec<CiaOut> = if rc2 {
                    full.cias
                        .iter()
                        .map(|c| CiaOut { path: c.file.clone(), name: base_name(&c.file).into(), size: format_size(c.size) })
                        .collect()
                } else {
                    t.cias.iter().map(|c| CiaOut { path: c.clone(), name: base_name(c).into(), size: String::new() }).collect()
                };
                let installing = (full.installing && full.install_size != 0).then(|| InstallingOut {
                    name: base_name(&full.install_file).into(),
                    permille: full.install_done * 1000 / full.install_size.max(1),
                });
                // a finished install is told once (not the one from before this connection)
                if let Some(r) = &full.install_result {
                    if t.last_install.is_some_and(|n| n != r.n) {
                        said = Some(if r.ok {
                            format!("Installed {} on the 3DS", base_name(&r.file))
                        } else {
                            let why = if r.error.is_empty() { "install failed".to_string() } else { r.error.clone() };
                            format!("Could not install {}: {why} ({})", base_name(&r.file), r.code.to_uppercase())
                        });
                    }
                    t.last_install = Some(r.n);
                } else if t.last_install.is_none() {
                    t.last_install = Some(0);
                }
                card = Some(CementOut {
                    battery: if info.battery.known { battery_text(&info.battery) } else { String::new() },
                    battery_percent: info.battery.percent,
                    charging: info.battery.charging,
                    plugged: info.battery.plugged,
                    low: info.battery.low,
                    sd_free: if info.has_space { format_size(info.sd_free) } else { String::new() },
                    sd_total: if info.has_space { format_size(info.sd_total) } else { String::new() },
                    sd_permille: if info.sd_total != 0 { (info.sd_total - info.sd_free.min(info.sd_total)) * 1000 / info.sd_total } else { 0 },
                    cias,
                    can_install: rc2,
                    installing,
                    version: info.version.clone(),
                    protocol: info.protocol,
                    login: full.login,
                    can_login: info.protocol >= baguette_core::LOGIN_PROTOCOL,
                });
            }
            (
                TabStateOut {
                    tab,
                    state: match state {
                        CONNECTED => "connected",
                        REFUSED => "refused",
                        _ => "waiting",
                    },
                    cement: card,
                },
                cement.then(|| (t.site.host.clone(), t.site.port)),
                said,
            )
        });
        let Some((out, cement_addr, said)) = out else { return };
        if let Some(m) = said {
            self.message(tab, m);
        }
        if let Some((host, port)) = cement_addr {
            // baguette-cli talks to the last Cement the window reached
            let mut c = self.config.lock().unwrap();
            if c.host != host || c.port != port {
                c.host = host;
                c.port = port;
                c.save();
            }
        }
        self.emit("tab-state", out);
        self.emit_status(tab);
    }
}

// ---------------------------------------------------------------------------
// what the window receives

#[derive(Serialize, Clone)]
struct StatusOut {
    tab: u32,
    part0: String,
    part1: String,
}

#[derive(Serialize, Clone)]
struct CementOut {
    battery: String,
    battery_percent: i32,
    charging: bool,
    plugged: bool,
    low: bool,
    sd_free: String,
    sd_total: String,
    sd_permille: u64,
    cias: Vec<CiaOut>,
    can_install: bool,
    installing: Option<InstallingOut>,
    version: String, // Cement's own version
    protocol: i32,   // its protocol level
    login: bool,     // Cement asks for a user name and password
    can_login: bool, // and Baguette can change that (protocol 3)
}

#[derive(Serialize, Clone)]
struct CiaOut {
    path: String,
    name: String,
    size: String,
}

#[derive(Serialize, Clone)]
struct InstallingOut {
    name: String,
    permille: u64,
}

#[derive(Serialize, Clone)]
struct TabStateOut {
    tab: u32,
    state: &'static str,
    cement: Option<CementOut>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EntryOut {
    name: String,
    dir: bool,
    size: u64,
    #[serde(default)]
    modified: String,
}

impl From<&Entry> for EntryOut {
    fn from(e: &Entry) -> EntryOut {
        EntryOut { name: e.name.clone(), dir: e.dir, size: e.size, modified: e.modified.clone() }
    }
}

impl From<EntryOut> for Entry {
    fn from(e: EntryOut) -> Entry {
        Entry { name: e.name, dir: e.dir, size: e.size, modified: e.modified }
    }
}

#[derive(Serialize, Clone)]
struct ListOut {
    tab: u32,
    notify: u32,
    path: String,
    ok: bool,
    entries: Vec<EntryOut>,
}

#[derive(Serialize, Clone)]
struct ProgressOut {
    tab: u32,
    active: bool,
    permille: u64,
    up: bool,
    name: String,
    detail: String,
}

#[derive(Serialize, Clone)]
struct SendBackOut {
    tab: u32,
    local: String,
    remote: String,
    remote_text: String,
    local_text: String,
    warn: String,
}

#[derive(Serialize, Clone)]
struct RefusedOut {
    tab: u32,
    message: String,
    fingerprint: String,
    host: String,
    protocol: String,
}

/// forwards one tab's worker events to the window
struct Bridge(Arc<AppState>, u32);

impl Sink for Bridge {
    fn emit(&self, e: Event) {
        let (app, tab) = (&self.0, self.1);
        match e {
            Event::State => app.update_conn_state(tab),
            Event::List(l) => {
                if l.ok {
                    // a CIA Cement installed is gone from its folder: off the card
                    let pruned = app.with_tab(tab, |t| {
                        let before = t.cias.len();
                        t.cias.retain(|c| {
                            baguette_core::util::parent_remote(c) != l.path
                                || l.entries.iter().any(|e| e.name == baguette_core::util::base_name(c))
                        });
                        before != t.cias.len()
                    });
                    if pruned == Some(true) {
                        app.update_conn_state(tab);
                    }
                }
                app.emit(
                    "list",
                    ListOut { tab, notify: l.notify, path: l.path, ok: l.ok, entries: l.entries.iter().map(EntryOut::from).collect() },
                )
            }
            Event::Message(m) => app.message(tab, m),
            Event::Refused { message, fingerprint } => {
                let site = app.with_tab(tab, |t| t.site.clone()).unwrap_or_default();
                app.emit(
                    "refused",
                    RefusedOut { tab, message, fingerprint, host: site.host, protocol: site.protocol.name().into() },
                );
            }
            Event::LoginChanged { user, pass } => {
                // the tab and its saved site use the new login from now on
                let saved = app.with_tab(tab, |t| {
                    t.site.user = user.clone();
                    t.site.pass = pass.clone();
                    t.saved_id
                });
                if let Some(id) = saved.filter(|id| *id != 0) {
                    let mut all = app.sites.lock().unwrap();
                    if let Some(s) = all.iter_mut().find(|s| s.id == id) {
                        s.site.user = user;
                        s.site.pass = pass;
                        baguette_core::sites::save(&all);
                    }
                }
                app.emit("sites-changed", ());
            }
            Event::Opened(f) => {
                let baseline = win::last_write(&f.local).map_or(0, |w| w.0);
                app.opened.lock().unwrap().push(Opened {
                    tab,
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
                            a.message(tab, e);
                        }
                    });
                }
            }
            Event::Sent(f) => {
                for o in app.opened.lock().unwrap().iter_mut().filter(|o| o.local == f.local) {
                    o.remote_stamp = f.remote_stamp.clone(); // the server now has this version
                }
                app.message(tab, format!("Sent your changes to {}", f.remote));
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
                    "The file was deleted or moved on the server. Sending creates it again."
                } else if changed {
                    "The file on the server changed since you opened it. Sending replaces that version."
                } else {
                    ""
                };
                app.emit(
                    "sendback",
                    SendBackOut {
                        tab,
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

#[derive(Default)]
struct Rate {
    was_active: bool,
    last_done: u64,
    last_tick: Option<Instant>,
    rate: f64,
    last_sent: Option<(bool, u64, String)>,
}

fn ticker(app: Arc<AppState>) {
    let mut rates: std::collections::HashMap<u32, Rate> = Default::default();
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let tabs: Vec<(u32, baguette_core::workers::Progress, bool)> = app
            .tabs
            .lock()
            .unwrap()
            .iter()
            .map(|t| {
                let sh = &t.client.shared;
                (t.id, sh.progress.lock().unwrap().clone(), sh.browse.connected() || sh.xfer.connected())
            })
            .collect();
        rates.retain(|id, _| tabs.iter().any(|t| t.0 == *id));
        for (tab, p, connected) in tabs {
            let r = rates.entry(tab).or_default();
            if p.active != r.was_active || r.last_tick.is_none() {
                r.was_active = p.active;
                r.rate = 0.0;
                r.last_done = p.done;
                r.last_tick = Some(Instant::now());
            }
            let mut detail = String::new();
            let mut text = None;
            if p.active {
                let ms = r.last_tick.unwrap().elapsed().as_millis() as f64;
                if ms >= 500.0 {
                    let inst = if p.done >= r.last_done { (p.done - r.last_done) as f64 * 1000.0 / ms } else { 0.0 };
                    r.rate = if r.rate == 0.0 { inst } else { r.rate * 0.6 + inst * 0.4 };
                    r.last_done = p.done;
                    r.last_tick = Some(Instant::now());
                }
                text = Some(if !connected {
                    "Paused, waiting for the server...".to_string()
                } else if p.file_count != 0 && p.del {
                    detail = format!("{} of {}", p.file_index, p.file_count);
                    format!("Deleting {}/{}: {}", p.file_index, p.file_count, p.name)
                } else if p.file_count != 0 {
                    detail = format!("{} / {}   {}/s", format_size(p.done), format_size(p.total), format_size(r.rate as u64));
                    let mut t = format!(
                        "{} {}/{}: {}   {detail}",
                        if p.up { "Uploading" } else { "Downloading" },
                        p.file_index,
                        p.file_count,
                        p.name,
                    );
                    if r.rate > 1.0 && p.total > p.done {
                        let eta = ((p.total - p.done) as f64 / r.rate) as u64;
                        let left = format!("   {}:{:02} left", eta / 60, eta % 60);
                        t += &left;
                        detail += &left;
                    }
                    t
                } else {
                    "Preparing...".to_string()
                });
            }
            let part1 = app.with_tab(tab, |t| {
                match &text {
                    Some(s) => t.status.part1 = s.clone(),
                    // the last message (one that came while the transfer still ran shows now)
                    None => t.status.part1 = t.status.last_msg.clone(),
                }
                t.status.part1.clone()
            });
            let Some(part1) = part1 else { continue };
            let permille = if p.total != 0 { p.done * 1000 / p.total } else { 0 };
            let now = (p.active, permille, part1);
            if r.last_sent.as_ref() != Some(&now) {
                let name = if p.file_count != 0 { format!("{} ({}/{})", p.name, p.file_index, p.file_count) } else { String::new() };
                app.emit("progress", ProgressOut { tab, active: p.active, permille, up: p.up, name, detail });
                app.emit_status(tab);
                r.last_sent = Some(now);
            }
        }
        check_opened(&app);
    }
}

/// a save is handled once it has not changed for 1.5 s (editors often write a
/// file in several steps): then the server side is fetched and the user asked
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
        ask.push((o.tab, o.remote.clone(), o.local.clone()));
    }
    for (tab, remote, local) in ask {
        let mut j = Job::new(Kind::Info);
        j.path = remote;
        j.path2 = local;
        app.with_client(tab, |c| c.shared.browse.push(j));
    }
}

/// a download on the tab's transfer connection: dir is the remote folder of the entries
pub fn start_download(
    state: &AppState,
    tab: u32,
    dir: &str,
    entries: Vec<Entry>,
    to: String,
    open_after: bool,
) -> Option<baguette_core::workers::JobPtr> {
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::Download);
        j.path = dir.to_string();
        j.path2 = to;
        j.remotes = entries;
        j.open_after = open_after;
        c.shared.start_progress();
        c.shared.xfer.push(j)
    })
}

/// starts a tab's connections
fn open_tab(state: &Arc<AppState>, saved_id: u32, name: String, site: Site) -> u32 {
    let id = state.next_tab.fetch_add(1, Ordering::Relaxed);
    let client = Client::start_site(site.clone(), Box::new(Bridge(state.clone(), id)));
    {
        let c = state.config.lock().unwrap();
        client.shared.bundles.store(c.bundles, Ordering::Relaxed);
        client.shared.bmp_to_png.store(c.bmp_to_png, Ordering::Relaxed);
    }
    state.tabs.lock().unwrap().push(Tab {
        id,
        saved_id,
        name,
        site,
        client,
        status: Status::default(),
        cias: Vec::new(),
        last_install: None,
    });
    id
}

fn main() {
    // leftovers from previous drag-out downloads and opened files
    win::delete_tree(&win::temp_root());

    let (config, _) = Config::load();
    let sites = baguette_core::sites::load();
    // the Windows folder stays read-only, unless Baguette is started with this
    // argument on purpose (there is no setting for it). Tests protect a
    // stand-in folder instead of the real one.
    let windows_dir = if std::env::args().any(|a| a == "--allow-windows-folder") {
        None
    } else {
        std::env::var("BAGUETTE_TEST_WINDOWS_DIR").ok().or_else(win::windows_dir)
    };
    let state = Arc::new(AppState {
        windows_dir,
        tabs: Mutex::new(Vec::new()),
        next_tab: AtomicU32::new(1),
        sites: Mutex::new(sites),
        config: Mutex::new(config.clone()),
        opened: Mutex::new(Vec::new()),
        handle: OnceLock::new(),
    });

    let s = state.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state.clone())
        .setup(move |app| {
            let _ = s.handle.set(app.handle().clone());
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title(format!("Baguette v{VERSION}"))
                .inner_size(1180.0, 720.0)
                .min_inner_size(900.0, 480.0)
                .decorations(false) // the title bar is drawn by the page
                .theme(Some(tauri::Theme::Dark))
                // portable: WebView2 keeps its data next to the exe, not in AppData
                .data_directory(format!("{}\\webview", exe_dir()).into())
                .build()?;
            let t = s.clone();
            std::thread::Builder::new().name("ticker".into()).spawn(move || ticker(t))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::init,
            commands::save_settings,
            commands::set_local_start,
            commands::set_protect_3ds,
            commands::set_check_updates,
            commands::set_bundles,
            commands::set_bmp_to_png,
            commands::set_send_to_pc,
            commands::check_update,
            commands::open_release,
            commands::remember_local,
            commands::install_cia,
            commands::set_cement_login,
            commands::sites_list,
            commands::site_save,
            commands::site_delete,
            commands::open_site,
            commands::open_url,
            commands::close_tab,
            commands::trust,
            commands::login,
            commands::navigate,
            commands::picker_list,
            commands::mkdir,
            commands::rename,
            commands::delete,
            commands::upload,
            commands::download,
            commands::download_to,
            commands::browse_folder,
            commands::browse_file,
            commands::browse_files,
            commands::open_file,
            commands::send_back,
            commands::send_to_pc,
            commands::cancel,
            commands::reconnect,
            commands::status_message,
            commands::normalize,
            commands::drag_out,
            local::local_list,
            local::local_mkdir,
            local::local_rename,
            local::local_delete,
            local::local_open,
            local::local_parent,
            local::local_join,
        ])
        .run(tauri::generate_context!())
        .expect("Baguette");

    for mut t in state.tabs.lock().unwrap().drain(..) {
        t.client.stop();
    }
}
