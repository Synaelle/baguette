// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Linux helpers, the counterpart of win.rs: the XDG folders, the trash
//! (freedesktop.org spec), mount points, opening files with their app, file
//! times, and the saved passwords' key.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// what joins local paths
pub const SEP: char = '/';

/// "dir/name"
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// the user's folder, where the PC side starts
pub fn home_dir() -> String {
    std::env::var("HOME").ok().filter(|h| h.starts_with('/')).unwrap_or_else(|| "/".into())
}

/// an XDG base folder ($XDG_CONFIG_HOME...), or its default under the home folder
fn xdg(var: &str, default: &str) -> String {
    std::env::var(var).ok().filter(|v| v.starts_with('/')).unwrap_or_else(|| join(&home_dir(), default))
}

/// where the settings, the password key and the web view's data are kept:
/// ~/.config/baguette (an AppImage can't write next to itself). Not created here.
pub fn data_dir() -> String {
    join(&xdg("XDG_CONFIG_HOME", ".config"), "baguette")
}

/// drag-out downloads and opened files (~/.cache/baguette/temp); emptied at start
pub fn temp_root() -> String {
    join(&xdg("XDG_CACHE_HOME", ".cache"), "baguette/temp")
}

/// a folder of ~/.config/user-dirs.dirs ("DOCUMENTS", "DOWNLOAD"...), in the
/// user's language; the English default when the file doesn't say
pub fn known_folder(name: &str) -> Option<String> {
    let home = home_dir();
    let text = std::fs::read_to_string(join(&xdg("XDG_CONFIG_HOME", ".config"), "user-dirs.dirs")).unwrap_or_default();
    let key = format!("XDG_{name}_DIR=");
    let from_file = text.lines().find_map(|l| l.trim().strip_prefix(&key)).map(|v| {
        let v = v.trim().trim_matches('"');
        match v.strip_prefix("$HOME") {
            Some(rest) => format!("{home}{rest}"),
            None => v.to_string(),
        }
    });
    let default = match name {
        "DESKTOP" => "Desktop",
        "DOCUMENTS" => "Documents",
        "DOWNLOAD" => "Downloads",
        "MUSIC" => "Music",
        "PICTURES" => "Pictures",
        "PUBLICSHARE" => "Public",
        "TEMPLATES" => "Templates",
        "VIDEOS" => "Videos",
        _ => return from_file,
    };
    from_file.or_else(|| Some(join(&home, default))).map(|d| d.trim_end_matches('/').to_string()).filter(|d| !d.is_empty())
}

/// "Replace PC folder" must never empty a drive or a personal/system folder
pub fn unsafe_replace_target(dir: &str) -> bool {
    if dir.trim().is_empty() {
        return true;
    }
    let Ok(full) = std::path::absolute(dir) else { return true };
    let f = full.to_string_lossy().trim_end_matches('/').to_string();
    if f.is_empty() {
        return true; // "/"
    }
    const SYSTEM: [&str; 22] = [
        "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib32", "/lib64", "/media", "/mnt", "/opt", "/proc",
        "/root", "/run", "/run/media", "/sbin", "/srv", "/sys", "/tmp", "/usr", "/var", "/snap",
    ];
    let home = home_dir();
    let personal = ["DESKTOP", "DOCUMENTS", "DOWNLOAD", "MUSIC", "PICTURES", "PUBLICSHARE", "TEMPLATES", "VIDEOS"]
        .iter()
        .filter_map(|n| known_folder(n))
        .chain([".config", ".local", ".local/share", ".cache"].iter().map(|d| join(&home, d)))
        .chain([home.trim_end_matches('/').to_string()]);
    SYSTEM.contains(&f.as_str())
        || personal.into_iter().any(|p| p == f)
        || drives().iter().any(|d| d.trim_end_matches('/') == f)
}

/// deletes a file, or a folder with everything in it
fn remove(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path).is_ok(),
        Ok(_) => std::fs::remove_file(path).is_ok(),
        Err(_) => true,
    }
}

/// deletes everything inside dir (not dir itself); to the trash when recycle
pub fn empty_folder(dir: &str, recycle: bool) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return true };
    let paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    if recycle {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        self::recycle(&paths)
    } else {
        paths.iter().fold(true, |ok, p| remove(p) && ok)
    }
}

/// deletes a file or folder tree for good
pub fn delete_tree(path: &str) {
    remove(Path::new(path));
}

