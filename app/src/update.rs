// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Is there a newer Baguette? One request to GitHub's latest-release API when
//! the window opens (unless turned off). On a click, the release's archive for
//! this system is downloaded, its files replace this Baguette's and it starts
//! again. Cement's own latest release is looked up the same way, for an older
//! Cement to be offered.

use std::cmp::Ordering;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use baguette_core::os;
use serde::{Deserialize, Serialize};

const LATEST: &str = "https://api.github.com/repos/Synaelle/baguette/releases/latest";
pub const RELEASES: &str = "https://github.com/Synaelle/baguette/releases";
const CEMENT_LATEST: &str = "https://api.github.com/repos/Synaelle/cement/releases/latest";
const CEMENT_RELEASES: &str = "https://github.com/Synaelle/cement/releases";

#[derive(Serialize, Clone)]
pub struct Update {
    pub version: String, // "2.0"
    pub url: String,     // its release page
    pub install: bool,   // it has this system's archive: Baguette can update itself
}

#[derive(Deserialize, Clone)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// the latest releases of Baguette and of Cement, once the check found them
static LATEST_RELEASE: Mutex<Option<Release>> = Mutex::new(None);
static LATEST_CEMENT: Mutex<Option<Release>> = Mutex::new(None);

/// what Baguette's files are called in a release's archive, and in its folder
#[cfg(windows)]
const FILES: [&str; 3] = ["Baguette.exe", "baguette-cli.exe", "add-to-path.bat"];
#[cfg(not(windows))]
const FILES: [&str; 2] = ["Baguette-x86_64.AppImage", "baguette-cli"];

/// "Baguette-1.2.0-windows.zip": this system's archive in the release
fn archive_name(version: &str) -> String {
    if cfg!(windows) { format!("Baguette-{version}-windows.zip") } else { format!("Baguette-{version}-linux.tar.gz") }
}

fn version_of(tag: &str) -> String {
    tag.trim_start_matches(['v', 'V']).to_string()
}

fn agent(current: &str, timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(format!("Baguette/{current}"))
        .build()
        .new_agent()
}

/// this build's version without its dev mark: "2.0.1-abc1234-dev" -> "2.0.1"
fn local_base(v: &str) -> &str {
    match v.strip_suffix("-dev") {
        Some(stripped) => stripped.rsplit_once('-').map_or(stripped, |(head, _hash)| head),
        None => v,
    }
}

/// "2.0", "2.0.1", "2.0-RC3": (major, minor, patch, release candidate or None
/// for a final release)
fn parse(v: &str) -> Option<(u32, u32, u32, Option<u32>)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let (main, pre) = match v.split_once('-') {
        Some((m, p)) => (m, Some(p)),
        None => (v, None),
    };
    let nums: Vec<u32> = main.split('.').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    let rc = match pre {
        None => None,
        Some(p) if p.len() > 2 && p[..2].eq_ignore_ascii_case("rc") => Some(p[2..].parse().ok()?),
        Some(_) => return None,
    };
    let get = |i: usize| nums.get(i).copied().unwrap_or(0);
    if nums.is_empty() || nums.len() > 3 {
        return None;
    }
    Some((get(0), get(1), get(2), rc))
}

/// is this build (`local`, with its build number) older than the release `tag`?
fn older(local: &str, tag: &str) -> bool {
    let (Some(a), Some(b)) = (parse(local_base(local)), parse(tag)) else { return false };
    match (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)) {
        Ordering::Less => true,
        Ordering::Greater => false,
        // same version: a release candidate comes before the final release
        Ordering::Equal => match (a.3, b.3) {
            (Some(x), Some(y)) => x < y,
            (Some(_), None) => true,
            _ => false,
        },
    }
}

/// the latest published release at the API address `url`
fn latest_release(current: &str, url: &str) -> Option<Release> {
    let mut resp = agent(current, 8).get(url).header("Accept", "application/vnd.github+json").call().ok()?;
    let r: Release = resp.body_mut().read_json().ok()?;
    (!r.draft).then_some(r)
}

