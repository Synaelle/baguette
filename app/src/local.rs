// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! The PC side of the window: folders of this computer. An empty path is
//! "This PC", the list of drives (on Linux: /, the home folder and the mounted disks).

use baguette_core::os;
use serde::Serialize;

use crate::{EntryOut, St};

#[derive(Serialize)]
pub struct LocalList {
    path: String,
    ok: bool,
    error: String,
    entries: Vec<EntryOut>,
}

#[tauri::command]
pub fn local_list(path: String) -> LocalList {
    if path.is_empty() {
        let entries = os::drives().into_iter().map(|d| EntryOut { name: d, dir: true, size: 0, modified: String::new() }).collect();
        return LocalList { path, ok: true, error: String::new(), entries };
    }
    let rd = match std::fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(e) => return LocalList { path, ok: false, error: e.to_string(), entries: Vec::new() },
    };
    let mut entries = Vec::new();
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if os::hidden(&name, &m) {
            continue;
        }
        entries.push(EntryOut {
            name,
            dir: m.is_dir(),
            size: if m.is_dir() { 0 } else { m.len() },
            modified: os::local_stamp(os::file_time(&m)),
        });
    }
    LocalList { path, ok: true, error: String::new(), entries }
}

/// "C:\a\b" -> "C:\a", "C:\" -> "" (This PC)
#[cfg(windows)]
#[tauri::command]
pub fn local_parent(path: String) -> String {
    let p = path.trim_end_matches('\\');
    match p.rfind('\\') {
        Some(i) if p[..i].ends_with(':') => format!("{}\\", &p[..i]),
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// "/a/b" -> "/a", "/a" -> "/", "/" -> "" (This PC)
#[cfg(not(windows))]
#[tauri::command]
pub fn local_parent(path: String) -> String {
    let p = path.trim_end_matches('/');
    match p.rfind('/') {
        _ if p.is_empty() => String::new(),
        Some(0) => "/".into(),
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

#[tauri::command]
pub fn local_join(dir: String, name: String) -> String {
    os::join(&dir, &name)
}

#[tauri::command]
pub fn local_mkdir(state: St, path: String) -> Result<(), String> {
    state.windows_guard(&path, false)?;
    std::fs::create_dir(&path).map_err(|e| format!("Can't create {path}: {e}"))
}

#[tauri::command]
pub fn local_rename(state: St, from: String, to: String) -> Result<(), String> {
    state.windows_guard(&from, true)?;
    state.windows_guard(&to, false)?;
    std::fs::rename(&from, &to).map_err(|e| format!("Can't rename {from}: {e}"))
}

/// to the Recycle Bin
#[tauri::command]
pub fn local_delete(state: St, paths: Vec<String>) -> Result<(), String> {
    for p in &paths {
        state.windows_guard(p, true)?;
    }
    if os::recycle(&paths) { Ok(()) } else { Err("Some items could not be deleted (in use?)".into()) }
}

#[tauri::command]
pub fn local_open(path: String) -> Result<(), String> {
    os::launch_file(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn parents() {
        assert_eq!(local_parent("/home/me".into()), "/home");
        assert_eq!(local_parent("/home/".into()), "/");
        assert_eq!(local_parent("/".into()), "");
        assert_eq!(local_join("/".into(), "a".into()), "/a");
        assert_eq!(local_join("/x".into(), "a".into()), "/x/a");
    }

    #[cfg(windows)]
    #[test]
    fn parents() {
        assert_eq!(local_parent("C:\\Users\\me".into()), "C:\\Users");
        assert_eq!(local_parent("C:\\Users".into()), "C:\\");
        assert_eq!(local_parent("C:\\".into()), "");
        assert_eq!(local_join("C:\\".into(), "a".into()), "C:\\a");
        assert_eq!(local_join("C:\\x".into(), "a".into()), "C:\\x\\a");
    }
}
