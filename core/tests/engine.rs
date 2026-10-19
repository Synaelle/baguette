// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Headless test of Chomp's workers against Cement's PC test server
//! (cargo build --release in the cement repo). The server is found through
//! CEMENT_SERVER, or next to this repo at ../cement/target/release/cement-server.exe;
//! without it the test is skipped.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chomp_core::ftp::Entry;
use chomp_core::workers::{CONNECTED, Client, Event, Job, Kind, ListResult, Sink, WAITING};

#[derive(Default)]
struct Events(Mutex<Vec<ListResult>>);

struct Collect(Arc<Events>);

impl Sink for Collect {
    fn emit(&self, e: Event) {
        if let Event::List(l) = e {
            self.0.0.lock().unwrap().push(l);
        }
    }
}

fn wait_for(mut cond: impl FnMut() -> bool, ms: u64) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cond()
}

fn write_random(p: &Path, size: usize, seed: u32) {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    let data: Vec<u8> = (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    std::fs::write(p, data).unwrap();
}

fn same(a: &Path, b: &Path) -> bool {
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

struct Checks(usize);

impl Checks {
    fn check(&mut self, ok: bool, what: &str) {
        println!("{} {what}", if ok { "  ok  " } else { "  FAIL" });
        if !ok {
            self.0 += 1;
        }
    }
}

fn server_exe() -> Option<PathBuf> {
    let p = std::env::var_os("CEMENT_SERVER").map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cement/target/release/cement-server.exe")
    });
    p.exists().then_some(p)
}

/// Chomp's tests run their Cement on 6000 (data 6001-6999): Cement's own tests
/// keep 5000, so both can run at the same time
const DEV_PORT: u16 = 6000;

fn start_server(exe: &Path, root: &Path, flag: &Path) -> Child {
    Command::new(exe)
        .env("CEMENT_ROOT", root)
        .env("CEMENT_PORT", DEV_PORT.to_string())
        .env("CEMENT_SLEEPFLAG", flag)
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("cement-server")
}

fn job(kind: Kind, path: &str, path2: &str) -> Job {
    let mut j = Job::new(kind);
    j.path = path.into();
    j.path2 = path2.into();
    j
}