/// the AppImage's own libraries and modules are for Baguette, not for the
/// programs it starts
pub fn without_appimage_env(cmd: &mut std::process::Command) {
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    for v in [
        "LD_LIBRARY_PATH", "LD_PRELOAD", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR", "GIO_MODULE_DIR",
        "GIO_EXTRA_MODULES", "GTK_PATH", "GTK_EXE_PREFIX", "GTK_DATA_PREFIX", "GTK_IM_MODULE_FILE",
        "GSETTINGS_SCHEMA_DIR", "PYTHONHOME", "PYTHONPATH", "PERLLIB", "QT_PLUGIN_PATH", "APPDIR", "APPIMAGE",
        "ARGV0", "OWD",
    ] {
        cmd.env_remove(v);
    }
}

/// opens a file (or a web address) with its app, through xdg-open.
/// Err(text) when there is nothing to open it with.
pub fn launch_file(path: &str) -> Result<(), String> {
    if std::env::var_os("BAGUETTE_NO_LAUNCH").is_some() {
        return Ok(()); // tests: download and watch only
    }
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    without_appimage_env(&mut cmd);
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait()); // no zombie left behind
            Ok(())
        }
        Err(e) => Err(format!("Could not open {path} (xdg-open: {e})")),
    }
}

/// a file's modification time, in nanoseconds since 1970
pub fn file_time(m: &std::fs::Metadata) -> u64 {
    (m.mtime().max(0) as u64).saturating_mul(1_000_000_000).saturating_add(m.mtime_nsec().max(0) as u64)
}

/// last write time (file_time) and size of a local file
pub fn last_write(path: &str) -> Option<(u64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((file_time(&m), m.len()))
}

