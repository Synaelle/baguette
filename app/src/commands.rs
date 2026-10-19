// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! What the window can ask for: sites and tabs, and the file operations of a
//! tab's remote side.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use chomp_core::config::Config;
use chomp_core::ftp::Entry;
use chomp_core::remote::{Protocol, Site};
use chomp_core::sites::{self, Saved};
use chomp_core::util::{join_remote, normalize_remote, parent_remote};
use chomp_core::workers::{Job, Kind};
use chomp_core::{VERSION, win};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, EntryOut, St, open_tab, start_download};

#[derive(Serialize, Deserialize, Clone)]
pub struct SettingsOut {
    send_dir: String,
    send_items: Vec<String>,
    send_replace: bool,
}

impl From<&Config> for SettingsOut {
    fn from(c: &Config) -> SettingsOut {
        SettingsOut { send_dir: c.send_dir.clone(), send_items: c.send_items.clone(), send_replace: c.send_replace }
    }
}

/// a saved site as the window sees it: passwords never leave Rust
#[derive(Serialize, Clone)]
pub struct SiteOut {
    id: u32,
    name: String,
    protocol: &'static str,
    host: String,
    port: u16,
    user: String,
    has_pass: bool,
    key: String,
    has_passphrase: bool,
    start_dir: String,
    needs_password: bool,
}

impl From<&Saved> for SiteOut {
    fn from(s: &Saved) -> SiteOut {
        SiteOut {
            id: s.id,
            name: s.name.clone(),
            protocol: s.site.protocol.name(),
            host: s.site.host.clone(),
            port: s.site.port,
            user: s.site.user.clone(),
            has_pass: !s.site.pass.is_empty(),
            key: s.site.key.clone(),
            has_passphrase: !s.site.passphrase.is_empty(),
            start_dir: s.site.start_dir.clone(),
            needs_password: s.needs_password,
        }
    }
}

#[derive(Deserialize)]
pub struct SiteIn {
    id: u32, // 0: new
    name: String,
    protocol: String,
    host: String,
    port: u16,
    user: String,
    pass: Option<String>, // None: keep the saved one
    key: String,
    passphrase: Option<String>,
    start_dir: String,
}

#[derive(Serialize)]
pub struct InitOut {
    version: String,
    settings: SettingsOut,
    sites: Vec<SiteOut>,
    home: String,
}

fn sites_out(state: &AppState) -> Vec<SiteOut> {
    state.sites.lock().unwrap().iter().map(SiteOut::from).collect()
}

#[tauri::command]
pub fn init(state: St) -> InitOut {
    let c = state.config.lock().unwrap();
    InitOut { version: VERSION.into(), settings: SettingsOut::from(&*c), sites: sites_out(&state), home: win::home_dir() }
}

/// Send to PC settings. Err(text) when they can't be used.
#[tauri::command]
pub fn save_settings(state: St, s: SettingsOut) -> Result<(), String> {
    if s.send_replace && !s.send_dir.trim().is_empty() && win::unsafe_replace_target(&s.send_dir) {
        return Err("\"Replace PC folder contents\" can't be used with a drive or with your Desktop, Documents, \
                    Downloads, user, Windows or Program Files folder. Choose a dedicated folder."
            .into());
    }
    let mut c = state.config.lock().unwrap();
    c.send_dir = s.send_dir.trim().to_string();
    c.send_items = s.send_items;
    c.send_replace = s.send_replace;
    c.save();
    Ok(())
}

