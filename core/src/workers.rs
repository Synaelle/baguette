// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The two connections of a Chomp.exe tab: one for browsing, one for
//! transfers, so the explorer stays responsive during uploads. Both reconnect
//! until the server is reachable again (a refused login waits for the user);
//! queued and interrupted transfers resume. Any protocol works through
//! `Remote`; Cement's extras (battery, SD space, batches) only run on Cement.
//! They report to the window through `Sink`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::ftp::{Aborter, Entry, Error, Ftp, Result};
use crate::remote::{self, Remote, Site};
use crate::status::{ServerInfo, battery_refusal, battery_text};
use crate::util::{base_name, format_size, join_remote, parent_remote};
use crate::{RC2_PROTOCOL, REQUIRED_PROTOCOL, win};

/// a listing for the main window (notify 0) or another view (the 3DS picker)
#[derive(Clone, Debug)]
pub struct ListResult {
    pub notify: u32,
    pub path: String,
    pub entries: Vec<Entry>,
    pub ok: bool,
}

/// a file downloaded to be opened (or sent back after editing), with the 3DS's
/// modification time at that moment
#[derive(Clone, Debug)]
pub struct Fetched {
    pub local: String,
    pub remote: String,
    pub remote_stamp: String, // "YYYYMMDDhhmmss", empty if unknown
}

/// the 3DS side of an edited file, fetched before asking to send it back
#[derive(Clone, Debug)]
pub struct RemoteInfo {
    pub local: String,
    pub remote: String,
    pub exists: bool,
    pub size: u64,
    pub stamp: String,
}

/// what the workers tell the window
#[derive(Clone, Debug)]
pub enum Event {
    /// a connection's state, the server info or the SD space/battery changed
    State,
    List(ListResult),
    /// a status line message
    Message(String),
    /// downloaded with open_after: open it and watch it
    Opened(Fetched),
    /// an edited file was sent back: the 3DS now has this version
    Sent(Fetched),
    Info(RemoteInfo),
    /// the server refused the connection (login, certificate, host key): it
    /// stays down until `reconnect`. fingerprint is set when the user may trust
    /// a certificate or host key to get in.
    Refused { message: String, fingerprint: String },
}

pub trait Sink: Send + Sync {
    fn emit(&self, event: Event);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    List,
    Mkdir,
    Rename,
    Delete,
    Upload,
    Download,
    SendToPc,
    Info, // size and time of path (remote) for the edited copy path2
    Install, // Cement installs the CIA at path (protocol 2)
    Color,   // Cement's accent colour becomes path ("b8283c", protocol 2)
}

#[derive(Clone, Debug)]
pub struct Item {
    pub local: String,
    pub remote: String,
    pub dir: bool,
    pub size: u64,
}

#[derive(Debug)]
pub struct Job {
    pub kind: Kind,
    pub path: String,  // List/Mkdir: target. Rename: from. Upload: remote dir. Download/Delete: remote dir
    pub path2: String, // Rename: to. Download/SendToPc: local dir. Info: the edited copy
    pub locals: Vec<String>,     // Upload sources
    pub remotes: Vec<Entry>,     // Download/Delete: entries inside path
    pub send_paths: Vec<String>, // SendToPc: absolute 3DS paths, copied under path2
    pub notify: u32,             // List: who gets the result (0: the main window)
    pub missing: usize,          // SendToPc: paths not found on the 3DS
    pub replace: bool,           // SendToPc: empty path2 first (once, when connected)
    pub recycle: bool,           // ...into the Recycle Bin
    pub open_after: bool,        // Download: open the file on the PC when done
    pub track_local: String,     // Upload of an edited opened file: report the new 3DS time

    // transfer state, kept across reconnects so the job resumes
    items: Vec<Item>,
    expanded: bool,
    next: usize,
    partial: bool, // items[next] was interrupted mid-file
    done: u64,
    total: u64,
    errors: usize,
}

impl Job {
    pub fn new(kind: Kind) -> Job {
        Job {
            kind,
            path: String::new(),
            path2: String::new(),
            locals: Vec::new(),
            remotes: Vec::new(),
            send_paths: Vec::new(),
            notify: 0,
            missing: 0,
            replace: false,
            recycle: true,
            open_after: false,
            track_local: String::new(),
            items: Vec::new(),
            expanded: false,
            next: 0,
            partial: false,
            done: 0,
            total: 0,
            errors: 0,
        }
    }
}

