// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! baguette-cli - command-line access to Cement, the FTP server on the 3DS, for
//! scripts and automation. Uses the same FTP engine and saved settings as Baguette.exe.
//!
//! Non-interactive: results on stdout (text or --json), progress on stderr.
//! Exit codes: 0 ok, 1 usage, 2 3DS unreachable, 3 operation failed, 4 still busy.
//! Commands that change files first wait until no other transfer is running,
//! announce their own job to the 3DS (SITE BATCH) and resume after dropped links.

use std::io::Write;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use baguette_core::ftp::{Entry, Error, Ftp, first_line};
use baguette_core::status::{Battery, Status, battery_refusal, battery_text, parse_status};
use baguette_core::util::{base_name, format_size, join_remote, json_str, parent_remote, parse_i64};
use baguette_core::{REQUIRED_PROTOCOL, VERSION, config};

const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 1;
const EXIT_UNREACHABLE: u8 = 2;
const EXIT_FAILED: u8 = 3;
const EXIT_BUSY: u8 = 4;

/// why a command stopped
enum Fail {
    Unreachable,
    Busy(String),
    Ftp(String),
    Usage(String),
}

type CliResult<T> = Result<T, Fail>;

static JSON: AtomicBool = AtomicBool::new(false);
static QUIET: AtomicBool = AtomicBool::new(false);
static TTY: AtomicBool = AtomicBool::new(false); // stderr is a console: progress can redraw one line
static PROGRESS_JSON: AtomicBool = AtomicBool::new(false);

fn json() -> bool {
    JSON.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// output

fn out(s: &str) {
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(s.as_bytes());
    let _ = o.flush();
}

fn note(s: &str) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }
    let mut e = std::io::stderr().lock();
    if TTY.load(Ordering::Relaxed) {
        let _ = e.write_all(b"\r\x1b[K");
    }
    let _ = writeln!(e, "{s}");
    let _ = e.flush();
}

fn time_text(s: i64) -> String {
    if s < 0 { "unknown time".into() } else { format!("{}:{:02}", s / 60, s % 60) }
}

// ---------------------------------------------------------------------------
// connection with retry

struct Link {
    host: String,
    port: u16,
    wait: u64,
    ftp: Ftp,
    // what is left of our job; re-announced after a reconnect
    batch_files: u64,
    batch_bytes: u64,
    batch_kind: &'static str,
}

impl Link {
    fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn batch(&mut self, files: u64, bytes: u64, kind: &'static str) {
        self.batch_files = files;
        self.batch_bytes = bytes;
        self.batch_kind = kind;
    }