/// adds or updates a site; returns the sites. Open tabs of that site use it
/// from their next connection.
#[tauri::command]
pub fn site_save(state: St, s: SiteIn) -> Result<Vec<SiteOut>, String> {
    let host = s.host.trim().to_string();
    if host.is_empty() {
        return Err("Enter the server's address.".into());
    }
    let protocol = Protocol::parse(&s.protocol).ok_or("Unknown protocol")?;
    {
        let mut all = state.sites.lock().unwrap();
        let id = if s.id == 0 { sites::next_id(&all) } else { s.id };
        let old = all.iter().find(|x| x.id == id).cloned();
        let mut site = Site {
            protocol,
            host,
            port: if s.port == 0 { protocol.default_port() } else { s.port },
            user: s.user.trim().to_string(),
            pass: s.pass.clone().unwrap_or_else(|| old.as_ref().map(|o| o.site.pass.clone()).unwrap_or_default()),
            key: s.key.trim().to_string(),
            passphrase: s.passphrase.clone().unwrap_or_else(|| old.as_ref().map(|o| o.site.passphrase.clone()).unwrap_or_default()),
            trusted: Vec::new(),
            start_dir: s.start_dir.trim().to_string(),
        };
        // a key or certificate stays trusted while the address is the same
        if let Some(o) = &old {
            if o.site.host == site.host && o.site.port == site.port && o.site.protocol == site.protocol {
                site.trusted = o.site.trusted.clone();
            }
        }
        let name = if s.name.trim().is_empty() { site.host.clone() } else { s.name.trim().to_string() };
        let needs_password = old.as_ref().is_some_and(|o| o.needs_password) && s.pass.is_none();
        let saved = Saved { id, name, site, needs_password };
        match all.iter_mut().find(|x| x.id == id) {
            Some(x) => *x = saved,
            None => all.push(saved),
        }
        sites::save(&all);
    }
    Ok(sites_out(&state))
}

#[tauri::command]
pub fn site_delete(state: St, id: u32) -> Vec<SiteOut> {
    let mut all = state.sites.lock().unwrap();
    all.retain(|s| s.id != id);
    sites::save(&all);
    drop(all);
    sites_out(&state)
}

#[derive(Serialize)]
pub struct TabOut {
    id: u32,
    saved_id: u32,
    name: String,
    protocol: &'static str,
    host: String,
    start: String,
}

fn tab_out(state: &AppState, id: u32) -> Option<TabOut> {
    state.with_tab(id, |t| TabOut {
        id,
        saved_id: t.saved_id,
        name: t.name.clone(),
        protocol: t.site.protocol.name(),
        host: t.site.host.clone(),
        start: t.site.start_dir.clone(),
    })
}

/// a tab for a saved site
#[tauri::command]
pub fn open_site(state: St, id: u32) -> Result<TabOut, String> {
    let saved = state.sites.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or("This site is gone")?;
    let tab = open_tab(state.inner(), saved.id, saved.name, saved.site);
    tab_out(&state, tab).ok_or_else(|| "Could not open the tab".into())
}

/// quick connect: "sftp://user@host", "192.168.1.10:5000"...
#[tauri::command]
pub fn open_url(state: St, url: String, pass: String) -> Result<TabOut, String> {
    let mut site = sites::parse_url(&url).ok_or("Type an address like sftp://user@host or 192.168.1.10:5000")?;
    if !pass.is_empty() {
        site.pass = pass;
    }
    let name = site.host.clone();
    let tab = open_tab(state.inner(), 0, name, site);
    tab_out(&state, tab).ok_or_else(|| "Could not open the tab".into())
}

#[tauri::command]
pub fn close_tab(state: St, tab: u32) {
    let t = {
        let mut tabs = state.tabs.lock().unwrap();
        tabs.iter().position(|t| t.id == tab).map(|i| tabs.remove(i))
    };
    if let Some(mut t) = t {
        // stopping waits for the workers: not on the window's thread
        std::thread::spawn(move || t.client.stop());
    }
    state.opened.lock().unwrap().retain(|o| o.tab != tab);
}