/// a queued job; `wait` blocks until it is finished
pub struct JobHandle {
    job: Mutex<Job>,
    finished: Mutex<Option<bool>>,
    cv: Condvar,
}

impl JobHandle {
    fn new(job: Job) -> Arc<JobHandle> {
        Arc::new(JobHandle { job: Mutex::new(job), finished: Mutex::new(None), cv: Condvar::new() })
    }

    fn finish(&self, ok: bool) {
        *self.finished.lock().unwrap() = Some(ok);
        self.cv.notify_all();
    }

    /// the job itself (its results, once finished)
    pub fn job(&self) -> std::sync::MutexGuard<'_, Job> {
        self.job.lock().unwrap()
    }

    pub fn is_finished(&self) -> bool {
        self.finished.lock().unwrap().is_some()
    }

    /// true when the job succeeded
    pub fn wait(&self) -> bool {
        let mut f = self.finished.lock().unwrap();
        while f.is_none() {
            f = self.cv.wait(f).unwrap();
        }
        f.unwrap()
    }

    pub fn wait_timeout(&self, d: Duration) -> Option<bool> {
        let f = self.finished.lock().unwrap();
        let (f, _) = self.cv.wait_timeout_while(f, d, |f| f.is_none()).unwrap();
        *f
    }
}

pub type JobPtr = Arc<JobHandle>;

pub const WAITING: i32 = 0;
pub const CONNECTED: i32 = 1;
pub const REFUSED: i32 = 2; // waits for reconnect (new password, trusted key)

/// the transfer progress shown under the list
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub active: bool,
    pub up: bool,
    pub del: bool, // deletion: done/total count items, not bytes
    pub name: String,
    pub done: u64,
    pub total: u64, // whole job
    pub file_index: usize,
    pub file_count: usize,
}

struct Queue {
    jobs: VecDeque<JobPtr>,
    running: Option<JobPtr>,
    quit: bool,
    kick: bool,
}

/// one worker's queue and connection state, shared with the window
pub struct Worker {
    q: Mutex<Queue>,
    cv: Condvar,
    pub state: AtomicI32,
    aborter: Mutex<Option<Aborter>>,
}

impl Worker {
    fn new() -> Worker {
        Worker {
            q: Mutex::new(Queue { jobs: VecDeque::new(), running: None, quit: false, kick: false }),
            cv: Condvar::new(),
            state: AtomicI32::new(WAITING),
            aborter: Mutex::new(None),
        }
    }

    pub fn push(&self, job: Job) -> JobPtr {
        let h = JobHandle::new(job);
        {
            let mut q = self.q.lock().unwrap();
            let (kind, notify) = {
                let j = h.job.lock().unwrap();
                (j.kind, j.notify)
            };
            if kind == Kind::List {
                // only the latest listing matters
                let running = q.running.clone();
                q.jobs.retain(|j| {
                    let same = running.as_ref().is_some_and(|r| Arc::ptr_eq(r, j));
                    let jj = j.job.lock().unwrap();
                    same || !(jj.kind == Kind::List && jj.notify == notify)
                });
            }
            q.jobs.push_back(h.clone());
        }
        self.cv.notify_all();
        h
    }

    /// drops the connection and connects again at once
    pub fn reconnect(&self) {
        self.q.lock().unwrap().kick = true;
        self.cv.notify_all();
        self.abort();
    }

    fn abort(&self) {
        if let Some(a) = &*self.aborter.lock().unwrap() {
            a.abort();
        }
    }

    /// drops every queued job (the running one stops via the cancel flag)
    pub fn clear_pending(&self) {
        let mut q = self.q.lock().unwrap();
        let running = q.running.clone();
        q.jobs.retain(|j| {
            let keep = running.as_ref().is_some_and(|r| Arc::ptr_eq(r, j));
            if !keep {
                j.finish(false);
            }
            keep
        });
    }

    pub fn busy(&self) -> bool {
        !self.q.lock().unwrap().jobs.is_empty()
    }

    pub fn connected(&self) -> bool {
        self.state.load(Ordering::Relaxed) == CONNECTED
    }
}

