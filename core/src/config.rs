// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The saved settings: chomp.ini next to the exe (Chomp is portable), or the
//! file named by CHOMP_CONFIG. Same format as the C++ Chomp's config.ini.

use windows_sys::Win32::System::WindowsProgramming::{GetPrivateProfileIntW, GetPrivateProfileStringW, WritePrivateProfileStringW};

use crate::win::{roaming_app_data, wide};

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub dark: bool,
    pub send_dir: String,        // "Send to PC" destination folder
    pub send_items: Vec<String>, // "Send to PC" 3DS paths
    pub send_replace: bool,      // "Send to PC" empties the PC folder first
    pub accent: String,          // "b8283c": the window's red, also sent to Cement
}

pub const DEFAULT_ACCENT: &str = "b8283c";

/// "#B8283C" or "b8283c" -> "b8283c"; None when it isn't a colour
pub fn normalize_accent(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#').to_ascii_lowercase();
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
        }
    }
}

/// the folder holding the running exe: Chomp is portable, everything stays there
pub fn exe_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .unwrap_or_else(|| ".".into())
}

/// settings saved by earlier versions in %APPDATA% (read once, never written)
fn legacy_config() -> Option<String> {
    let base = roaming_app_data()?;
    ["\\Chomp\\config.ini", "\\Cement\\config.ini", "\\Syna 3DS Tools\\config.ini", "\\synaftp\\config.ini"]
        .iter()
        .map(|old| format!("{base}{old}"))
        .find(|p| std::fs::metadata(p).is_ok())
}

/// the settings file: chomp.ini next to the exe (or CHOMP_CONFIG). The first
/// time, settings from an earlier, non-portable version are copied over.
pub fn config_path() -> String {
    if let Some(custom) = std::env::var_os("CHOMP_CONFIG") {
        return custom.to_string_lossy().into_owned();
    }
    let path = format!("{}\\chomp.ini", exe_dir());
    if std::fs::metadata(&path).is_err() {
        if let Some(old) = legacy_config() {
            let _ = std::fs::copy(old, &path);
        }
    }
    path
}

/// the server address only, without writing anything (chomp-cli)
pub fn read_address() -> Option<(String, u16)> {
    let file = match std::env::var_os("CHOMP_CONFIG") {
        Some(f) => f.to_string_lossy().into_owned(),
        None => {
            let file = format!("{}\\chomp.ini", exe_dir());
            if std::fs::metadata(&file).is_ok() { file } else { legacy_config()? }
        }
    };
    let host = get_str(&file, "server", "host", "");
    if host.is_empty() {
        return None;
    }
    Some((host, get_int(&file, "server", "port", 5000) as u16))
}

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

pub(crate) fn get_int(file: &str, section: &str, key: &str, default: i32) -> i32 {
    unsafe { GetPrivateProfileIntW(wide(section).as_ptr(), wide(key).as_ptr(), default, wide(file).as_ptr()) as i32 }
}

pub(crate) fn put(file: &str, section: &str, key: Option<&str>, value: Option<&str>) {
    let k = key.map(wide);
    let v = value.map(wide);
    unsafe {
        WritePrivateProfileStringW(
            wide(section).as_ptr(),
            k.as_ref().map_or(std::ptr::null(), |k| k.as_ptr()),
            v.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            wide(file).as_ptr(),
        );
    }
}

/// "%XX"-escapes non-ASCII so the list survives an ANSI ini file
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::new();
    for &c in s.as_bytes() {
        if c >= 0x80 || c < 0x20 || c == b'%' {
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
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Config {
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

    pub fn save(&self) {
        let path = config_path();
        put(&path, "server", Some("host"), Some(&self.host));
        put(&path, "server", Some("port"), Some(&self.port.to_string()));
        put(&path, "ui", Some("dark"), Some(if self.dark { "1" } else { "0" }));
        put(&path, "ui", Some("accent"), Some(&self.accent));
        put(&path, "transfer", Some("sendto"), Some(&self.send_dir));
        put(&path, "transfer", Some("replace"), Some(if self.send_replace { "1" } else { "0" }));
        put(&path, "sendtopc", None, None); // drop the old list
        put(&path, "sendtopc", Some("count"), Some(&self.send_items.len().to_string()));
        for (i, item) in self.send_items.iter().enumerate() {
            put(&path, "sendtopc", Some(&format!("item{i}")), Some(&escape(item)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(escape("/été 100%"), "/%C3%A9t%C3%A9 100%25");
        assert_eq!(unescape(&escape("/été 100%")), "/été 100%");
    }
}
