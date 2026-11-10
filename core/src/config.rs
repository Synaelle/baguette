// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! The saved settings: baguette.ini next to the exe (Baguette is portable), or the
//! file named by BAGUETTE_CONFIG.

#[cfg(windows)]
use windows_sys::Win32::System::WindowsProgramming::{GetPrivateProfileIntW, GetPrivateProfileStringW, WritePrivateProfileStringW};

#[cfg(windows)]
use crate::win::wide;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub dark: bool,
    pub send_dir: String,        // "Send to PC" destination folder
    pub send_items: Vec<String>, // "Send to PC" 3DS paths
    pub send_replace: bool,      // "Send to PC" empties the PC folder first
    pub accent: String,          // "e44c3e": the window's red, also sent to Cement
    pub local_start: String,     // where the PC side opens: "home", "last" or a folder
    pub local_last: String,      // the PC folder shown last
    pub protect_3ds: bool,       // refuse to delete or replace important 3DS files
    pub check_updates: bool,     // ask GitHub for a newer release at start
    pub bundles: bool,           // experimental: many files to Cement as one bundle
    pub bmp_to_png: bool,        // downloaded .bmp files are saved as .png
    pub send_to_pc: bool,        // the Send to PC button on the Cement card
    pub mode: String,            // "standard" or "lite"; empty until chosen at first launch
    pub accent_time: u64,        // when the accent was picked (local clock, 0: never); the newer side wins
    pub look: String,            // "evil-twin", "ficelle" or "ficelle-light"
}

pub const DEFAULT_ACCENT: &str = "e44c3e";
/// the red before Ficelle's: a colour still set to it moves to the new one
const OLD_DEFAULT_ACCENT: &str = "b8283c";

/// "evil-twin", "ficelle" or "ficelle-light" (any case); None for anything else
pub fn normalize_look(s: &str) -> Option<String> {
    let l = s.trim().to_ascii_lowercase();
    matches!(l.as_str(), "evil-twin" | "ficelle" | "ficelle-light").then_some(l)
}

/// "standard" or "lite" (any case); None for anything else
pub fn normalize_mode(s: &str) -> Option<String> {
    let m = s.trim().to_ascii_lowercase();
    matches!(m.as_str(), "standard" | "lite").then_some(m)
}

/// "#2B9349" or "2b9349" -> "2b9349" (the old default red gives the new one); None when it isn't a colour
pub fn normalize_accent(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#').to_ascii_lowercase();
    let h = if h == OLD_DEFAULT_ACCENT { DEFAULT_ACCENT.to_string() } else { h };
    (h.len() == 6 && h.bytes().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

impl Default for Config {
    fn default() -> Config {
        Config {
            host: String::new(),
            port: 5000,
            dark: true,
            send_dir: String::new(),
            send_items: Vec::new(),
            send_replace: false,
            accent: DEFAULT_ACCENT.into(),
            local_start: "home".into(),
            local_last: String::new(),
            protect_3ds: true,
            check_updates: true,
            bundles: false,
            bmp_to_png: false,
            send_to_pc: false,
            mode: String::new(),
            accent_time: 0,
            look: "evil-twin".into(),
        }
    }
}

/// the folder holding the running exe: Baguette is portable, everything stays there
pub fn exe_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ".".into())
}

/// the settings file: baguette.ini next to the exe on Windows, in ~/.config/baguette
/// on Linux (or BAGUETTE_CONFIG)
pub fn config_path() -> String {
    if let Some(custom) = std::env::var_os("BAGUETTE_CONFIG") {
        return custom.to_string_lossy().into_owned();
    }
    crate::os::join(&crate::os::data_dir(), "baguette.ini")
}

/// the settings file to read, if there is one, without creating it (baguette-cli)
pub fn existing_config() -> Option<String> {
    let file = config_path();
    std::fs::metadata(&file).is_ok().then_some(file)
}

/// the server address only, without writing anything (baguette-cli)
pub fn read_address() -> Option<(String, u16)> {
    let file = existing_config()?;
    let host = get_str(&file, "server", "host", "");
    if host.is_empty() {
        return None;
    }
    Some((host, get_int(&file, "server", "port", 5000) as u16))
}