/// everything the workers and the window share
pub struct Shared {
    pub site: Mutex<Site>,
    pub server_info: Mutex<ServerInfo>,
    pub cwd: Mutex<String>,
    pub cancel: AtomicBool,
    pub progress: Mutex<Progress>,
    /// the accent colour Cement should use ("b8283c"; empty: leave it)
    pub accent: Mutex<String>,
    pub browse: Worker,
    pub xfer: Worker,
    sink: Box<dyn Sink>,
}

impl Shared {
    pub fn cwd(&self) -> String {
        self.cwd.lock().unwrap().clone()
    }

    fn message(&self, text: String) {
        self.sink.emit(Event::Message(text));
    }

    /// marks the progress bar active (a transfer job is about to be queued)
    pub fn start_progress(&self) {
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.lock().unwrap().active = true;
    }
}

/// the two workers and their threads
pub struct Client {
    pub shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Client {
    /// Cement at host:port
    pub fn start(host: &str, port: u16, sink: Box<dyn Sink>) -> Client {
        Client::start_site(Site::cement(host, port), sink)
    }

    pub fn start_site(site: Site, sink: Box<dyn Sink>) -> Client {
        // empty: the server's home folder, found when the browse connection opens
        let cwd = site.start_dir.clone();
        let shared = Arc::new(Shared {
            site: Mutex::new(site),
            server_info: Mutex::new(ServerInfo::default()),
            cwd: Mutex::new(cwd),
            cancel: AtomicBool::new(false),
            progress: Mutex::new(Progress::default()),
            accent: Mutex::new(String::new()),
            browse: Worker::new(),
            xfer: Worker::new(),
            sink,
        });
        let threads = [false, true]
            .into_iter()
            .map(|transfer| {
                let s = shared.clone();
                std::thread::Builder::new()
                    .name(if transfer { "transfer" } else { "browse" }.into())
                    .spawn(move || run(&s, transfer))
                    .expect("worker thread")
            })
            .collect();
        Client { shared, threads }
    }

    pub fn stop(&mut self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
        for w in [&self.shared.browse, &self.shared.xfer] {
            w.q.lock().unwrap().quit = true;
            w.cv.notify_all();
            w.abort();
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.stop();
    }
}

fn worker(s: &Shared, transfer: bool) -> &Worker {
    if transfer { &s.xfer } else { &s.browse }
}

fn set_state(s: &Shared, w: &Worker, state: i32) {
    if w.state.swap(state, Ordering::Relaxed) != state {
        s.sink.emit(Event::State);
    }
}

/// waits for reconnect (or quit), at most `timeout`; true on quit
fn wait_kick(w: &Worker, timeout: Option<Duration>) -> bool {
    let q = w.q.lock().unwrap();
    let mut q = match timeout {
        Some(t) => w.cv.wait_timeout_while(q, t, |q| !q.quit && !q.kick).unwrap().0,
        None => w.cv.wait_while(q, |q| !q.quit && !q.kick).unwrap(),
    };
    q.kick = false;
    q.quit
}

fn list_job(path: &str) -> Job {
    let mut j = Job::new(Kind::List);
    j.path = path.to_string();
    j
}

fn cement_of(r: &mut Box<dyn Remote>) -> Option<&mut Ftp> {
    r.cement()
}

fn end_batch(r: &mut Box<dyn Remote>) {
    if let Some(f) = r.cement() {
        f.end_batch();
    }
}

fn run(s: &Arc<Shared>, transfer: bool) {
    let w = worker(s, transfer);
    let mut site = s.site.lock().unwrap().clone();
    let mut r = remote::for_site(&site);
    *w.aborter.lock().unwrap() = Some(r.aborter());
    loop {
        if !r.connected() {
            let now = s.site.lock().unwrap().clone();
            if now.protocol != site.protocol {
                r = remote::for_site(&now);
                *w.aborter.lock().unwrap() = Some(r.aborter());
            }
            site = now;
            let refused = match r.open(&site) {
                Ok(()) => None,
                Err(Error::Ftp(m)) => Some((m, String::new())),
                Err(Error::Untrusted { fingerprint, detail }) => Some((detail, fingerprint)),
                Err(_) => {
                    set_state(s, w, WAITING);
                    if wait_kick(w, Some(Duration::from_secs(2))) {
                        return;
                    }
                    continue;
                }
            };
            if let Some((message, fingerprint)) = refused {
                r.close();
                set_state(s, w, REFUSED);
                if !transfer {
                    s.sink.emit(Event::Refused { message, fingerprint });
                }
                if wait_kick(w, None) {
                    return;
                }
                continue;
            }
            if !transfer {
                // shown next to "Connected", with a hint when the 3DS app is too old
                let info = match cement_of(&mut r).map(|f| f.server_info()) {
                    Some(Ok(i)) => i,
                    _ => ServerInfo { protocol: REQUIRED_PROTOCOL, ..Default::default() }, // unknown: do not warn
                };
                // the accent chosen in Chomp, when Cement shows another one
                let accent = s.accent.lock().unwrap().clone();
                if !accent.is_empty() && info.protocol >= RC2_PROTOCOL && !info.full.accent.eq_ignore_ascii_case(&accent) {
                    if let Some(f) = cement_of(&mut r) {
                        let _ = f.set_accent(&accent);
                    }
                }
                *s.server_info.lock().unwrap() = info;
                if s.cwd().is_empty() {
                    let home = r.home().unwrap_or_else(|_| "/".into());
                    *s.cwd.lock().unwrap() = home;
                }
            }
            set_state(s, w, CONNECTED);
            if !transfer {
                // always show fresh content after (re)connecting
                w.push(list_job(&s.cwd()));
            }
        }

        let job = {
            let q = w.q.lock().unwrap();
            let (mut q, _) = w
                .cv
                .wait_timeout_while(q, idle_wait(s, transfer), |q| !q.quit && !q.kick && q.jobs.is_empty())
                .unwrap();
            if q.quit {
                return;
            }
            if q.kick {
                q.kick = false;
                drop(q);
                r.close();
                set_state(s, w, WAITING);
                continue;
            }
            let job = q.jobs.front().cloned();
            q.running = job.clone();
            job
        };

        let Some(job) = job else {
            // keep-alive, and notice quickly when the server went away; the browse
            // connection also refreshes the SD space and battery in the status bar
            let alive = if !transfer && r.cement().is_some() { refresh_space(s, &mut r) } else { r.keepalive() };
            if alive.is_err() {
                r.close();
                set_state(s, w, WAITING);
            }
            continue;
        };

        let mut ok = false;
        let mut done = true;
        let result = {
            let mut j = job.job.lock().unwrap();
            execute(s, &mut r, &mut j)
        };
        match result {
            Ok(()) => ok = job.job.lock().unwrap().errors == 0,
            Err(Error::Net) => {
                // connection lost: keep the job, it resumes after reconnecting
                r.close();
                set_state(s, w, WAITING);
                done = s.cancel.load(Ordering::Relaxed);
            }
            Err(Error::Cancel) => {
                end_batch(&mut r);
                s.message("Transfer cancelled".into());
            }
            Err(Error::Ftp(msg)) | Err(Error::Untrusted { detail: msg, .. }) => {
                end_batch(&mut r);
                s.message(format!("Error: {}", msg.trim_end_matches(['\r', '\n'])));
            }
        }

        {
            let mut q = w.q.lock().unwrap();
            q.running = None;
            if done {
                q.jobs.retain(|j| !Arc::ptr_eq(j, &job));
            }
        }
        if !done {
            continue;
        }
        job.finish(ok);
        let (kind, open_after, track_local, first) = {
            let j = job.job.lock().unwrap();
            (j.kind, j.open_after, j.track_local.clone(), j.items.first().cloned())
        };
        if ok && (open_after || !track_local.is_empty()) {
            if let Some(first) = first {
                // remember the 3DS file's time, to tell later whether it changed there
                let remote_stamp = r.mtime(&first.remote).ok().flatten().unwrap_or_default();
                let f = Fetched { local: if open_after { first.local } else { track_local }, remote: first.remote, remote_stamp };
                s.sink.emit(if open_after { Event::Opened(f) } else { Event::Sent(f) });
            }
        }
        if transfer && !w.busy() {
            s.progress.lock().unwrap().active = false;
        }
        if !matches!(kind, Kind::List | Kind::Info | Kind::Color) {
            s.browse.push(list_job(&s.cwd())); // show the result
        }
    }
}

/// SD card free/total space and battery for the status bar (Cement only)
/// how long the idle browse connection waits before asking Cement again: often
/// while it installs, so the progress moves
fn idle_wait(s: &Shared, transfer: bool) -> Duration {
    if !transfer && s.server_info.lock().unwrap().full.installing {
        Duration::from_millis(700)
    } else {
        Duration::from_secs(10)
    }
}

fn refresh_space(s: &Shared, r: &mut Box<dyn Remote>) -> Result<()> {
    let Some(ftp) = r.cement() else { return Ok(()) };
    let info = ftp.server_info()?;
    {
        let mut si = s.server_info.lock().unwrap();
        si.has_space = info.has_space;
        si.sd_free = info.sd_free;
        si.sd_total = info.sd_total;
        si.battery = info.battery;
        si.full = info.full;
    }
    s.sink.emit(Event::State);
    Ok(())
}

fn execute(s: &Shared, ftp: &mut Box<dyn Remote>, job: &mut Job) -> Result<()> {
    match job.kind {
        Kind::List => {
            let mut r = ListResult { notify: job.notify, path: job.path.clone(), entries: Vec::new(), ok: false };
            match ftp.list(&job.path) {
                Ok(e) => {
                    r.entries = e;
                    r.ok = true;
                    if job.notify == 0 {
                        refresh_space(s, ftp)?;
                    }
                }
                Err(Error::Ftp(_)) | Err(Error::Untrusted { .. }) => {}
                Err(e) => return Err(e),
            }
            s.sink.emit(Event::List(r));
        }
        Kind::Info => {
            let size = ftp.size(&job.path)?;
            let stamp = if size.is_some() { ftp.mtime(&job.path)?.unwrap_or_default() } else { String::new() };
            s.sink.emit(Event::Info(RemoteInfo {
                local: job.path2.clone(),
                remote: job.path.clone(),
                exists: size.is_some(),
                size: size.unwrap_or(0),
                stamp,
            }));
        }
        Kind::Install | Kind::Color => {
            let protocol = s.server_info.lock().unwrap().protocol;
            let Some(c) = ftp.cement() else { return Err(Error::Ftp("Only Cement can install CIAs".into())) };
            if protocol < RC2_PROTOCOL {
                return Err(Error::Ftp("Update Cement on the 3DS first (install its new CIA with A)".into()));
            }
            if job.kind == Kind::Install {
                let answer = c.install(&job.path)?;
                s.message(format!("{}: {}", base_name(&job.path), answer));
            } else {
                c.set_accent(&job.path)?;
            }
            refresh_space(s, ftp)?;
        }
        Kind::Mkdir => ftp.mkdir(&job.path)?,
        Kind::Rename => {
            ftp.rename(&job.path, &job.path2)?;
            s.message(format!("Renamed {} to {}", base_name(&job.path), base_name(&job.path2)));
        }
        Kind::Delete => delete_items(s, ftp, job)?,
        Kind::Upload | Kind::Download | Kind::SendToPc => transfer(s, ftp, job)?,
    }
    Ok(())
}

fn expand_local(job: &mut Job, local: &str, remote: &str) {
    let Ok(meta) = std::fs::metadata(local) else { return };
    if meta.is_dir() {
        job.items.push(Item { local: local.into(), remote: remote.into(), dir: true, size: 0 });
        let Ok(rd) = std::fs::read_dir(local) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            expand_local(job, &format!("{local}\\{name}"), &join_remote(remote, &name));
        }
    } else {
        job.items.push(Item { local: local.into(), remote: remote.into(), dir: false, size: meta.len() });
        job.total += meta.len();
    }
}

fn expand_remote(ftp: &mut Box<dyn Remote>, job: &mut Job, remote: &str, local: &str, e: &Entry) -> Result<()> {
    if e.dir {
        job.items.push(Item { local: local.into(), remote: remote.into(), dir: true, size: 0 });
        for child in ftp.list(remote)? {
            expand_remote(ftp, job, &join_remote(remote, &child.name), &format!("{local}\\{}", child.name), &child)?;
        }
    } else {
        job.items.push(Item { local: local.into(), remote: remote.into(), dir: false, size: e.size });
        job.total += e.size;
    }
    Ok(())
}

/// folder contents first, the folder itself last
fn expand_delete(ftp: &mut Box<dyn Remote>, job: &mut Job, remote: &str, dir: bool) -> Result<()> {
    if dir {
        for child in ftp.list(remote)? {
            expand_delete(ftp, job, &join_remote(remote, &child.name), child.dir)?;
        }
    }
    job.items.push(Item { local: String::new(), remote: remote.into(), dir, size: 0 });
    Ok(())
}

/// file by file, so no single command runs long on the 3DS; resumable like transfers
fn delete_items(s: &Shared, ftp: &mut Box<dyn Remote>, job: &mut Job) -> Result<()> {
    if !job.expanded {
        for e in job.remotes.clone() {
            expand_delete(ftp, job, &join_remote(&job.path, &e.name), e.dir)?;
        }
        job.expanded = true;
    }
    let mut first_error = String::new();
    let mut resuming = job.partial; // the last attempt was cut off mid-item
    if let Some(c) = ftp.cement() {
        c.announce_batch((job.items.len() - job.next) as u64, 0, "delete")?;
    }
    while job.next < job.items.len() {
        if s.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancel);
        }
        let it = job.items[job.next].clone();
        {
            let mut p = s.progress.lock().unwrap();
            p.active = true;
            p.del = true;
            p.name = base_name(&it.remote).to_string();
            p.file_index = job.next + 1;
            p.file_count = job.items.len();
            p.done = job.next as u64;
            p.total = job.items.len() as u64;
        }
        job.partial = true;
        match ftp.delete(&it.remote, it.dir) {
            Ok(()) => {}
            // after a reconnect the interrupted item may already be gone: that is fine
            Err(Error::Ftp(text)) => {
                if !resuming {
                    job.errors += 1;
                    if first_error.is_empty() {
                        first_error = format!("{}: {}", it.remote, text.trim_end_matches(['\r', '\n']));
                    }
                }
            }
            Err(e) => return Err(e),
        }
        job.partial = false;
        resuming = false;
        job.next += 1;
    }
    end_batch(ftp);
    if job.errors != 0 {
        s.message(format!("Deleted with {} error(s), first: {first_error}", job.errors));
    } else {
        s.message(format!("Deleted {} item(s)", job.items.len()));
    }
    Ok(())
}