/// Some(update) when GitHub has a newer release than `current`; None when
/// this one is up to date, newer, or GitHub can't be reached
pub fn check(current: &str) -> Option<Update> {
    if current == "dev" {
        return None;
    }
    // tests answer from a local server
    let cement = std::env::var("BAGUETTE_CEMENT_UPDATE_URL").unwrap_or_else(|_| CEMENT_LATEST.into());
    *LATEST_CEMENT.lock().unwrap() = latest_release(current, &cement);
    let latest = std::env::var("BAGUETTE_UPDATE_URL").unwrap_or_else(|_| LATEST.into());
    let r = latest_release(current, &latest)?;
    *LATEST_RELEASE.lock().unwrap() = Some(r.clone());
    if !older(current, &r.tag_name) {
        return None;
    }
    let version = version_of(&r.tag_name);
    let install = has(&r, &archive_name(&version)) && signed(&r) && self_path().is_some();
    let url = if r.html_url.starts_with(RELEASES) { r.html_url } else { RELEASES.into() };
    Some(Update { version, url, install })
}

/// the newer version an older Cement can update to, from Cement's release the
/// check found (when it has a Cement.cia)
pub fn cement_update(cement_version: &str) -> Option<String> {
    let r = LATEST_CEMENT.lock().unwrap().clone()?;
    if cement_version.is_empty() || !older(cement_version, &r.tag_name) {
        return None;
    }
    (has(&r, "Cement.cia") && signed(&r)).then(|| version_of(&r.tag_name))
}

fn has(r: &Release, name: &str) -> bool {
    r.assets.iter().any(|a| a.name == name)
}

/// only a signed release can be installed from Baguette (see `check_signed`)
fn signed(r: &Release) -> bool {
    has(r, SUMS) && has(r, SUMS_SIG)
}

/// downloads `url` to the file `to`, refusing anything but the expected size
fn fetch(current: &str, url: &str, size: u64, to: &Path) -> Result<(), String> {
    let mut resp = agent(current, 600).get(url).call().map_err(|e| format!("Could not download the update: {e}"))?;
    let mut body = resp.body_mut().with_config().limit(512 << 20).reader();
    let mut file = std::fs::File::create(to).map_err(|e| format!("Could not save the update: {e}"))?;
    let got = std::io::copy(&mut body, &mut file).map_err(|e| format!("Could not download the update: {e}"))?;
    if size != 0 && got != size {
        return Err("The download was cut short: try again".into());
    }
    Ok(())
}

/// the release file `name` (from Baguette's or Cement's latest release),
/// downloaded into Baguette's temp folder
fn download_asset(current: &str, name: &str, cement: bool) -> Result<(PathBuf, String), String> {
    let (latest, releases, test) = if cement {
        (&LATEST_CEMENT, CEMENT_RELEASES, "BAGUETTE_CEMENT_UPDATE_URL")
    } else {
        (&LATEST_RELEASE, RELEASES, "BAGUETTE_UPDATE_URL")
    };
    let r = latest.lock().unwrap().clone().ok_or("Look for updates first")?;
    let asset = |n: &str| -> Result<Asset, String> {
        let a = r.assets.iter().find(|a| a.name == n).ok_or_else(|| format!("This release has no {n}"))?;
        if !a.browser_download_url.starts_with(releases) && std::env::var_os(test).is_none() {
            return Err("Not a release file from GitHub".into());
        }
        Ok(a.clone())
    };
    let file = asset(name)?;
    let unsigned = |_| "This release isn't signed, so Baguette won't install it: get it from GitHub yourself".to_string();
    let (sums, sig) = (asset(SUMS).map_err(unsigned)?, asset(SUMS_SIG).map_err(unsigned)?);
    let dir = PathBuf::from(os::temp_root()).join("update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not save the update: {e}"))?;
    let to = dir.join(name);
    fetch(current, &file.browser_download_url, file.size, &to)?;
    let (sums_at, sig_at) = (dir.join(SUMS), dir.join(SUMS_SIG));
    fetch(current, &sums.browser_download_url, sums.size, &sums_at)?;
    fetch(current, &sig.browser_download_url, sig.size, &sig_at)?;
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("Could not read the update: {e}"));
    check_signed(&RELEASE_KEY, &read(&sums_at)?, &read(&sig_at)?, name, &sha256_of(&to)?)?;
    Ok((to, version_of(&r.tag_name)))
}

/// what each release carries besides its files: their SHA-256 sums, and the
/// release key's Ed25519 signature of that list (made by the release workflow)
const SUMS: &str = "SHA256SUMS";
const SUMS_SIG: &str = "SHA256SUMS.sig";