    fn connect(&mut self) -> CliResult<()> {
        if self.ftp.connected() {
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(self.wait);
        let mut told = false;
        // a 3DS with a login: the one saved with its site in Baguette
        let mut login = baguette_core::ftp::Login::default();
        if let Some((user, pass)) = baguette_core::sites::find_login(&self.host, self.port) {
            login.user = user;
            login.pass = pass;
        }
        loop {
            match self.ftp.open(&self.host, self.port, login.clone()) {
                Ok(()) => break,
                Err(Error::Ftp(m)) => {
                    return Err(Fail::Ftp(format!("{m} (save the 3DS's user name and password with its site in Baguette)")));
                }
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(Fail::Unreachable);
            }
            if !told {
                note(&format!("waiting for the 3DS at {}...", self.address()));
            }
            told = true;
            std::thread::sleep(Duration::from_secs(1));
        }
        if self.batch_files != 0 && self.ftp.announce_batch(self.batch_files, self.batch_bytes, self.batch_kind).is_err() {
            self.ftp.close(); // handled by the next attempt
        }
        Ok(())
    }

    /// runs f, reconnecting (up to --wait seconds) whenever the connection drops
    fn run<T>(&mut self, mut f: impl FnMut(&mut Ftp) -> baguette_core::ftp::Result<T>) -> CliResult<T> {
        loop {
            self.connect()?;
            if !self.ftp.connected() {
                continue;
            }
            match f(&mut self.ftp) {
                Ok(v) => return Ok(v),
                Err(Error::Net) => {
                    self.ftp.close();
                    note("connection lost, reconnecting...");
                }
                Err(Error::Ftp(m)) | Err(Error::Untrusted { detail: m, .. }) => return Err(Fail::Ftp(m)),
                Err(Error::Cancel) => return Err(Fail::Ftp("cancelled".into())),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// what the 3DS is doing (SITE STATUS)

fn query_status(link: &mut Link) -> CliResult<Status> {
    let (code, text) = link.run(|f| f.cmd_text("SITE STATUS"))?;
    Ok(parse_status(code, &text))
}

/// one line per activity, for humans and for "waiting because ..." notes
fn describe(st: &Status) -> Vec<String> {
    let mut lines = Vec::new();
    for t in &st.transfers {
        if t.dir == "up" || t.dir == "down" {
            let mut l = format!("{}{}  {}", if t.dir == "up" { "upload " } else { "download " }, t.file, format_size(t.done));
            if t.size != 0 {
                l += &format!(" / {}", format_size(t.size));
            }
            if t.bps != 0 {
                l += &format!("  {}/s", format_size(t.bps));
            }
            if t.eta >= 0 {
                l += &format!("  {} left", time_text(t.eta));
            }
            lines.push(format!("{l}  (from {})", t.peer));
        }
        if !t.batch.is_empty() {
            let mut l = format!(
                "{}{}/{}{}",
                match t.batch.as_str() {
                    "delete" => "delete job: ",
                    "down" => "download job: ",
                    _ => "upload job: ",
                },
                t.batch_files_done,
                t.batch_files,
                if t.batch == "delete" { " items" } else { " files" }
            );
            if t.batch_bytes != 0 {
                l += &format!(", {} / {}", format_size(t.batch_bytes_done), format_size(t.batch_bytes));
            }
            l += &format!(", {} left  (from {})", time_text(t.batch_eta), t.peer);
            lines.push(l);
        }
    }
    if st.installing {
        let pct = if st.install_size != 0 { st.install_done * 100 / st.install_size } else { 0 };
        lines.push(format!("installing CIA {} {pct}%", st.install_file));
    }
    if st.busy && lines.is_empty() {
        lines.push("files were just changed (deletion or rename in progress)".into());
    }
    lines
}

fn status_json(st: &Status, address: &str) -> String {
    let transfers: Vec<String> = st
        .transfers
        .iter()
        .map(|t| {
            let job = if t.batch.is_empty() {
                "null".to_string()
            } else {
                format!(
                    "{{\"kind\":{},\"files_done\":{},\"files\":{},\"bytes_done\":{},\"bytes\":{},\"eta_seconds\":{}}}",
                    json_str(&t.batch),
                    t.batch_files_done,
                    t.batch_files,
                    t.batch_bytes_done,
                    t.batch_bytes,
                    t.batch_eta
                )
            };
            format!(
                "{{\"peer\":{},\"direction\":{},\"file\":{},\"done\":{},\"size\":{},\"bytes_per_second\":{},\"eta_seconds\":{},\"job\":{}}}",
                json_str(&t.peer),
                json_str(&t.dir),
                json_str(&t.file),
                t.done,
                t.size,
                t.bps,
                t.eta,
                job
            )
        })
        .collect();
    let install = if st.installing {
        format!("{{\"file\":{},\"done\":{},\"size\":{}}}", json_str(&st.install_file), st.install_done, st.install_size)
    } else {
        "null".into()
    };
    let b = &st.battery;
    let battery = if b.known {
        format!(
            "{{\"percent\":{},\"bars\":{},\"charging\":{},\"plugged\":{},\"low\":{}}}",
            if b.percent >= 0 { b.percent.to_string() } else { "null".into() },
            b.bars,
            b.charging,
            b.plugged,
            b.low
        )
    } else {
        "null".into()
    };
    let opt = |has: bool, v: u64| if has { v.to_string() } else { "null".into() };
    // outdated only when this tool needs a newer 3DS app, not for every new build
    let outdated = st.protocol < REQUIRED_PROTOCOL;
    format!(
        "{{\"ok\":true,\"address\":{},\"version\":{},\"client_version\":{},\"protocol\":{},\"app_outdated\":{},\"status_supported\":{},\"busy\":{},\"idle_ms\":{},\"connections\":{},\"sd_free\":{},\"sd_total\":{},\"battery\":{},\"transfers\":[{}],\"install\":{}{}}}\n",
        json_str(address),
        json_str(&st.version),
        json_str(VERSION),
        st.protocol,
        outdated,
        st.known,
        st.busy,
        st.idle_ms,
        st.connections,
        opt(st.has_space, st.sd_free),
        opt(st.has_space, st.sd_total),
        battery,
        transfers.join(","),
        install,
        if st.known {
            ""
        } else {
            ",\"warning\":\"this 3DS app does not report its transfers, so busy/idle are unknown: install the new Cement.cia\""
        }
    )
}

/// blocks until nothing else is transferring, up to budget seconds
fn wait_idle(link: &mut Link, budget: u64) -> CliResult<()> {
    let start = Instant::now();
    let mut last_note: Option<Instant> = None;
    loop {
        let st = query_status(link)?;
        if !st.known || !st.busy {
            return Ok(());
        }
        let what = describe(&st).join("; ");
        if start.elapsed() >= Duration::from_secs(budget) {
            return Err(Fail::Busy(what));
        }
        if last_note.is_none_or(|t| t.elapsed() >= Duration::from_secs(15)) {
            note(&format!("3DS busy, waiting: {what}"));
            last_note = Some(Instant::now());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

// ---------------------------------------------------------------------------
// helpers

/// "/a/b": forward slashes, leading slash, no trailing slash. An "sd:" prefix
/// (sd:/3ds/app) is accepted: shells like Git Bash rewrite a leading "/".
fn remote_path(arg: &str) -> String {
    let mut p = arg.to_string();
    if p.len() >= 3 && p.is_char_boundary(3) && p[..3].eq_ignore_ascii_case("sd:") {
        p.drain(..3);
    }
    let p = p.replace('\\', "/");
    let mut out = String::new();
    for c in format!("/{p}").chars() {
        if !(c == '/' && out.ends_with('/')) {
            out.push(c);
        }
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// looks the item up in its parent folder; None if it does not exist
fn stat_remote(link: &mut Link, path: &str) -> CliResult<Option<Entry>> {
    if path == "/" {
        return Ok(Some(Entry { name: "/".into(), dir: true, ..Default::default() }));
    }
    let name = base_name(path).to_string();
    link.run(|f| match f.list(&parent_remote(path)) {
        Ok(siblings) => Ok(siblings.into_iter().rfind(|e| e.name == name)),
        Err(Error::Ftp(_)) => Ok(None), // parent missing
        Err(e) => Err(e),
    })
}

fn mkdir1(link: &mut Link, path: &str) -> CliResult<()> {
    link.run(|f| f.cmd(&format!("MKD {path}"), None).map(|_| ())) // fails harmlessly when it exists
}

fn mkdirs(link: &mut Link, path: &str) -> CliResult<()> {
    let mut pos = 1;
    while pos <= path.len() {
        let next = path[pos..].find('/').map_or(path.len(), |n| n + pos);
        mkdir1(link, &path[..next])?;
        pos = next + 1;
    }
    if path != "/" && !stat_remote(link, path)?.is_some_and(|e| e.dir) {
        return Err(Fail::Ftp(format!("could not create folder {path}")));
    }
    Ok(())
}

#[derive(Clone)]
struct Item {
    local: String,
    remote: String,
    dir: bool,
    size: u64,
}

struct Totals {
    files: u64,
    folders: u64,
    bytes: u64,
    errors: Vec<String>,
    start: Instant,
}

impl Totals {
    fn new() -> Totals {
        Totals { files: 0, folders: 0, bytes: 0, errors: Vec::new(), start: Instant::now() }
    }
}

/// --progress-json: one JSON line on stdout per finished item, plus a heartbeat
/// every 5 s during long files, so a caller can follow a command that outlives
/// its own timeout. The final summary stays the last line.
struct JobStats {
    action: &'static str,
    files: u64,
    files_done: u64,
    bytes: u64,
    bytes_done: u64,
    start: Instant,
}

static JOB: Mutex<Option<JobStats>> = Mutex::new(None);

fn job_start(action: &'static str, files: u64, bytes: u64) {
    *JOB.lock().unwrap() = Some(JobStats { action, files, files_done: 0, bytes, bytes_done: 0, start: Instant::now() });
}

fn job_file_done(bytes: u64) {
    if let Some(j) = JOB.lock().unwrap().as_mut() {
        j.files_done += 1;
        j.bytes_done += bytes;
    }
}

fn progress_event(event: &str, path: &str, file_done: u64, file_size: u64, error: &str) {
    if !PROGRESS_JSON.load(Ordering::Relaxed) {
        return;
    }
    let j = JOB.lock().unwrap();
    let Some(j) = j.as_ref() else { return };
    let bytes_done = j.bytes_done + if event == "progress" { file_done } else { 0 };
    out(&format!(
        "{{\"event\":\"{event}\",\"action\":\"{}\",\"path\":{},\"file_done\":{file_done},\"file_size\":{file_size},\"files_done\":{},\"files\":{},\"bytes_done\":{bytes_done},\"bytes\":{},\"elapsed_s\":{:.1}{}}}\n",
        j.action,
        json_str(path),
        j.files_done,
        j.files,
        j.bytes,
        j.start.elapsed().as_secs_f64(),
        if error.is_empty() { String::new() } else { format!(",\"error\":{}", json_str(error)) }
    ));
}

/// progress callback: one redrawn line on a console, nothing when captured
fn progress_line(name: &str, size: u64, start: u64) -> impl FnMut(u64) -> baguette_core::ftp::Result<()> {
    let name = name.to_string();
    let mut done = start;
    let mut last: Option<Instant> = None;
    let mut beat = Instant::now();
    move |n| {
        done += n;
        if PROGRESS_JSON.load(Ordering::Relaxed) && beat.elapsed() >= Duration::from_secs(5) {
            beat = Instant::now();
            progress_event("progress", &name, done, size, "");
        }
        if !TTY.load(Ordering::Relaxed) || QUIET.load(Ordering::Relaxed) || last.is_some_and(|t| t.elapsed() < Duration::from_millis(200)) {
            return Ok(());
        }
        last = Some(Instant::now());
        let mut e = std::io::stderr().lock();
        let _ = write!(e, "\r\x1b[K  {name}  {} / {}", format_size(done), format_size(size));
        let _ = e.flush();
        Ok(())
    }
}

fn collect_local(local: &str, remote_dir: &str, items: &mut Vec<Item>, t: &mut Totals) {
    let Ok(meta) = std::fs::metadata(local) else {
        t.errors.push(format!("local path not found: {local}"));
        return;
    };
    let remote = join_remote(remote_dir, base_name(local));
    if !meta.is_dir() {
        items.push(Item { local: local.into(), remote, dir: false, size: meta.len() });
        return;
    }
    items.push(Item { local: local.into(), remote: remote.clone(), dir: true, size: 0 });
    let mut names: Vec<String> = std::fs::read_dir(local)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort(); // FindFirstFile order on NTFS
    for name in names {
        collect_local(&format!("{local}\\{name}"), &remote, items, t);
    }
}

fn collect_remote(link: &mut Link, remote: &str, e: &Entry, local: &str, items: &mut Vec<Item>) -> CliResult<()> {
    items.push(Item { local: local.into(), remote: remote.into(), dir: e.dir, size: e.size });
    if !e.dir {
        return Ok(());
    }
    let children = link.run(|f| f.list(remote))?;
    for c in children {
        collect_remote(link, &join_remote(remote, &c.name), &c, &format!("{local}\\{}", c.name), items)?;
    }
    Ok(())
}

fn collect_delete(link: &mut Link, path: &str, dir: bool, items: &mut Vec<Item>) -> CliResult<()> {
    if dir {
        let children = link.run(|f| f.list(path))?;
        for c in children {
            collect_delete(link, &join_remote(path, &c.name), c.dir, items)?;
        }
    }
    items.push(Item { local: String::new(), remote: path.into(), dir, size: 0 }); // contents first, the folder last
    Ok(())
}

fn upload_file(link: &mut Link, it: &Item) -> CliResult<()> {
    let mut retry = false;
    link.run(|f| {
        let mut offset = 0;
        if retry {
            // resume after a dropped connection
            if let Some(have) = f.size(&it.remote)? {
                if have <= it.size {
                    offset = have;
                }
            }
        }
        retry = true;
        let mut progress = progress_line(base_name(&it.remote), it.size, offset);
        f.upload(&it.local, &it.remote, offset, &mut progress)
    })?;
    let have = link.run(|f| f.size(&it.remote))?.unwrap_or(0);
    if have != it.size {
        return Err(Fail::Ftp(format!(
            "size check failed for {} ({have} of {} bytes on the 3DS)",
            it.remote, it.size
        )));
    }
    Ok(())
}

fn download_file(link: &mut Link, it: &Item) -> CliResult<()> {
    let mut retry = false;
    link.run(|f| {
        let mut offset = 0;
        if retry {
            if let Ok(m) = std::fs::metadata(&it.local) {
                offset = it.size.min(m.len());
            }
        }
        retry = true;
        let mut progress = progress_line(base_name(&it.remote), it.size, offset);
        f.download(&it.remote, &it.local, offset, &mut progress)
    })
}

fn print_summary(verb: &str, t: &Totals) {
    let s = t.start.elapsed().as_secs_f64();
    let rate = if s > 0.0 { t.bytes as f64 / s } else { 0.0 };
    if json() {
        let errs: Vec<String> = t.errors.iter().map(|e| json_str(e)).collect();
        out(&format!(
            "{{\"ok\":{},\"files\":{},\"folders\":{},\"bytes\":{},\"seconds\":{s:.2},\"bytes_per_second\":{rate:.0},\"errors\":[{}]{}}}\n",
            t.errors.is_empty(),
            t.files,
            t.folders,
            t.bytes,
            errs.join(","),
            if PROGRESS_JSON.load(Ordering::Relaxed) { ",\"event\":\"done\"" } else { "" }
        ));
        return;
    }
    out(&format!(
        "{verb} {} file(s), {} folder(s), {} in {s:.1} s ({}/s)\n",
        t.files,
        t.folders,
        format_size(t.bytes),
        format_size(rate as u64)
    ));
    for e in &t.errors {
        out(&format!("error: {e}\n"));
    }
}

// ---------------------------------------------------------------------------
// commands

struct Options {
    busy_wait: u64,
    no_wait: bool,
    contents: bool, // get: copy a folder's contents, not the folder
}

fn icompare(a: &str, b: &str) -> std::cmp::Ordering {
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

fn run(link: &mut Link, cmd: &str, args: &[String], opt: &Options) -> CliResult<u8> {
    let need = |n: usize| -> CliResult<()> {
        if args.len() < n { Err(Fail::Usage(format!("'{cmd}' needs more arguments"))) } else { Ok(()) }
    };
    // commands that change files: never start on top of another transfer
    let idle = |link: &mut Link| -> CliResult<()> { if opt.no_wait { Ok(()) } else { wait_idle(link, opt.busy_wait) } };
    // the 3DS refuses transfers on a nearly empty battery: fail before starting
    let battery_check = |link: &mut Link| -> CliResult<()> {
        let b: Battery = query_status(link)?.battery;
        if b.low {
            return Err(Fail::Ftp(format!("3DS battery too low ({}): plug in the charger", battery_text(&b))));
        }
        Ok(())
    };

    match cmd {
        "status" => {
            let st = query_status(link)?;
            if json() {
                out(&status_json(&st, &link.address()));
            } else {
                out(&format!("connected to {}\n", link.address()));
                out(&format!(
                    "3DS app: {}\n",
                    if st.version.is_empty() { "unknown version".to_string() } else { format!("v{}", st.version) }
                ));
                if st.protocol < REQUIRED_PROTOCOL {
                    out(&format!("the 3DS app is too old for this tool (v{VERSION}): install the new CIA\n"));
                }
                if st.has_space {
                    out(&format!("SD card: {} free of {}\n", format_size(st.sd_free), format_size(st.sd_total)));
                }
                if st.battery.known {
                    out(&format!(
                        "battery: {}{}\n",
                        battery_text(&st.battery),
                        if st.battery.low { " (too low: transfers are refused until the 3DS is plugged in)" } else { "" }
                    ));
                }
                if !st.known {
                    out("this server does not report its activity (SITE STATUS)\n");
                } else if !st.busy {
                    out(&format!("idle for {} s\n", st.idle_ms / 1000));
                } else {
                    out("busy:\n");
                    for l in describe(&st) {
                        out(&format!("  {l}\n"));
                    }
                }
            }
            Ok(EXIT_OK)
        }

        "df" => {
            let mut st = query_status(link)?;
            if !st.has_space {
                // older 3DS app or another FTP server: AVBL gives the free space alone
                if let Some(free) = link.run(|f| f.free_bytes())? {
                    st.sd_free = free;
                    st.has_space = true;
                }
            }
            if !st.has_space {
                return Err(Fail::Ftp("this server does not report free space (install the new Cement.cia)".into()));
            }
            let used = st.sd_total.saturating_sub(st.sd_free);
            if json() {
                let total = if st.sd_total != 0 { st.sd_total.to_string() } else { "null".into() };
                let used = if st.sd_total != 0 { used.to_string() } else { "null".into() };
                out(&format!("{{\"ok\":true,\"free\":{},\"total\":{total},\"used\":{used}}}\n", st.sd_free));
            } else if st.sd_total != 0 {
                out(&format!(
                    "SD card: {} free of {} ({}% used)\n",
                    format_size(st.sd_free),
                    format_size(st.sd_total),
                    used * 100 / st.sd_total
                ));
            } else {
                out(&format!("SD card: {} free\n", format_size(st.sd_free)));
            }
            Ok(EXIT_OK)
        }

        "wait" => {
            wait_idle(link, opt.busy_wait)?;
            out(if json() { "{\"ok\":true,\"busy\":false}\n" } else { "idle\n" });
            Ok(EXIT_OK)
        }

        "ls" => {
            let path = remote_path(args.first().map_or("/", String::as_str));
            let (mut entries, error) = link.run(|f| match f.list(&path) {
                Ok(e) => Ok((e, String::new())),
                Err(Error::Ftp(m)) => Ok((Vec::new(), first_line(&m).to_string())),
                Err(e) => Err(e),
            })?;
            entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| icompare(&a.name, &b.name)));
            if json() {
                let list: Vec<String> = entries
                    .iter()
                    .map(|e| {
                        format!("{{\"name\":{},\"type\":\"{}\",\"size\":{}}}", json_str(&e.name), if e.dir { "dir" } else { "file" }, e.size)
                    })
                    .collect();
                out(&format!(
                    "{{\"ok\":{},\"path\":{}{},\"entries\":[{}]}}\n",
                    error.is_empty(),
                    json_str(&path),
                    if error.is_empty() { String::new() } else { format!(",\"error\":{}", json_str(&error)) },
                    list.join(",")
                ));
            } else if error.is_empty() {
                for e in &entries {
                    let size = if e.dir { String::new() } else { e.size.to_string() };
                    out(&format!("{:<5} {:>12}  {}\n", if e.dir { "dir" } else { "file" }, size, e.name));
                }
            } else {
                out(&format!("error: {error}\n"));
            }
            Ok(if error.is_empty() { EXIT_OK } else { EXIT_FAILED })
        }

        "stat" => {
            need(1)?;
            let path = remote_path(&args[0]);
            let found = stat_remote(link, &path)?;
            match (&found, json()) {
                (Some(e), true) => out(&format!(
                    "{{\"ok\":true,\"path\":{},\"type\":\"{}\",\"size\":{}}}\n",
                    json_str(&path),
                    if e.dir { "dir" } else { "file" },
                    e.size
                )),
                (None, true) => out(&format!("{{\"ok\":false,\"path\":{},\"error\":\"not found\"}}\n", json_str(&path))),
                (Some(e), false) => out(&format!("{} {} {path}\n", if e.dir { "dir" } else { "file" }, e.size)),
                (None, false) => out(&format!("not found: {path}\n")),
            }
            Ok(if found.is_some() { EXIT_OK } else { EXIT_FAILED })
        }

        "put" => {
            need(2)?;
            let dir = remote_path(args.last().unwrap());
            let mut t = Totals::new();
            let mut items = Vec::new();
            for a in &args[..args.len() - 1] {
                let mut local = a.clone();
                while local.len() > 1 && (local.ends_with('\\') || local.ends_with('/')) && !local[..local.len() - 1].ends_with(':') {
                    local.pop();
                }
                collect_local(&local, &dir, &mut items, &mut t);
            }
            let mut files = items.iter().filter(|i| !i.dir).count() as u64;
            let mut bytes: u64 = items.iter().map(|i| i.size).sum();
            idle(link)?;
            battery_check(link)?;
            // refuse up front what can't fit, instead of failing halfway with a full card
            let sizes: Vec<(String, u64)> = items.iter().filter(|i| !i.dir).map(|i| (i.remote.clone(), i.size)).collect();
            let (fit, need_bytes, avail) = link.run(|f| f.fits(&sizes))?;
            if !fit {
                return Err(Fail::Ftp(format!(
                    "not enough space on the SD card: needs {}, {} free",
                    format_size(need_bytes),
                    format_size(avail)
                )));
            }
            mkdirs(link, &dir)?;
            link.batch(files, bytes, "up");
            link.run(|f| f.announce_batch(files, bytes, "up"))?;
            job_start("put", files, bytes);
            for it in &items {
                if it.dir {
                    mkdir1(link, &it.remote)?;
                    t.folders += 1;
                    continue;
                }
                match upload_file(link, it) {
                    Ok(()) => {
                        note(&format!("put {} ({})", it.remote, format_size(it.size)));
                        t.files += 1;
                        t.bytes += it.size;
                        job_file_done(it.size);
                        progress_event("file", &it.remote, it.size, it.size, "");
                    }
                    Err(Fail::Ftp(msg)) => {
                        t.errors.push(first_line(&msg).to_string());
                        progress_event("error", &it.remote, 0, it.size, first_line(&msg));
                        if battery_refusal(&msg) {
                            break; // the rest would be refused too
                        }
                    }
                    Err(e) => return Err(e),
                }
                files -= 1;
                bytes -= it.size;
                link.batch(files, bytes, "up");
            }
            link.batch(0, 0, "up");
            link.run(|f| {
                f.end_batch();
                Ok(())
            })?;
            print_summary("uploaded", &t);
            Ok(if t.errors.is_empty() { EXIT_OK } else { EXIT_FAILED })
        }

        "get" => {
            need(2)?;
            let dir = args.last().unwrap().clone();
            let mut t = Totals::new();
            let mut items = Vec::new();
            for a in &args[..args.len() - 1] {
                let remote = remote_path(a);
                let found = if remote == "/" { None } else { stat_remote(link, &remote)? };
                match found {
                    None => t.errors.push(format!("not found on the 3DS: {remote}")),
                    Some(e) if e.dir && opt.contents => {
                        // --contents: the folder's contents go straight into local-dir
                        let children = link.run(|f| f.list(&remote))?;
                        for c in children {
                            collect_remote(link, &join_remote(&remote, &c.name), &c, &format!("{dir}\\{}", c.name), &mut items)?;
                        }
                    }
                    Some(e) => collect_remote(link, &remote, &e, &format!("{dir}\\{}", e.name), &mut items)?,
                }
            }
            let mut files = items.iter().filter(|i| !i.dir).count() as u64;
            let mut bytes: u64 = items.iter().map(|i| i.size).sum();
            idle(link)?;
            battery_check(link)?;
            let _ = std::fs::create_dir_all(&dir);
            link.batch(files, bytes, "down");
            link.run(|f| f.announce_batch(files, bytes, "down"))?;
            job_start("get", files, bytes);
            for it in &items {
                if it.dir {
                    let _ = std::fs::create_dir(&it.local);
                    t.folders += 1;
                    continue;
                }
                match download_file(link, it) {
                    Ok(()) => {
                        note(&format!("got {} ({})", it.remote, format_size(it.size)));
                        t.files += 1;
                        t.bytes += it.size;
                        job_file_done(it.size);
                        progress_event("file", &it.remote, it.size, it.size, "");
                    }
                    Err(Fail::Ftp(msg)) => {
                        t.errors.push(first_line(&msg).to_string());
                        progress_event("error", &it.remote, 0, it.size, first_line(&msg));
                        if battery_refusal(&msg) {
                            break; // the rest would be refused too
                        }
                    }
                    Err(e) => return Err(e),
                }
                files -= 1;
                bytes -= it.size;
                link.batch(files, bytes, "down");
            }
            link.batch(0, 0, "down");
            link.run(|f| {
                f.end_batch();
                Ok(())
            })?;
            print_summary("downloaded", &t);
            Ok(if t.errors.is_empty() { EXIT_OK } else { EXIT_FAILED })
        }

        "rm" => {
            need(1)?;
            let mut t = Totals::new();
            let mut items = Vec::new();
            for a in args {
                let path = remote_path(a);
                if path == "/" {
                    return Err(Fail::Usage("refusing to delete the whole SD card".into()));
                }
                match stat_remote(link, &path)? {
                    None => t.errors.push(format!("not found on the 3DS: {path}")),
                    Some(e) => collect_delete(link, &path, e.dir, &mut items)?,
                }
            }
            idle(link)?;
            let mut left = items.len() as u64;
            link.batch(left, 0, "delete");
            link.run(|f| f.announce_batch(left, 0, "delete"))?;
            job_start("rm", left, 0);
            for it in &items {
                let (code, text) = link.run(|f| f.cmd_text(&format!("{} {}", if it.dir { "RMD" } else { "DELE" }, it.remote)))?;
                // after a reconnect the item may already be gone: only report what still exists
                if let Some(j) = JOB.lock().unwrap().as_mut() {
                    j.files_done += 1;
                }
                if code / 100 != 2 && stat_remote(link, &it.remote)?.is_some() {
                    t.errors.push(format!("{}: {}", it.remote, first_line(&text)));
                    progress_event("error", &it.remote, 0, 0, first_line(&text));
                } else {
                    if it.dir {
                        t.folders += 1;
                    } else {
                        t.files += 1;
                    }
                    progress_event("file", &it.remote, 0, 0, "");
                }
                left -= 1;
                link.batch(left, 0, "delete");
            }
            link.batch(0, 0, "delete");
            link.run(|f| {
                f.end_batch();
                Ok(())
            })?;
            print_summary("deleted", &t);
            Ok(if t.errors.is_empty() { EXIT_OK } else { EXIT_FAILED })
        }

        "mkdir" => {
            need(1)?;
            idle(link)?;
            for a in args {
                mkdirs(link, &remote_path(a))?;
            }
            out(if json() { "{\"ok\":true}\n" } else { "ok\n" });
            Ok(EXIT_OK)
        }

        "mv" => {
            need(2)?;
            let (from, to) = (remote_path(&args[0]), remote_path(&args[1]));
            idle(link)?;
            link.run(|f| {
                f.simple(&format!("RNFR {from}"), 350)?;
                f.simple(&format!("RNTO {to}"), 250)
            })?;
            out(if json() { "{\"ok\":true}\n" } else { "ok\n" });
            Ok(EXIT_OK)
        }

        _ => Err(Fail::Usage(format!("unknown command '{cmd}'"))),
    }
}

const USAGE: &str = "baguette-cli - talk to Cement, the FTP server app on the 3DS (SD card only)

usage: baguette-cli [options] <command> [arguments]

commands:
  status                         reachable? busy (transfers, ETA, CIA install)? SD space, battery
  wait                           block until no transfer/deletion is running
  ls [remote-dir]                list a folder (default /)
  stat <remote>                  type and size of a file/folder
  df                             free and total space on the SD card
  put <local>... <remote-dir>    upload files/folders into remote-dir (created if needed)
  get <remote>... <local-dir>    download files/folders into local-dir
  rm <remote>...                 delete files/folders (folders recursively)
  mkdir <remote>...              create folders (with parents)
  mv <from> <to>                 rename or move

options:
  --host <ip> --port <n>         3DS address (default: the one saved by Baguette.exe)
  --wait <seconds>               keep retrying an unreachable 3DS / dropped link (default 20)
  --busy-wait <seconds>          how long put/get/rm/mkdir/mv and wait wait for other
                                 transfers to finish (default 600)
  --no-wait                      start even if another transfer is running
  --json                         print one JSON object on stdout
  --quiet                        no progress messages on stderr
  --progress-json                implies --json; also print one JSON line per finished
                                 file (and every 5 s during long ones) before the summary
  --contents                     get: copy a folder's contents into local-dir, not the folder
  --version                      print this tool's version

exit codes: 0 ok, 1 usage, 2 3DS unreachable, 3 operation failed, 4 still busy after --busy-wait
remote paths are absolute on the SD card, e.g. /3ds/app/app.3dsx or sd:/3ds/app/app.3dsx
(use the sd: form from Git Bash, which rewrites arguments starting with /)
";

fn console_setup() {
    use windows_sys::Win32::System::Console::*;
    unsafe {
        SetConsoleOutputCP(65001); // UTF-8
        let err = GetStdHandle(STD_ERROR_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(err, &mut mode) != 0 {
            TTY.store(true, Ordering::Relaxed);
            SetConsoleMode(err, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

fn main() -> ExitCode {
    console_setup();
    let argv: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    let mut host = String::new();
    let (mut port, mut wait): (u16, u64) = (0, 20);
    let mut opt = Options { busy_wait: 600, no_wait: false, contents: false };
    let mut cmd = String::new();
    let mut args = Vec::new();

    let parsed = (|| -> CliResult<Option<u8>> {
        let mut i = 0;
        while i < argv.len() {
            let a = argv[i].clone();
            let mut value = || -> CliResult<String> {
                i += 1;
                argv.get(i).cloned().ok_or_else(|| Fail::Usage(format!("{a} needs a value")))
            };
            match a.as_str() {
                "--json" => JSON.store(true, Ordering::Relaxed),
                "--quiet" => QUIET.store(true, Ordering::Relaxed),
                "--host" => host = value()?,
                "--port" => port = parse_i64(&value()?).clamp(0, 65535) as u16,
                "--wait" => wait = parse_i64(&value()?).max(0) as u64,
                "--busy-wait" => opt.busy_wait = parse_i64(&value()?).max(0) as u64,
                "--progress-json" => {
                    PROGRESS_JSON.store(true, Ordering::Relaxed);
                    JSON.store(true, Ordering::Relaxed);
                }
                "--contents" => opt.contents = true,
                "--no-wait" => opt.no_wait = true,
                "--version" => {
                    out(&format!("baguette-cli {VERSION}\n"));
                    return Ok(Some(EXIT_OK));
                }
                "-h" | "--help" | "help" => {
                    out(USAGE);
                    return Ok(Some(EXIT_OK));
                }
                _ if cmd.is_empty() => cmd = a,
                _ => args.push(a),
            }
            i += 1;
        }
        if cmd.is_empty() {
            return Err(Fail::Usage("no command".into()));
        }
        Ok(None)
    })();

    let usage = |msg: &str| -> ExitCode {
        if json() {
            out(&format!("{{\"ok\":false,\"error\":{}}}\n", json_str(msg)));
        } else {
            eprint!("baguette-cli: {msg}\n\n{USAGE}");
        }
        ExitCode::from(EXIT_USAGE)
    };
    match parsed {
        Ok(Some(code)) => return ExitCode::from(code),
        Err(Fail::Usage(m)) => return usage(&m),
        _ => {}
    }

    let saved = config::read_address();
    if host.is_empty() {
        match &saved {
            Some((h, _)) => host = h.clone(),
            None => return usage("no 3DS address saved yet: open Baguette.exe once or pass --host and --port"),
        }
    }
    if port == 0 {
        port = saved.map_or(5000, |s| s.1);
    }

    let mut link = Link { host, port, wait, ftp: Ftp::new(), batch_files: 0, batch_bytes: 0, batch_kind: "up" };
    let fail = |code: u8, msg: &str, extra: &str| -> ExitCode {
        out(&if json() { format!("{{\"ok\":false,\"error\":{}{extra}}}\n", json_str(msg)) } else { format!("error: {msg}\n") });
        ExitCode::from(code)
    };
    let result = run(&mut link, &cmd, &args, &opt);
    link.ftp.close();
    match result {
        Ok(code) => ExitCode::from(code),
        Err(Fail::Unreachable) => fail(
            EXIT_UNREACHABLE,
            &format!("3DS not reachable at {} (is Cement open on the 3DS, Wi-Fi on and the lid open?)", link.address()),
            "",
        ),
        Err(Fail::Busy(what)) => {
            fail(EXIT_BUSY, &format!("3DS still busy after {} s: {what}", opt.busy_wait), ",\"busy\":true")
        }
        Err(Fail::Ftp(msg)) => fail(EXIT_FAILED, first_line(&msg), ""),
        Err(Fail::Usage(msg)) => usage(&msg),
    }
}
