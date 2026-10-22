// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The 3DS paths Baguette keeps read-only on Cement unless the user turns the
//! protection off: losing them can wipe installed games and saves, or stop the
//! console from booting into its custom firmware.

/// the protected paths, all read-only: no delete, rename or write, inside either
pub const PATHS: &[(&str, &str)] = &[
    ("/Nintendo 3DS", "installed games, saves and extra data"),
    ("/boot.firm", "what the 3DS boots (Luma3DS)"),
    ("/luma", "Luma3DS settings and payloads"),
    ("/gm9", "GodMode9 files, like NAND backups"),
    ("/boot.3dsx", "the Homebrew Launcher"),
];

/// inside a protected folder but still free: what Luma3DS writes for you to take
pub const FREE: &[&str] = &["/luma/screenshots", "/luma/dumps"];

fn free(path: &str) -> bool {
    FREE.iter().any(|f| within(path, f))
}

fn lower(p: &str) -> String {
    p.trim_end_matches('/').to_ascii_lowercase()
}

/// a is b or inside it
fn within(a: &str, b: &str) -> bool {
    let (a, b) = (lower(a), lower(b));
    a == b || a.starts_with(&format!("{b}/"))
}

/// Some(why) when deleting or renaming this path would hit a protected one
/// (the path itself, something inside one, or a folder holding one)
pub fn delete_or_rename(path: &str) -> Option<String> {
    let path = if path.is_empty() { "/" } else { path };
    PATHS
        .iter()
        .find(|(p, _)| (within(path, p) && !free(path)) || within(p, path))
        .map(|(p, what)| format!("{p} is read-only: it holds {what}"))
}

/// Some(why) when writing or creating this path would change a protected one
pub fn write(path: &str) -> Option<String> {
    PATHS
        .iter()
        .find(|(p, _)| within(path, p) && !free(path))
        .map(|(p, what)| format!("{p} is read-only: it holds {what}"))
}

/// for a local folder kept read-only (the Windows folder): is `path` it, inside
/// it, or a folder holding it?
pub fn local_hit(path: &str, protected: &str) -> bool {
    let (a, b) = (local_norm(path), local_norm(protected));
    !b.is_empty() && (a == b || a.starts_with(&format!("{b}\\")) || b.starts_with(&format!("{a}\\")))
}

/// for a local folder kept read-only: is `path` it or inside it (writing there)?
pub fn local_inside(path: &str, protected: &str) -> bool {
    let (a, b) = (local_norm(path), local_norm(protected));
    !b.is_empty() && (a == b || a.starts_with(&format!("{b}\\")))
}

/// "c:/Windows/" -> "c:\windows"
fn local_norm(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert!(delete_or_rename("/Nintendo 3DS").is_some());
        assert!(delete_or_rename("/nintendo 3ds/abc/title").is_some(), "inside, any case");
        assert!(delete_or_rename("/").is_some(), "the root holds them all");
        assert!(delete_or_rename("/boot.firm").is_some());
        assert!(delete_or_rename("/luma").is_some());
        assert!(delete_or_rename("/luma/payloads/old.firm").is_some(), "read-only inside too");
        assert!(delete_or_rename("/3ds/app").is_none());
        assert!(delete_or_rename("/3ds").is_none(), "homebrew lives there: not protected");
        assert!(delete_or_rename("/Nintendo 3DS games").is_none(), "only the exact name");
        assert!(delete_or_rename("/notes.txt").is_none());
        assert!(write("/boot.firm").is_some());
        assert!(write("/Nintendo 3DS/x").is_some());
        assert!(write("/luma/payloads/new.firm").is_some());
        assert!(write("/boot.3dsx").is_some());
        assert!(write("/3ds/new.3dsx").is_none());
        assert!(write("/luma/screenshots/top.bmp").is_none(), "Luma's screenshots stay free");
        assert!(delete_or_rename("/luma/screenshots/top.bmp").is_none());
        assert!(delete_or_rename("/luma/dumps/arm11/crash.dmp").is_none(), "and its crash dumps");
        assert!(delete_or_rename("/luma/screenshots").is_none());
        assert!(delete_or_rename("/luma/config.ini").is_some());
        assert!(local_hit(r"C:\Windows\System32", r"C:\WINDOWS"));
        assert!(local_hit("c:/windows", r"C:\Windows\"));
        assert!(local_hit(r"C:\", r"C:\Windows"), "a drive holding it");
        assert!(!local_hit(r"C:\Windows.old", r"C:\Windows"));
        assert!(!local_hit(r"D:\Games", r"C:\Windows"));
        assert!(local_inside(r"C:\Windows\Temp\x", r"C:\Windows"));
        assert!(!local_inside(r"C:\", r"C:\Windows"));
    }
}