/// the public half of the release key; its private half is a secret of the
/// GitHub repositories, used only by the release workflows of Baguette and Cement
const RELEASE_KEY: [u8; 32] = [
    0x18, 0xd7, 0x87, 0x17, 0x70, 0x65, 0xe9, 0x63, 0x04, 0x29, 0x2b, 0x13, 0xb2, 0xee, 0x55, 0xdc,
    0xc8, 0x40, 0x91, 0x27, 0x31, 0xf0, 0xca, 0xad, 0x59, 0xbb, 0xb6, 0x0c, 0xe3, 0x32, 0x0b, 0x67,
];

/// Ok when `sig` is `key`'s signature of `sums` and `sums` lists `name` with
/// this SHA-256 (hex)
fn check_signed(key: &[u8], sums: &[u8], sig: &[u8], name: &str, sha256: &str) -> Result<(), String> {
    use ring::signature::{ED25519, UnparsedPublicKey};
    UnparsedPublicKey::new(&ED25519, key)
        .verify(sums, sig)
        .map_err(|_| "The release's signature is wrong, so Baguette won't install it".to_string())?;
    // sha256sum's lines: "<hex>  <name>", or "<hex> *<name>" in binary mode
    let listed = String::from_utf8_lossy(sums)
        .lines()
        .find_map(|l| l.split_once(' ').filter(|(_, n)| n.get(1..) == Some(name)).map(|(h, _)| h.to_string()));
    match listed {
        Some(h) if h.eq_ignore_ascii_case(sha256) => Ok(()),
        Some(_) => Err(format!("The downloaded {name} isn't the one the release signed, so Baguette won't install it")),
        None => Err(format!("The release's signed list doesn't have {name}")),
    }
}

/// a file's SHA-256, as lowercase hex
fn sha256_of(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path).map_err(|e| format!("Could not read the update: {e}"))?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match f.read(&mut buf).map_err(|e| format!("Could not read the update: {e}"))? {
            0 => break,
            n => h.update(&buf[..n]),
        }
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// the newest Cement.cia, downloaded for a 3DS: its path on the PC and its version
pub fn download_cement(current: &str) -> Result<(String, String), String> {
    let (path, version) = download_asset(current, "Cement.cia", true)?;
    Ok((path.to_string_lossy().into_owned(), version))
}

/// the file this Baguette runs from: the exe, or the AppImage (a build run any
/// other way can't replace itself)
fn self_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::current_exe().ok()
    } else {
        std::env::var_os("APPIMAGE").map(PathBuf::from)
    }
}

/// reads Baguette's files out of the downloaded archive (they sit in a
/// Baguette folder): (name, contents), only the known names
fn unpack(archive: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let bad = |e: String| format!("The update is damaged ({e})");
    let mut out = Vec::new();
    let mut keep = |path: &str, data: Vec<u8>| {
        let name = path.strip_prefix("Baguette/").unwrap_or(path);
        if FILES.contains(&name) {
            out.push((name.to_string(), data));
        }
    };
    let file = std::fs::File::open(archive).map_err(|e| bad(e.to_string()))?;
    #[cfg(windows)]
    {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| bad(e.to_string()))?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
            if f.is_file() {
                let name = f.name().map_err(|e| bad(e.to_string()))?.into_owned();
                let mut data = Vec::new();
                f.read_to_end(&mut data).map_err(|e| bad(e.to_string()))?;
                keep(&name, data);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for f in tar.entries().map_err(|e| bad(e.to_string()))? {
            let mut f = f.map_err(|e| bad(e.to_string()))?;
            if f.header().entry_type().is_file() {
                let path = f.path().map_err(|e| bad(e.to_string()))?.to_string_lossy().into_owned();
                let mut data = Vec::new();
                f.read_to_end(&mut data).map_err(|e| bad(e.to_string()))?;
                keep(&path, data);
            }
        }
    }
    if !out.iter().any(|(n, _)| n == FILES[0]) {
        return Err(bad(format!("no {}", FILES[0])));
    }
    Ok(out)
}

