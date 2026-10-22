// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! cement-cli and chomp-cli - the old names of baguette-cli, kept so existing
//! scripts and agents still work: runs baguette-cli.exe from the same folder with
//! the same arguments and returns its exit code. Output goes straight through; a
//! notice on stderr says it is a stub that will go away. Remove it in a later build.

use std::process::{Command, ExitCode};

/// runs baguette-cli.exe in place of this stub
pub fn run() -> ExitCode {
    let me = std::env::current_exe().ok();
    let name = me.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "cement-cli".into());
    // stderr, so the JSON on stdout stays clean for scripts
    eprintln!("{name}: this is a stub that runs baguette-cli.exe. {name} will be removed in a later build: use baguette-cli instead.");
    let exe = me.as_ref().and_then(|p| p.parent().map(|d| d.join("baguette-cli.exe")));
    let Some(exe) = exe else { return ExitCode::from(1) };
    match Command::new(&exe).args(std::env::args_os().skip(1)).status() {
        Ok(st) => ExitCode::from(st.code().unwrap_or(1) as u8),
        Err(_) => {
            eprintln!("{name}: baguette-cli.exe not found next to {name}.exe ({})", exe.display());
            ExitCode::from(1)
        }
    }
}