fn expand_send_to_pc(ftp: &mut Box<dyn Remote>, job: &mut Job) -> Result<()> {
    if job.replace {
        if win::unsafe_replace_target(&job.path2) {
            return Err(Error::Ftp(format!("Refusing to empty {} (drive or personal/system folder)", job.path2)));
        }
        if !win::empty_folder(&job.path2, job.recycle) {
            return Err(Error::Ftp(format!("Could not empty {} (a file may be in use)", job.path2)));
        }
        job.replace = false; // only once, even if the job resumes after a reconnect
    }
    for rp in job.send_paths.clone() {
        if rp == "/" {
            // the whole SD card: its contents go straight into the PC folder
            let to = job.path2.clone();
            expand_remote(ftp, job, &rp, &to, &Entry { dir: true, ..Default::default() })?;
            continue;
        }
        // each item lands directly in the PC folder: /openmw/logs -> <PC folder>\logs
        let name = base_name(&rp).to_string();
        let local = format!("{}\\{name}", job.path2);
        // find the item in its parent folder to know if it is a folder or a file
        let siblings = match ftp.list(&parent_remote(&rp)) {
            Ok(v) => v,
            Err(Error::Ftp(_)) => Vec::new(),
            Err(e) => return Err(e),
        };
        match siblings.into_iter().find(|e| e.name == name) {
            Some(e) => expand_remote(ftp, job, &rp, &local, &e)?,
            None => job.missing += 1,
        }
    }
    Ok(())
}

