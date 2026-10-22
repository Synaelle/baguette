// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The PC side of the window: folders of this computer. An empty path is
//! "This PC", the list of drives.

use std::os::windows::fs::MetadataExt;

use baguette_core::win;
use serde::Serialize;

use crate::{EntryOut, St};

#[derive(Serialize)]
pub struct LocalList {
    path: String,
    ok: bool,
    error: String,
    entries: Vec<EntryOut>,
}

/// hidden and system files stay hidden, like in Explorer's default view
fn hidden(m: &std::fs::Metadata) -> bool {
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    m.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[tauri::command]
pub fn local_list(path: String) -> LocalList {
    if path.is_empty() {
        let entries = win::drives().into_iter().map(|d| EntryOut { name: d, dir: true, size: 0, modified: String::new() }).collect();
        return LocalList { path, ok: true, error: String::new(), entries };
    }
    let rd = match std::fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(e) => return LocalList { path, ok: false, error: e.to_string(), entries: Vec::new() },
    };
    let mut entries = Vec::new();
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if hidden(&m) {
            continue;
        }
        entries.push(EntryOut {
            name: e.file_name().to_string_lossy().into_owned(),
            dir: m.is_dir(),
            size: if m.is_dir() { 0 } else { m.len() },
            modified: win::local_stamp(m.last_write_time()),
        });
    }
    LocalList { path, ok: true, error: String::new(), entries }
}

/// "C:\a\b" -> "C:\a", "C:\" -> "" (This PC)
#[tauri::command]
pub fn local_parent(path: String) -> String {
    let p = path.trim_end_matches('\\');
    match p.rfind('\\') {
        Some(i) if p[..i].ends_with(':') => format!("{}\\", &p[..i]),
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

#[tauri::command]
pub fn local_join(dir: String, name: String) -> String {
    if dir.ends_with('\\') { format!("{dir}{name}") } else { format!("{dir}\\{name}") }
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
    if win::recycle(&paths) { Ok(()) } else { Err("Some items could not be deleted (in use?)".into()) }
}

#[tauri::command]
pub fn local_open(path: String) -> Result<(), String> {
    win::launch_file(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents() {
        assert_eq!(local_parent("C:\\Users\\me".into()), "C:\\Users");
        assert_eq!(local_parent("C:\\Users".into()), "C:\\");
        assert_eq!(local_parent("C:\\".into()), "");
        assert_eq!(local_join("C:\\".into(), "a".into()), "C:\\a");
        assert_eq!(local_join("C:\\x".into(), "a".into()), "C:\\x\\a");
    }
}