fn local_tm(secs: i64) -> libc::tm {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

fn tm_text(tm: &libc::tm, date_sep: &str, mid: &str) -> String {
    format!(
        "{:04}{date_sep}{:02}{date_sep}{:02}{mid}{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// now on the local wall clock, in seconds since 1970 (the clock Cement's
/// colour times use: the 3DS keeps local time)
pub fn local_now() -> u64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    (now + local_tm(now).tm_gmtoff as i64).max(0) as u64
}

/// "YYYY-MM-DD hh:mm:ss" in local time, from a file_time
pub fn local_time_text(ft: u64) -> String {
    tm_text(&local_tm((ft / 1_000_000_000) as i64), "-", " ")
}

/// "YYYYMMDDhhmmss" in local time, from a file_time
pub fn local_stamp(ft: u64) -> String {
    local_time_text(ft).chars().filter(char::is_ascii_digit).collect()
}

/// dot files stay hidden, like in file managers' default view
pub fn hidden(name: &str, _m: &std::fs::Metadata) -> bool {
    name.starts_with('.')
}

/// there is no Windows folder to protect here
pub fn windows_dir() -> Option<String> {
    None
}

/// "\040" in /proc/self/mounts is a space
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "/", the home folder, then the mounted disks, USB sticks and network shares
pub fn drives() -> Vec<String> {
    let mut out = vec!["/".to_string(), home_dir()];
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    for line in mounts.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(dir), Some(fs)) = (f.next(), f.next(), f.next()) else { continue };
        let dir = unescape_mount(dir);
        let real = dev.starts_with("/dev/") && !dev.starts_with("/dev/loop") && fs != "squashfs"
            || matches!(fs, "9p" | "drvfs" | "virtiofs" | "nfs" | "nfs4" | "cifs" | "smb3" | "fuseblk" | "fuse.sshfs");
        let system = ["/proc", "/sys", "/dev", "/run", "/boot", "/snap", "/var", "/usr", "/etc", "/tmp", "/opt", "/srv"]
            .iter()
            .any(|s| dir == *s || dir.starts_with(&format!("{s}/")))
            && !dir.starts_with("/run/media/");
        if real && !system && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// the folder at the top of the disk holding `path` (its mount point)
fn top_dir(path: &Path, dev: u64) -> PathBuf {
    let mut top = path.to_path_buf();
    while let Some(parent) = top.parent() {
        if std::fs::metadata(parent).map_or(true, |m| m.dev() != dev) {
            break;
        }
        top = parent.to_path_buf();
    }
    top
}

fn mkdir_private(p: &Path) -> bool {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p).is_ok()
}

/// "%"-escapes a path for a .trashinfo file or a file:// URI
fn trash_escape(p: &str) -> String {
    let mut out = String::new();
    for &c in p.as_bytes() {
        if c.is_ascii_alphanumeric() || b"/-_.~".contains(&c) {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// "file:///home/me/a%20b.txt": a local path as a URI, for drag and drop (text/uri-list)
pub fn file_uri(path: &str) -> String {
    format!("file://{}", trash_escape(path))
}

/// moves one file or folder to the trash: the home trash when it is on the same
/// disk, else <disk>/.Trash-<uid> (freedesktop.org trash spec)
fn trash(path: &str) -> bool {
    let Ok(full) = std::path::absolute(path) else { return false };
    let Ok(m) = std::fs::symlink_metadata(&full) else { return true }; // already gone
    let home_trash = PathBuf::from(xdg("XDG_DATA_HOME", ".local/share")).join("Trash");
    let mut probe = home_trash.clone();
    while !probe.exists() && probe.pop() {}
    let same_disk = std::fs::metadata(&probe).is_ok_and(|h| h.dev() == m.dev());
    let trash_dir = if same_disk {
        home_trash
    } else {
        top_dir(&full, m.dev()).join(format!(".Trash-{}", unsafe { libc::getuid() }))
    };
    let (files, info) = (trash_dir.join("files"), trash_dir.join("info"));
    if !mkdir_private(&files) || !mkdir_private(&info) {
        return false;
    }
    let name = full.file_name().map_or("item".into(), |n| n.to_string_lossy().into_owned());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let record = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        trash_escape(&full.to_string_lossy()),
        tm_text(&local_tm(now), "-", "T")
    );
    for n in 1..10000 {
        let taken = if n == 1 { name.clone() } else { format!("{name}.{n}") };
        // the .trashinfo file reserves the name (created only if it doesn't exist)
        let info_file = info.join(format!("{taken}.trashinfo"));
        let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&info_file) else {
            continue;
        };
        if files.join(&taken).symlink_metadata().is_ok() || f.write_all(record.as_bytes()).is_err() {
            let _ = std::fs::remove_file(&info_file);
            continue;
        }
        if std::fs::rename(&full, files.join(&taken)).is_ok() {
            return true;
        }
        let _ = std::fs::remove_file(&info_file);
        return false;
    }
    false
}

/// to the trash
pub fn recycle(paths: &[String]) -> bool {
    paths.iter().fold(true, |ok, p| trash(p) && ok)
}

// The saved passwords: encrypted (ChaCha20-Poly1305) with a random key kept next
// to baguette.ini (secret.key, readable by this user only). Like Windows' DPAPI,
// the ini file copied without the key still opens, and its sites ask for their
// password again.

/// the folder of the settings file, where the key goes
fn key_dir() -> String {
    let ini = crate::config::config_path();
    Path::new(&ini).parent().map_or_else(data_dir, |d| d.to_string_lossy().into_owned())
}

fn key(dir: &str, create: bool) -> Option<ring::aead::LessSafeKey> {
    use ring::aead::{CHACHA20_POLY1305, LessSafeKey, UnboundKey};
    let file = join(dir, "secret.key");
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(_) if create => {
            let mut k = [0u8; 32];
            ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut k).ok()?;
            mkdir_private(Path::new(dir));
            match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&file) {
                Ok(mut f) => {
                    f.write_all(&k).ok()?;
                    k.to_vec()
                }
                Err(_) => std::fs::read(&file).ok()?, // another Baguette made it first
            }
        }
        Err(_) => return None,
    };
    Some(LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &bytes).ok()?))
}

/// encrypts with this user's key, as hex for the ini file; None when it can't
/// (no key could be made)
pub fn protect(text: &str) -> Option<String> {
    protect_in(&key_dir(), text)
}

fn protect_in(dir: &str, text: &str) -> Option<String> {
    use ring::aead::{Aad, NONCE_LEN, Nonce};
    if text.is_empty() {
        return Some(String::new());
    }
    let k = key(dir, true)?;
    let mut nonce = [0u8; NONCE_LEN];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce).ok()?;
    let mut data = text.as_bytes().to_vec();
    k.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut data).ok()?;
    Some(nonce.iter().chain(&data).map(|b| format!("{b:02x}")).collect())
}

