// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows helpers: known folders, the temp folder, the Recycle Bin, opening
//! files with their app, file times.

use std::path::Path;

use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::core::GUID;

/// what joins local paths
pub const SEP: char = '\\';

/// "dir\name"
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('\\') { format!("{dir}{name}") } else { format!("{dir}\\{name}") }
}

/// where baguette.ini and WebView2's data are kept: next to the exe (Baguette is portable)
pub fn data_dir() -> String {
    crate::config::exe_dir()
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

pub fn known_folder(id: &GUID) -> Option<String> {
    let mut p: *mut u16 = std::ptr::null_mut();
    let ok = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p) } >= 0;
    let s = from_wide(p);
    unsafe { CoTaskMemFree(p.cast()) };
    (ok && !s.is_empty()).then_some(s)
}

/// the temp folder next to the exe (Baguette is portable): drag-out downloads and
/// opened files; emptied at start. Named so it can't be a folder the user made
/// beside a copy kept in Downloads or on the Desktop.
pub fn temp_root() -> String {
    format!("{}\\Baguette.temp", crate::config::exe_dir())
}

/// "Replace PC folder" must never empty a drive or a personal/system folder
pub fn unsafe_replace_target(dir: &str) -> bool {
    if dir.trim().is_empty() {
        return true;
    }
    let Ok(full) = std::path::absolute(dir) else { return true };
    let mut f = full.to_string_lossy().into_owned();
    while f.len() > 3 && (f.ends_with('\\') || f.ends_with('/')) {
        f.pop();
    }
    if f.len() <= 3 {
        return true; // "C:\" or shorter
    }
    let ids = [
        FOLDERID_Profile,
        FOLDERID_Desktop,
        FOLDERID_Documents,
        FOLDERID_Downloads,
        FOLDERID_Pictures,
        FOLDERID_Music,
        FOLDERID_Videos,
        FOLDERID_Windows,
        FOLDERID_ProgramFiles,
        FOLDERID_ProgramFilesX86,
        FOLDERID_RoamingAppData,
        FOLDERID_LocalAppData,
        FOLDERID_Public,
    ];
    ids.iter().any(|id| known_folder(id).is_some_and(|k| k.eq_ignore_ascii_case(&f)))
}

fn shell_delete(paths: &[String], recycle: bool) -> bool {
    if paths.is_empty() {
        return true;
    }
    // SHFileOperation wants a double-null-terminated list
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.encode_utf16());
        list.push(0);
    }
    list.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = list.as_ptr();
    op.fFlags = (FOF_NO_UI | if recycle { FOF_ALLOWUNDO } else { 0 }) as u16;
    unsafe { SHFileOperationW(&mut op) == 0 && op.fAnyOperationsAborted == 0 }
}

/// deletes everything inside dir (not dir itself); to the Recycle Bin when recycle
pub fn empty_folder(dir: &str, recycle: bool) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return true };
    let paths: Vec<String> = rd.flatten().map(|e| e.path().to_string_lossy().into_owned()).collect();
    shell_delete(&paths, recycle)
}

/// deletes a file or folder tree for good
pub fn delete_tree(path: &str) {
    if Path::new(path).exists() {
        shell_delete(&[path.to_string()], false);
    }
}

/// opens a file with its Windows app; lets the user pick one when none is registered.
/// Err(text) when Windows could not open it.
pub fn launch_file(path: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    if std::env::var_os("BAGUETTE_NO_LAUNCH").is_some() {
        return Ok(()); // tests: download and watch only
    }
    let (verb, file) = (wide("open"), wide(path));
    let rc = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    } as isize;
    if rc == SE_ERR_NOASSOC as isize || rc == SE_ERR_ASSOCINCOMPLETE as isize {
        // no app registered for this type: let the user pick one
        let info = OPENASINFO {
            pcszFile: file.as_ptr(),
            pcszClass: std::ptr::null(),
            oaifInFlags: OAIF_EXEC | OAIF_ALLOW_REGISTRATION,
        };
        unsafe { SHOpenWithDialog(std::ptr::null_mut(), &info) };
        Ok(())
    } else if rc <= 32 {
        Err(format!("Windows could not open {path}"))
    } else {
        Ok(())
    }
}

/// a file's last write time (100 ns units, as Windows keeps it)
pub fn file_time(m: &std::fs::Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    m.last_write_time()
}

/// last write time (file_time) and size of a local file
pub fn last_write(path: &str) -> Option<(u64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((file_time(&m), m.len()))
}

/// hidden and system files stay hidden, like in Explorer's default view
pub fn hidden(_name: &str, m: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    m.file_attributes() & (HIDDEN | SYSTEM) != 0
}

/// now on the local wall clock, in seconds since 1970 (the clock Cement's
/// colour times use: the 3DS keeps local time)
pub fn local_now() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let ticks = (now + 11_644_473_600) * 10_000_000;
    let a = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut b = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    unsafe { FileTimeToLocalFileTime(&a, &mut b) };
    let local = ((b.dwHighDateTime as u64) << 32) | b.dwLowDateTime as u64;
    (local / 10_000_000).saturating_sub(11_644_473_600)
}

/// "YYYY-MM-DD hh:mm:ss" in local time, from a Windows file time
pub fn local_time_text(ft: u64) -> String {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;
    let a = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let mut b = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe {
        FileTimeToLocalFileTime(&a, &mut b);
        FileTimeToSystemTime(&b, &mut st);
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond)
}

/// encrypts for this Windows user (DPAPI), as hex for the ini file; None when
/// Windows refuses
pub fn protect(text: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData};
    if text.is_empty() {
        return Some(String::new());
    }
    let input = CRYPT_INTEGER_BLOB { cbData: text.len() as u32, pbData: text.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) };
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect();
    unsafe { LocalFree(out.pbData.cast()) };
    Some(hex)
}

/// None when it was encrypted by another Windows user or another PC
pub fn unprotect(hex: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData};
    if hex.is_empty() {
        return Some(String::new());
    }
    let mut bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
    let input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_mut_ptr() };
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
    };
    if ok == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }).into_owned();
    unsafe { LocalFree(out.pbData.cast()) };
    Some(text)
}

/// the Windows folder ("C:\Windows")
pub fn windows_dir() -> Option<String> {
    known_folder(&FOLDERID_Windows).or_else(|| std::env::var("WINDIR").ok())
}

/// to the Recycle Bin
pub fn recycle(paths: &[String]) -> bool {
    shell_delete(paths, true)
}

/// "YYYYMMDDhhmmss" in local time, from a Windows file time
pub fn local_stamp(ft: u64) -> String {
    local_time_text(ft).chars().filter(char::is_ascii_digit).collect()
}

/// "C:\", "D:\"...
pub fn drives() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8).filter(|i| mask & (1 << i) != 0).map(|i| format!("{}:\\", (b'A' + i) as char)).collect()
}

/// the user's folder, where the PC side starts
pub fn home_dir() -> String {
    std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into())
}