/// puts `data` in place of `dest`: written next to it first, then renamed over
/// it. On Windows the running exe can't be replaced, only renamed, so the old
/// file becomes <name>.old (deleted at the next start).
fn replace(dest: &Path, data: &[u8]) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Could not replace {}: {e}", dest.display());
    let new = dest.with_file_name(format!("{}.new", dest.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::write(&new, data).map_err(fail)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    }
    if cfg!(windows) && dest.exists() {
        let old = dest.with_file_name(format!("{}.old", dest.file_name().unwrap_or_default().to_string_lossy()));
        let _ = std::fs::remove_file(&old);
        move_file(dest, &old).map_err(fail)?;
        if let Err(e) = move_file(&new, dest) {
            // never leave Baguette without its exe: the old one goes back
            let _ = move_file(&old, dest);
            let _ = std::fs::remove_file(&new);
            return Err(fail(e));
        }
        return Ok(());
    }
    move_file(&new, dest).map_err(fail)
}

/// a rename that also works on the running exe: std's rename asks for delete
/// access, which Windows refuses for a running program, MoveFileEx doesn't
#[cfg(windows)]
#[allow(unsafe_code)]
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MoveFileExW};
    let (a, b) = (os::wide(&from.to_string_lossy()), os::wide(&to.to_string_lossy()));
    // a virus scanner looking at a file just written holds it for a moment
    let mut tries = 0;
    loop {
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_REPLACE_EXISTING) } != 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        tries += 1;
        if e.raw_os_error() != Some(32) || tries == 40 {
            return Err(e);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(not(windows))]
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// downloads the latest release's archive and replaces this Baguette's files
/// with it. Returns the file to start: the new Baguette.
pub fn install(current: &str) -> Result<PathBuf, String> {
    let me = self_path().ok_or("Only the AppImage can update itself: download the new one from GitHub")?;
    let dir = me.parent().ok_or("Baguette's folder is unknown")?.to_path_buf();
    let version = LATEST_RELEASE.lock().unwrap().as_ref().map(|r| version_of(&r.tag_name)).ok_or("Look for updates first")?;
    let (archive, _) = download_asset(current, &archive_name(&version), false)?;
    let files = unpack(&archive)?;
    let _ = std::fs::remove_dir_all(archive.parent().unwrap_or(&archive));
    for (name, data) in &files {
        // the app is the file running now, whatever it was renamed to; the
        // others go next to it, and on Linux only when already there
        let dest = if name == FILES[0] { me.clone() } else { dir.join(name) };
        if name != FILES[0] && !cfg!(windows) && !dest.exists() {
            continue;
        }
        replace(&dest, data)?;
    }
    Ok(me)
}

/// starts the new Baguette (the old one then quits)
pub fn start(path: &Path) -> Result<(), String> {
    if std::env::var_os("BAGUETTE_NO_LAUNCH").is_some() {
        return Ok(()); // tests: replaced, not started
    }
    let mut cmd = std::process::Command::new(path);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(not(windows))]
    os::without_appimage_env(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| format!("Updated, but the new Baguette didn't start ({e}): open it again"))
}