fn transfer(s: &Shared, ftp: &mut Box<dyn Remote>, job: &mut Job) -> Result<()> {
    let up = job.kind == Kind::Upload;
    if !job.expanded {
        // the 3DS refuses transfers on a nearly empty battery: say so before starting
        if let Some(c) = ftp.cement() {
            let bat = c.server_info()?.battery;
            if bat.low {
                return Err(Error::Ftp(format!("3DS battery too low ({}): plug in the charger", battery_text(&bat))));
            }
        }
        if up {
            for l in job.locals.clone() {
                let remote = join_remote(&job.path, base_name(&l));
                expand_local(job, &l, &remote);
            }
        } else if job.kind == Kind::SendToPc {
            expand_send_to_pc(ftp, job)?;
        } else {
            for e in job.remotes.clone() {
                let (remote, local) = (join_remote(&job.path, &e.name), format!("{}\\{}", job.path2, e.name));
                expand_remote(ftp, job, &remote, &local, &e)?;
            }
        }
        job.expanded = true;
        if let (true, Some(c)) = (up, ftp.cement()) {
            // refuse up front what can't fit, instead of failing halfway with a full card
            let sizes: Vec<(String, u64)> = job.items.iter().filter(|i| !i.dir).map(|i| (i.remote.clone(), i.size)).collect();
            let (fits, need, avail) = c.fits(&sizes)?;
            if !fits {
                return Err(Error::Ftp(format!(
                    "not enough space on the SD card: needs {}, {} free",
                    format_size(need),
                    format_size(avail)
                )));
            }
        }
    }

    let file_count = job.items.iter().filter(|i| !i.dir).count();
    let files_left = job.items.iter().enumerate().filter(|(i, it)| !it.dir && *i >= job.next).count();
    // let the 3DS (and other clients asking it) know what is left of this job
    if let Some(c) = ftp.cement() {
        c.announce_batch(files_left as u64, job.total - job.done, if up { "up" } else { "down" })?;
    }

    while job.next < job.items.len() {
        if s.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancel);
        }
        let it = job.items[job.next].clone();
        if it.dir {
            if up {
                match ftp.mkdir(&it.remote) {
                    Ok(()) | Err(Error::Ftp(_)) => {} // fails harmlessly if it exists
                    Err(e) => return Err(e),
                }
            } else {
                let _ = std::fs::create_dir(&it.local);
            }
            job.next += 1;
            continue;
        }
        let index = job.items[..=job.next].iter().filter(|i| !i.dir).count();

        // resume an interrupted file where it stopped
        let mut offset = 0;
        if job.partial {
            if up {
                if let Some(remote) = ftp.size(&it.remote)? {
                    if remote <= it.size {
                        offset = remote;
                    }
                }
            } else if let Ok(m) = std::fs::metadata(&it.local) {
                offset = it.size.min(m.len());
            }
        }

        let base = job.done;
        {
            let mut p = s.progress.lock().unwrap();
            p.active = true;
            p.up = up;
            p.del = false;
            p.name = base_name(&it.remote).to_string();
            p.file_index = index;
            p.file_count = file_count;
            p.total = job.total;
            p.done = base + offset;
        }
        let mut file_done = offset;
        let mut progress = |n: u64| -> Result<()> {
            file_done += n;
            s.progress.lock().unwrap().done = base + file_done;
            if s.cancel.load(Ordering::Relaxed) { Err(Error::Cancel) } else { Ok(()) }
        };

        job.partial = true;
        let r = if up {
            ftp.upload(&it.local, &it.remote, offset, &mut progress)
        } else {
            ftp.download(&it.remote, &it.local, offset, &mut progress)
        };
        match r {
            Ok(()) => {}
            Err(Error::Ftp(msg)) => {
                if battery_refusal(&msg) {
                    job.partial = false;
                    return Err(Error::Ftp(msg)); // the rest would be refused too
                }
                job.errors += 1;
                s.message(format!("Error: {}", msg.trim_end_matches(['\r', '\n'])));
            }
            Err(Error::Cancel) => {
                if !up {
                    let _ = std::fs::remove_file(&it.local);
                }
                return Err(Error::Cancel);
            }
            Err(Error::Untrusted { detail, .. }) => {
                job.errors += 1;
                s.message(format!("Error: {detail}"));
            }
            Err(Error::Net) => return Err(Error::Net), // remember how far this job got
        }
        job.partial = false;
        job.done = base + it.size;
        job.next += 1;
    }
    end_batch(ftp);

    let missing = if job.missing != 0 { format!("  ({} item(s) not found on the 3DS)", job.missing) } else { String::new() };
    if job.errors == 0 {
        let what = match job.kind {
            Kind::Upload => "Upload",
            Kind::SendToPc => "Send to PC",
            _ => "Download",
        };
        s.message(format!("{what} complete: {file_count} file(s), {}{missing}", format_size(job.total)));
    }
    Ok(())
}