/// the app's "protect important 3DS files" setting (on when nothing is saved)
pub fn protect_3ds() -> bool {
    existing_config().is_none_or(|file| get_int(&file, "ui", "protect_3ds", 1) != 0)
}

#[cfg(windows)]
pub(crate) fn get_str(file: &str, section: &str, key: &str, default: &str) -> String {
    let mut buf = vec![0u16; 4096];
    let n = unsafe {
        GetPrivateProfileStringW(
            wide(section).as_ptr(),
            wide(key).as_ptr(),
            wide(default).as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            wide(file).as_ptr(),
        )
    };
    String::from_utf16_lossy(&buf[..n as usize])
}

#[cfg(windows)]
pub(crate) fn get_int(file: &str, section: &str, key: &str, default: i32) -> i32 {
    unsafe { GetPrivateProfileIntW(wide(section).as_ptr(), wide(key).as_ptr(), default, wide(file).as_ptr()) as i32 }
}

/// why the settings file couldn't be written
fn not_saved(file: &str, e: std::io::Error) -> String {
    format!("Could not save the settings to {file}: {e}")
}

#[cfg(windows)]
pub(crate) fn put(file: &str, section: &str, key: Option<&str>, value: Option<&str>) -> Result<(), String> {
    let k = key.map(wide);
    let v = value.map(wide);
    let ok = unsafe {
        WritePrivateProfileStringW(
            wide(section).as_ptr(),
            k.as_ref().map_or(std::ptr::null(), |k| k.as_ptr()),
            v.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            wide(file).as_ptr(),
        )
    };
    if ok == 0 { Err(not_saved(file, std::io::Error::last_os_error())) } else { Ok(()) }
}

#[cfg(not(windows))]
pub(crate) fn get_str(file: &str, section: &str, key: &str, default: &str) -> String {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    crate::ini::get(&text, section, key).unwrap_or_else(|| default.to_string())
}

#[cfg(not(windows))]
pub(crate) fn get_int(file: &str, section: &str, key: &str, default: i32) -> i32 {
    crate::ini::get_int(&std::fs::read_to_string(file).unwrap_or_default(), section, key, default)
}

/// writes through a temp file and a rename: a crash never leaves half a file
#[cfg(not(windows))]
pub(crate) fn put(file: &str, section: &str, key: Option<&str>, value: Option<&str>) -> Result<(), String> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(not_saved(file, e)),
    };
    let new = crate::ini::set(&text, section, key, value);
    if new == text {
        return Ok(());
    }
    if let Some(dir) = std::path::Path::new(file).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| not_saved(file, e))?;
    }
    let tmp = format!("{file}.tmp");
    std::fs::write(&tmp, new).map_err(|e| not_saved(file, e))?;
    std::fs::rename(&tmp, file).map_err(|e| not_saved(file, e))
}

/// "%XX"-escapes non-ASCII so the list survives an ANSI ini file
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::new();
    for &c in s.as_bytes() {
        if !(0x20..0x80).contains(&c) || c == b'%' {
            out.push_str(&format!("%{c:02X}"));
        } else {
            out.push(c as char);
        }
    }
    out
}