/// the files an update left behind (<name>.old next to the exe), once the old
/// Baguette has quit
pub fn clean_old() {
    let Some(dir) = self_path().and_then(|p| p.parent().map(Path::to_path_buf)) else { return };
    std::thread::spawn(move || {
        for _ in 0..20 {
            let mut left = false;
            for name in FILES {
                let old = dir.join(format!("{name}.old"));
                if old.exists() && std::fs::remove_file(&old).is_err() {
                    left = true;
                }
            }
            if let Some(me) = std::env::current_exe().ok().filter(|_| cfg!(windows)) {
                let old = me.with_file_name(format!("{}.old", me.file_name().unwrap_or_default().to_string_lossy()));
                if old.exists() && std::fs::remove_file(&old).is_err() {
                    left = true;
                }
            }
            if !left {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(local_base("2.0.1"), "2.0.1");
        assert_eq!(local_base("2.0.1-abc1234-dev"), "2.0.1");
        assert!(older("2.0.1", "v2.0.2"));
        assert!(older("2.0.1", "v2.1"));
        assert!(older("2.0-RC3", "v2.0"), "an RC before the final release");
        assert!(older("2.0-RC2", "v2.0-RC3"));
        assert!(!older("2.0.1", "v2.0.1"));
        assert!(!older("2.0.1-abc1234-dev", "v2.0.1"), "a dev build of the same version");
        assert!(!older("2.1.0", "v2.0.1"));
        assert!(!older("2.0.1", "v2.0"));
        assert!(!older("2.0.1", "nightly"), "unknown tags are ignored");
    }

    /// an archive like a release's: Baguette's files in a Baguette folder, and a stray one
    #[test]
    fn unpacks_the_archive() {
        let dir = std::env::temp_dir().join(format!("baguette-unpack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("release");
        let entries: Vec<(String, Vec<u8>)> = FILES
            .iter()
            .map(|n| (format!("Baguette/{n}"), n.as_bytes().to_vec()))
            .chain([("Baguette/stray.so".to_string(), b"no".to_vec())])
            .collect();
        #[cfg(windows)]
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            for (name, data) in &entries {
                z.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        #[cfg(not(windows))]
        {
            let gz = flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast());
            let mut t = tar::Builder::new(gz);
            for (name, data) in &entries {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o755);
                h.set_cksum();
                t.append_data(&mut h, name, data.as_slice()).unwrap();
            }
            t.into_inner().unwrap().finish().unwrap();
        }
        let mut got = unpack(&archive).unwrap();
        got.sort();
        let mut want: Vec<(String, Vec<u8>)> = FILES.iter().map(|n| (n.to_string(), n.as_bytes().to_vec())).collect();
        want.sort();
        assert_eq!(got, want, "only Baguette's own files, without their folder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unhex(h: &str) -> Vec<u8> {
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }

    /// SHA256SUMS as the release workflow makes it, signed there by openssl with the
    /// release key (`openssl pkeyutl -sign -rawin`)
    const SUMS_TEXT: &str = "c865f6c5ab8d1b0bcd383a5e1e3879d22681c96bf462c269b7581d523fbe70ab  Baguette-1.6.0-windows.zip\n\
        6d7ebc44c5bc26207e62f4f628f912e1a0f41ed11764891aa7dd99eab83228e7  Baguette-1.6.0-linux.tar.gz\n";
    const SUMS_SIG_HEX: &str = "4c2693936a380f98b52e45cb829e528ce50db5cce7ae196a4c0806f9de127510\
        fe32f88da5add3ae48550de22c3f769ae4da85007412beb21481141c6b2e430b";

    #[test]
    fn checks_the_signed_checksums() {
        let (sums, sig) = (SUMS_TEXT.as_bytes(), unhex(SUMS_SIG_HEX));
        let linux = "6d7ebc44c5bc26207e62f4f628f912e1a0f41ed11764891aa7dd99eab83228e7";
        assert_eq!(check_signed(&RELEASE_KEY, sums, &sig, "Baguette-1.6.0-linux.tar.gz", linux), Ok(()));
        let other = "00".repeat(32);
        assert!(check_signed(&RELEASE_KEY, sums, &sig, "Baguette-1.6.0-linux.tar.gz", &other).is_err_and(|e| e.contains("isn't the one")));
        assert!(check_signed(&RELEASE_KEY, sums, &sig, "Baguette-1.7.0-linux.tar.gz", linux).is_err_and(|e| e.contains("doesn't have")));
        let tampered = SUMS_TEXT.replacen("6d7e", "6d7f", 1);
        assert!(check_signed(&RELEASE_KEY, tampered.as_bytes(), &sig, "Baguette-1.6.0-linux.tar.gz", linux).is_err_and(|e| e.contains("signature")));
        let mut bad_sig = sig.clone();
        bad_sig[0] ^= 1;
        assert!(check_signed(&RELEASE_KEY, sums, &bad_sig, "Baguette-1.6.0-linux.tar.gz", linux).is_err());
        assert!(check_signed(&[7u8; 32], sums, &sig, "Baguette-1.6.0-linux.tar.gz", linux).is_err(), "another key");
    }

    #[test]
    fn hashes_files() {
        let f = std::env::temp_dir().join(format!("baguette-sha-test-{}", std::process::id()));
        std::fs::write(&f, b"l\n").unwrap();
        assert_eq!(sha256_of(&f).unwrap(), "6d7ebc44c5bc26207e62f4f628f912e1a0f41ed11764891aa7dd99eab83228e7");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn replaces_files() {
        let dir = std::env::temp_dir().join(format!("baguette-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(FILES[1]);
        std::fs::write(&f, b"old").unwrap();
        replace(&f, b"new").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"new");
        assert!(!dir.join(format!("{}.new", FILES[1])).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