#[test]
fn workers_against_cement() {
    let Some(exe) = server_exe() else {
        println!("skipped: build cement-server (cargo build --release in the cement repo) or set CEMENT_SERVER");
        return;
    };
    let base = std::env::temp_dir().join(format!("chomp-engine-test-{}", std::process::id()));
    let (root, local, back, flag) = (base.join("sd"), base.join("local"), base.join("back"), base.join("sleep.flag"));
    for d in [&root, &local, &back] {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut c = Checks(0);
    let events = Arc::new(Events::default());
    let lists = |e: &Arc<Events>| e.0.lock().unwrap().clone();

    println!("waits for the server, then connects");
    let mut client = Client::start("127.0.0.1", DEV_PORT, Box::new(Collect(events.clone())));
    let s = client.shared.clone();
    std::thread::sleep(Duration::from_millis(2500));
    c.check(s.browse.state.load(Ordering::Relaxed) == WAITING, "waiting while the server is down");
    let mut server = start_server(&exe, &root, &flag);
    c.check(wait_for(|| s.browse.connected() && s.xfer.connected(), 8000), "both connections up once the server starts");
    c.check(
        wait_for(|| lists(&events).last().is_some_and(|l| l.path == "/" && l.ok), 3000),
        "folder refreshed automatically after connecting",
    );

    println!("upload a folder tree");
    std::fs::create_dir_all(local.join("tree/sub dir")).unwrap();
    std::fs::create_dir_all(local.join("tree/empty")).unwrap();
    write_random(&local.join("tree/a.bin"), 3 << 20, 1);
    write_random(&local.join("tree/sub dir/b été.bin"), 777777, 2);
    write_random(&local.join("tree/zero.bin"), 0, 3);
    write_random(&local.join("single.bin"), 1 << 20, 4);
    let upload = |files: Vec<PathBuf>, dir: &str| {
        let mut j = job(Kind::Upload, dir, "");
        j.locals = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        s.start_progress();
        s.xfer.push(j)
    };
    let h = upload(vec![local.join("tree"), local.join("single.bin")], "/");
    c.check(h.wait_timeout(Duration::from_secs(20)) == Some(true), "upload job finished");
    c.check(same(&root.join("tree/a.bin"), &local.join("tree/a.bin")), "tree/a.bin intact");
    c.check(same(&root.join("tree/sub dir/b été.bin"), &local.join("tree/sub dir/b été.bin")), "nested unicode file intact");
    c.check(root.join("tree/empty").is_dir(), "empty folder created");
    c.check(root.join("tree/zero.bin").exists(), "empty file created");
    c.check(same(&root.join("single.bin"), &local.join("single.bin")), "single file intact");
    c.check(wait_for(|| lists(&events).last().is_some_and(|l| l.entries.len() >= 2), 3000), "listing refreshed after upload");

    let done = || s.progress.lock().unwrap().done;
    println!("lid closes mid-upload, transfer resumes after wake");
    write_random(&local.join("big.bin"), 400 << 20, 5);
    let h = upload(vec![local.join("big.bin")], "/");
    wait_for(|| done() > 60 << 20, 20000);
    std::fs::write(&flag, b"").unwrap();
    c.check(wait_for(|| s.xfer.state.load(Ordering::Relaxed) == WAITING, 15000), "client notices the 3DS went to sleep");
    c.check(!h.is_finished(), "job kept while waiting");
    std::thread::sleep(Duration::from_millis(2500));
    std::fs::remove_file(&flag).unwrap();
    c.check(h.wait_timeout(Duration::from_secs(60)) == Some(true), "upload resumed and finished after wake");
    c.check(same(&root.join("big.bin"), &local.join("big.bin")), "resumed 400 MB file intact");

    println!("force reconnect mid-upload");
    std::fs::remove_file(root.join("big.bin")).unwrap();
    let h = upload(vec![local.join("big.bin")], "/");
    wait_for(|| done() > 100 << 20, 20000);
    s.xfer.reconnect();
    s.browse.reconnect();
    c.check(h.wait_timeout(Duration::from_secs(60)) == Some(true), "upload finished after forced reconnect");
    c.check(same(&root.join("big.bin"), &local.join("big.bin")), "file intact after forced reconnect");

    println!("cancel removes the partial remote file");
    std::fs::remove_file(root.join("big.bin")).unwrap();
    let h = upload(vec![local.join("big.bin")], "/");
    wait_for(|| done() > 50 << 20, 20000);
    s.cancel.store(true, Ordering::Relaxed);
    s.xfer.clear_pending();
    c.check(h.wait_timeout(Duration::from_secs(15)) == Some(false), "upload cancelled");
    c.check(wait_for(|| !root.join("big.bin").exists(), 5000), "partial file deleted on the 3DS");
    c.check(wait_for(|| s.xfer.state.load(Ordering::Relaxed) == CONNECTED, 5000), "transfer connection still usable");

    println!("download a folder (drag-out path)");
    *s.cwd.lock().unwrap() = "/".into();
    let mut j = job(Kind::Download, "/", &back.to_string_lossy());
    j.remotes = vec![Entry { name: "tree".into(), dir: true, ..Default::default() }];
    s.start_progress();
    let h = s.xfer.push(j);
    c.check(h.wait_timeout(Duration::from_secs(20)).is_some(), "download job finished");
    c.check(same(&back.join("tree/sub dir/b été.bin"), &local.join("tree/sub dir/b été.bin")), "recursive download intact");
    c.check(back.join("tree/empty").is_dir(), "empty folder downloaded");

    println!("send to PC (paths from settings, each item directly in the PC folder)");
    let dest = back.join("sendto");
    std::fs::create_dir_all(root.join("3ds/Checkpoint/saves/Game é")).unwrap();
    write_random(&root.join("3ds/Checkpoint/saves/Game é/main"), 123456, 7);
    write_random(&root.join("boot.firm"), 5000, 8);
    std::fs::create_dir_all(&dest).unwrap();
    let mut j = job(Kind::SendToPc, "", &dest.to_string_lossy());
    j.send_paths = vec!["/3ds/Checkpoint/saves".into(), "/boot.firm".into(), "/does/not/exist".into()];
    s.start_progress();
    let h = s.xfer.push(j);
    c.check(h.wait_timeout(Duration::from_secs(20)) == Some(true), "send to PC job finished");
    c.check(same(&dest.join("saves/Game é/main"), &root.join("3ds/Checkpoint/saves/Game é/main")), "folder copied directly into the PC folder");
    c.check(!dest.join("3ds").exists(), "no parent folders created");
    c.check(same(&dest.join("boot.firm"), &root.join("boot.firm")), "single file copied");
    c.check(h.job().missing == 1, "missing path skipped and counted");

    println!("send to PC with \"replace PC folder contents\"");
    write_random(&dest.join("old file.txt"), 10, 9);
    std::fs::create_dir_all(dest.join("old dir/deep")).unwrap();
    let mut j = job(Kind::SendToPc, "", &dest.to_string_lossy());
    j.send_paths = vec!["/boot.firm".into()];
    j.replace = true;
    j.recycle = false; // keep the test out of the Recycle Bin
    s.start_progress();
    let h = s.xfer.push(j);
    c.check(h.wait_timeout(Duration::from_secs(20)) == Some(true), "replace job finished");
    c.check(
        !dest.join("old file.txt").exists() && !dest.join("old dir").exists() && !dest.join("saves").exists(),
        "old PC folder contents removed",
    );
    c.check(same(&dest.join("boot.firm"), &root.join("boot.firm")), "3DS content copied after emptying");
    c.check(dest.is_dir(), "the PC folder itself is kept");
    let docs = chomp_core::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Documents).unwrap_or_default();
    c.check(
        chomp_core::win::unsafe_replace_target("C:\\")
            && chomp_core::win::unsafe_replace_target(&docs)
            && chomp_core::win::unsafe_replace_target(&format!("{docs}\\"))
            && !chomp_core::win::unsafe_replace_target(&format!("{docs}\\3DS backup")),
        "drives and personal folders refused, subfolders allowed",
    );
    let mut j = job(Kind::SendToPc, "", "C:\\");
    j.send_paths = vec!["/boot.firm".into()];
    j.replace = true;
    let h = s.xfer.push(j);
    c.check(h.wait_timeout(Duration::from_secs(10)) == Some(false), "worker refuses to empty a drive root");

    println!("explorer operations");
    s.browse.push(job(Kind::Mkdir, "/newdir", ""));
    c.check(wait_for(|| root.join("newdir").is_dir(), 5000), "new folder");
    s.browse.push(job(Kind::Rename, "/single.bin", "/newdir/moved.bin"));
    c.check(wait_for(|| root.join("newdir/moved.bin").exists(), 5000), "rename/move");
    let mut del = job(Kind::Delete, "/", "");
    del.remotes = ["tree", "newdir", "3ds"]
        .into_iter()
        .map(|n| Entry { name: n.into(), dir: true, ..Default::default() })
        .chain([Entry { name: "boot.firm".into(), dir: false, ..Default::default() }])
        .collect();
    s.browse.push(del);
    c.check(wait_for(|| !root.join("tree").exists() && !root.join("newdir").exists(), 5000), "delete folders recursively");
    events.0.lock().unwrap().clear();
    s.browse.push(job(Kind::List, "/", ""));
    c.check(
        wait_for(|| lists(&events).last().is_some_and(|l| l.entries.is_empty()), 5000),
        "root listing empty after delete",
    );

    println!("tracked deletion of many files, surviving a forced reconnect");
    for d in 0..8 {
        let dir = root.join(format!("many/d{d}/sub"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..60 {
            write_random(&dir.join(format!("f{f}.bin")), 100, f);
        }
    }
    let mut j = job(Kind::Delete, "/", "");
    j.remotes = vec![Entry { name: "many".into(), dir: true, ..Default::default() }];
    s.start_progress();
    let h = s.xfer.push(j);
    let mut saw_progress = false;
    wait_for(
        || {
            let p = s.progress.lock().unwrap();
            saw_progress |= p.del && p.file_count == 8 * 60 + 8 * 2 + 1;
            p.del && p.done > 120
        },
        20000,
    );
    s.xfer.reconnect();
    c.check(saw_progress, "deletion reports progress per item (497 items)");
    c.check(h.wait_timeout(Duration::from_secs(60)) == Some(true), "deletion finished without errors after a reconnect");
    c.check(!root.join("many").exists(), "every file and folder deleted");

    client.stop();
    let _ = server.kill();
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&base);
    println!("\n{} failure(s)", c.0);
    assert_eq!(c.0, 0, "engine test failures");
}