pub(crate) fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len()
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Config {
    /// the folder the PC side opens in; the user folder when the saved one is gone
    pub fn local_start_dir(&self) -> String {
        let pick = match self.local_start.as_str() {
            "home" => String::new(),
            "last" => self.local_last.clone(),
            dir => dir.to_string(),
        };
        if !pick.is_empty() && std::path::Path::new(&pick).is_dir() { pick } else { crate::os::home_dir() }
    }

    /// remembers the PC folder on screen (only written when it changed)
    pub fn remember_local(&mut self, dir: &str) -> Result<(), String> {
        if self.local_last != dir {
            self.local_last = dir.to_string();
            put(&config_path(), "ui", Some("local_last"), Some(&escape(dir)))?;
        }
        Ok(())
    }

    /// None when no server address is saved yet
    pub fn load() -> (Config, bool) {
        let path = config_path();
        let mut c = Config {
            dark: get_int(&path, "ui", "dark", 1) != 0,
            host: get_str(&path, "server", "host", ""),
            port: get_int(&path, "server", "port", 5000) as u16,
            send_dir: get_str(&path, "transfer", "sendto", ""),
            send_replace: get_int(&path, "transfer", "replace", 0) != 0,
            send_items: Vec::new(),
            accent: normalize_accent(&get_str(&path, "ui", "accent", "")).unwrap_or_else(|| DEFAULT_ACCENT.into()),
            local_start: unescape(&get_str(&path, "ui", "local_start", "home")),
            local_last: unescape(&get_str(&path, "ui", "local_last", "")),
            protect_3ds: get_int(&path, "ui", "protect_3ds", 1) != 0,
            check_updates: get_int(&path, "ui", "check_updates", 1) != 0,
            bundles: get_int(&path, "experimental", "bundles", 0) != 0,
            bmp_to_png: get_int(&path, "ui", "bmp_to_png", 0) != 0,
            send_to_pc: get_int(&path, "ui", "send_to_pc", 0) != 0,
            mode: normalize_mode(&get_str(&path, "ui", "mode", "")).unwrap_or_default(),
            accent_time: get_str(&path, "ui", "accent_time", "0").trim().parse().unwrap_or(0),
            look: normalize_look(&get_str(&path, "ui", "look", "")).unwrap_or_else(|| "evil-twin".into()),
        };
        let count = get_int(&path, "sendtopc", "count", 0);
        for i in 0..count {
            let item = get_str(&path, "sendtopc", &format!("item{i}"), "");
            if !item.is_empty() {
                c.send_items.push(unescape(&item));
            }
        }
        let configured = !c.host.is_empty();
        (c, configured)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        put(&path, "server", Some("host"), Some(&self.host))?;
        put(&path, "server", Some("port"), Some(&self.port.to_string()))?;
        put(&path, "ui", Some("dark"), Some(if self.dark { "1" } else { "0" }))?;
        put(&path, "ui", Some("accent"), Some(&self.accent))?;
        put(&path, "ui", Some("accent_time"), Some(&self.accent_time.to_string()))?;
        put(&path, "ui", Some("look"), Some(&self.look))?;
        put(&path, "ui", Some("local_start"), Some(&escape(&self.local_start)))?;
        put(&path, "ui", Some("protect_3ds"), Some(if self.protect_3ds { "1" } else { "0" }))?;
        put(&path, "ui", Some("check_updates"), Some(if self.check_updates { "1" } else { "0" }))?;
        put(&path, "experimental", Some("bundles"), Some(if self.bundles { "1" } else { "0" }))?;
        put(&path, "ui", Some("bmp_to_png"), Some(if self.bmp_to_png { "1" } else { "0" }))?;
        put(&path, "ui", Some("send_to_pc"), Some(if self.send_to_pc { "1" } else { "0" }))?;
        if !self.mode.is_empty() {
            put(&path, "ui", Some("mode"), Some(&self.mode))?;
        }
        put(&path, "ui", Some("local_last"), Some(&escape(&self.local_last)))?;
        put(&path, "transfer", Some("sendto"), Some(&self.send_dir))?;
        put(&path, "transfer", Some("replace"), Some(if self.send_replace { "1" } else { "0" }))?;
        put(&path, "sendtopc", None, None)?; // drop the old list
        put(&path, "sendtopc", Some("count"), Some(&self.send_items.len().to_string()))?;
        for (i, item) in self.send_items.iter().enumerate() {
            put(&path, "sendtopc", Some(&format!("item{i}")), Some(&escape(item)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(escape("/été 100%"), "/%C3%A9t%C3%A9 100%25");
        assert_eq!(unescape(&escape("/été 100%")), "/été 100%");
        assert_eq!(unescape("%é%"), "%é%", "a stray % before a non-ASCII letter");
    }

    #[test]
    fn accents() {
        assert_eq!(normalize_accent("#2B9349").as_deref(), Some("2b9349"));
        assert_eq!(normalize_accent("b8283c").as_deref(), Some(DEFAULT_ACCENT), "the old red moves to the new one");
        assert_eq!(normalize_accent("#12345"), None);
    }

    #[test]
    fn looks() {
        assert_eq!(normalize_look(" Ficelle-Light ").as_deref(), Some("ficelle-light"));
        assert_eq!(normalize_look("evil-twin").as_deref(), Some("evil-twin"));
        assert_eq!(normalize_look("miku"), None);
    }
}
