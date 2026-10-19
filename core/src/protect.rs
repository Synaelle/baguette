// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The 3DS paths Chomp refuses to touch on Cement unless the user turns the
//! protection off: losing them can wipe installed games and saves, or stop the
//! console from booting into its custom firmware.

/// how far a path is protected
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    /// no delete, rename or write, inside either
    All,
    /// no delete or rename of this path itself (writing inside is fine)
    Keep,
}

const PATHS: &[(&str, Level, &str)] = &[
    ("/Nintendo 3DS", Level::All, "installed games, saves and extra data"),
    ("/boot.firm", Level::All, "what the 3DS boots (Luma3DS)"),
    ("/luma", Level::Keep, "Luma3DS settings and payloads"),
    ("/gm9", Level::Keep, "GodMode9 files, like NAND backups"),
    ("/boot.3dsx", Level::Keep, "the Homebrew Launcher"),
];

fn lower(p: &str) -> String {
    p.trim_end_matches('/').to_ascii_lowercase()
}

/// a is b or inside it
fn within(a: &str, b: &str) -> bool {
    let (a, b) = (lower(a), lower(b));
    a == b || a.starts_with(&format!("{b}/"))
}

/// Some(why) when deleting or renaming this path would hit a protected one
/// (the path itself, a folder holding one, or anything inside an All path)
pub fn delete_or_rename(path: &str) -> Option<String> {
    let path = if path.is_empty() { "/" } else { path };
    for (p, level, what) in PATHS {
        let hit = within(p, path) || (*level == Level::All && within(path, p));
        if hit {
            return Some(format!("{p} is protected: it holds {what}"));
        }
    }
    None
}

/// Some(why) when writing this file would replace or add to a protected one
pub fn write(path: &str) -> Option<String> {
    for (p, level, what) in PATHS {
        if *level == Level::All && within(path, p) {
            return Some(format!("{p} is protected: it holds {what}"));
        }
    }
    None
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
        assert!(delete_or_rename("/luma/payloads/old.firm").is_none(), "inside a Keep folder is fine");
        assert!(delete_or_rename("/3ds/app").is_none());
        assert!(delete_or_rename("/3ds").is_none(), "homebrew lives there: not protected");
        assert!(delete_or_rename("/Nintendo 3DS games").is_none(), "only the exact name");
        assert!(delete_or_rename("/notes.txt").is_none());
        assert!(write("/boot.firm").is_some());
        assert!(write("/Nintendo 3DS/x").is_some());
        assert!(write("/luma/payloads/new.firm").is_none());
        assert!(write("/boot.3dsx").is_none(), "updating the Homebrew Launcher is normal");
    }
}