/// applies a change to a tab's site (and the saved site), then reconnects
fn change_site(state: &AppState, tab: u32, f: impl Fn(&mut Site), save: bool) {
    let saved_id = state.with_tab(tab, |t| {
        f(&mut t.site);
        *t.client.shared.site.lock().unwrap() = t.site.clone();
        t.client.shared.browse.reconnect();
        t.client.shared.xfer.reconnect();
        t.saved_id
    });
    if let (Some(id), true) = (saved_id, save) {
        if id != 0 {
            let mut all = state.sites.lock().unwrap();
            if let Some(s) = all.iter_mut().find(|s| s.id == id) {
                f(&mut s.site);
                s.needs_password = false;
                sites::save(&all);
            }
        }
    }
}

/// the user accepted this certificate or host key
#[tauri::command]
pub fn trust(state: St, tab: u32, fingerprint: String) {
    change_site(&state, tab, |s| {
        if !s.trusted.contains(&fingerprint) {
            s.trusted.push(fingerprint.clone());
        }
    }, true);
}

/// a new password (or key passphrase) after a refused login
#[tauri::command]
pub fn login(state: St, tab: u32, user: String, pass: String, save: bool) {
    change_site(&state, tab, |s| {
        if !user.trim().is_empty() {
            s.user = user.trim().to_string();
        }
        if s.key.is_empty() {
            s.pass = pass.clone();
        } else {
            s.passphrase = pass.clone();
        }
    }, save);
}

#[tauri::command]
pub fn navigate(state: St, tab: u32, path: String) {
    state.with_client(tab, |c| {
        *c.shared.cwd.lock().unwrap() = path.clone();
        let mut j = Job::new(Kind::List);
        j.path = path;
        c.shared.browse.push(j);
    });
}

/// a listing for the 3DS picker in the Send to PC settings
#[tauri::command]
pub fn picker_list(state: St, tab: u32, path: String) {
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::List);
        j.path = path;
        j.notify = 1;
        c.shared.browse.push(j);
    });
}

#[tauri::command]
pub fn mkdir(state: St, tab: u32, path: String) {
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::Mkdir);
        j.path = path;
        c.shared.browse.push(j);
    });
}

#[tauri::command]
pub fn rename(state: St, tab: u32, from: String, to: String) {
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::Rename);
        j.path = from;
        j.path2 = to;
        c.shared.browse.push(j);
    });
}

/// on the transfer connection: tracked with the progress bar, cancellable, resumable
#[tauri::command]
pub fn delete(state: St, tab: u32, dir: String, entries: Vec<EntryOut>) {
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::Delete);
        j.path = dir;
        j.remotes = entries.into_iter().map(Entry::from).collect();
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
}

#[tauri::command]
pub fn upload(state: St, tab: u32, paths: Vec<String>, dir: String) {
    if paths.is_empty() {
        return;
    }
    state.with_tab(tab, |t| {
        // CIAs sent to Cement show in its card until they are installed
        for p in &paths {
            if p.to_ascii_lowercase().ends_with(".cia") {
                let remote = join_remote(&dir, chomp_core::util::base_name(p));
                if !t.cias.contains(&remote) {
                    t.cias.push(remote);
                }
            }
        }
        let mut j = Job::new(Kind::Upload);
        j.path = dir;
        j.locals = paths;
        t.client.shared.start_progress();
        t.client.shared.xfer.push(j);
    });
    state.update_conn_state(tab); // the CIA card
}