/// None when it was encrypted with another key (another PC or user)
pub fn unprotect(hex: &str) -> Option<String> {
    unprotect_in(&key_dir(), hex)
}

fn unprotect_in(dir: &str, hex: &str) -> Option<String> {
    use ring::aead::{Aad, NONCE_LEN, Nonce};
    if hex.is_empty() {
        return Some(String::new());
    }
    let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    if bytes.len() < NONCE_LEN {
        return None;
    }
    let (nonce, rest) = bytes.split_at(NONCE_LEN);
    let mut data = rest.to_vec();
    let k = key(dir, false)?;
    let plain = k.open_in_place(Nonce::try_assume_unique_for_key(nonce).ok()?, Aad::empty(), &mut data).ok()?;
    Some(String::from_utf8_lossy(plain).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_and_paths() {
        assert_eq!(unescape_mount("/media/me/My\\040Disk"), "/media/me/My Disk");
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/a", "b"), "/a/b");
        assert!(drives().first().is_some_and(|d| d == "/"));
        assert_eq!(trash_escape("/home/me/été 1.txt"), "/home/me/%C3%A9t%C3%A9%201.txt");
        assert_eq!(file_uri("/tmp/x/100% #1?.bin"), "file:///tmp/x/100%25%20%231%3F.bin");
        let home = home_dir();
        assert!(unsafe_replace_target("/") && unsafe_replace_target("/usr/") && unsafe_replace_target(&home));
        assert!(unsafe_replace_target(&format!("{home}/")));
        assert!(!unsafe_replace_target(&format!("{home}/3DS backup")));
        assert_eq!(local_time_text(0).len(), 19);
    }

    #[test]
    fn trash_spec() {
        let base = std::env::temp_dir().join(format!("baguette-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("data/dir/deep")).unwrap();
        std::fs::write(base.join("data/a b.txt"), "x").unwrap();
        std::fs::write(base.join("data/dir/deep/f"), "y").unwrap();
        // a home of its own: the test's trash stays out of the user's
        unsafe { std::env::set_var("XDG_DATA_HOME", base.join("share")) };
        let items = [base.join("data/a b.txt"), base.join("data/dir")].map(|p| p.to_string_lossy().into_owned());
        assert!(recycle(&items));
        assert!(!base.join("data/a b.txt").exists() && !base.join("data/dir").exists());
        let trash = base.join("share/Trash");
        // /tmp may be another disk than the test's share folder: then the top-dir trash is used
        let trash = if trash.join("files/a b.txt").exists() { trash } else { top_dir(&base, std::fs::metadata(&base).unwrap().dev()).join(format!(".Trash-{}", unsafe { libc::getuid() })) };
        assert!(trash.join("files/dir/deep/f").exists());
        let info = std::fs::read_to_string(trash.join("info/a b.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/") && info.contains("a%20b.txt\nDeletionDate=2"), "{info}");
        std::fs::write(base.join("data/a b.txt"), "again").unwrap();
        assert!(recycle(&items[..1]));
        assert!(trash.join("files/a b.txt.2").exists(), "same name: numbered");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn key_round_trip() {
        let base = std::env::temp_dir().join(format!("baguette-key-{}", std::process::id()));
        let (dir, other_dir) = (base.join("a").to_string_lossy().into_owned(), base.join("b").to_string_lossy().into_owned());
        assert_eq!(unprotect_in(&dir, "00112233445566778899aabbccddeeff"), None, "no key yet");
        let h = protect_in(&dir, "hunter2 é").unwrap();
        assert!(!h.is_empty() && !h.contains("hunter2"));
        assert_eq!(std::fs::metadata(join(&dir, "secret.key")).unwrap().mode() & 0o777, 0o600);
        assert_eq!(unprotect_in(&dir, &h).as_deref(), Some("hunter2 é"));
        assert_ne!(protect_in(&dir, "hunter2 é").unwrap(), h, "a new nonce each time");
        assert_eq!(unprotect_in(&dir, "00ff"), None);
        let mut tampered = h.clone();
        tampered.replace_range(30..32, if &h[30..32] == "00" { "01" } else { "00" });
        assert_eq!(unprotect_in(&dir, &tampered), None, "tampered");
        protect_in(&other_dir, "x");
        assert_eq!(unprotect_in(&other_dir, &h), None, "another key (other PC or user)");
        let _ = std::fs::remove_dir_all(&base);
    }
}