/// into a PC folder (dragged onto the PC side)
#[tauri::command]
pub fn download(state: St, tab: u32, dir: String, entries: Vec<EntryOut>, to: String) {
    start_download(&state, tab, &dir, entries.into_iter().map(Entry::from).collect(), to, false);
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
pub async fn download_to(app: AppHandle, state: St<'_>, tab: u32, dir: String, entries: Vec<EntryOut>) -> Result<(), ()> {
    if let Some(to) = pick_folder(&app, "Download to") {
        start_download(&state, tab, &dir, entries.into_iter().map(Entry::from).collect(), to, false);
    }
    Ok(())
}

#[tauri::command]
pub async fn browse_folder(app: AppHandle, title: String) -> Result<Option<String>, ()> {
    Ok(pick_folder(&app, &title))
}

/// a key file for a site
#[tauri::command]
pub async fn browse_file(app: AppHandle, title: String) -> Result<Option<String>, ()> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app.dialog().file().set_title(title);
    if let Some(w) = app.get_webview_window("main") {
        d = d.set_parent(&w);
    }
    Ok(d.blocking_pick_file().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

/// a copy for viewing and editing: saved edits are offered back to the server
#[tauri::command]
pub fn open_file(state: St, tab: u32, dir: String, entry: EntryOut) {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let to = format!("{}\\open\\{stamp}", win::temp_root());
    let _ = std::fs::create_dir_all(&to);
    start_download(&state, tab, &dir, vec![entry.into()], to, true);
}

#[tauri::command]
pub fn send_back(state: St, local: String, send: bool) {
    let mut opened = state.opened.lock().unwrap();
    let Some(o) = opened.iter_mut().find(|o| o.local == local) else { return };
    // either way this save is handled; the next one asks again
    o.baseline = o.seen;
    o.changed_at = None;
    o.asking = false;
    if !send {
        return;
    }
    let (tab, remote, local) = (o.tab, o.remote.clone(), o.local.clone());
    drop(opened);
    state.with_client(tab, |c| {
        let mut j = Job::new(Kind::Upload);
        j.path = parent_remote(&remote);
        j.locals = vec![local.clone()];
        j.track_local = local;
        c.shared.start_progress();
        c.shared.xfer.push(j);
    });
}

/// one click: copy every 3DS item listed in the settings directly into the PC
/// folder (/openmw/logs -> <folder>\logs)
#[tauri::command]
pub async fn send_to_pc(app: AppHandle, state: St<'_>, tab: u32) -> Result<(), ()> {
    let (items, mut dir, replace) = {
        let c = state.config.lock().unwrap();
        (c.send_items.clone(), c.send_dir.clone(), c.send_replace)
    };
    if items.is_empty() {
        state.message(tab, "Add the 3DS folders and files to send in the Send to PC settings first".into());
        let _ = app.emit("open-send-settings", tab);
        return Ok(());
    }
    if dir.is_empty() {
        let Some(d) = pick_folder(&app, "Choose the Send to PC folder (saved in the settings)") else { return Ok(()) };
        dir = d;
        let mut c = state.config.lock().unwrap();
        c.send_dir = dir.clone();
        c.save();
    }
    let _ = std::fs::create_dir_all(&dir);
    if !std::path::Path::new(&dir).is_dir() {
        state.message(tab, format!("Can't use the Send to PC folder: {dir}"));
        return Ok(());
    }
    state.with_client(tab, |c| {
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
pub fn cancel(state: St, tab: u32) {
    state.with_client(tab, |c| {
        c.shared.cancel.store(true, Ordering::Relaxed);
        c.shared.xfer.clear_pending();
    });
}

#[tauri::command]
pub fn reconnect(state: St, tab: u32) {
    state.with_client(tab, |c| {
        c.shared.browse.reconnect();
        c.shared.xfer.reconnect();
    });
}

#[tauri::command]
pub fn status_message(state: St, tab: u32, text: String) {
    state.message(tab, text);
}

#[tauri::command]
pub fn normalize(path: String) -> String {
    normalize_remote(&path)
}

/// starts dragging the selected remote items out to Explorer; they are
/// downloaded when dropped
#[tauri::command]
pub fn drag_out(app: AppHandle, state: St, tab: u32, dir: String, entries: Vec<EntryOut>) {
    let st: Arc<AppState> = state.inner().clone();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::dragout::drag_out(&st, tab, &dir, entries.into_iter().map(Entry::from).collect());
        let _ = a.emit("dragout-done", ());
    });
}
