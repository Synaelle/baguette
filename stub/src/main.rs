// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! cement-cli - the old name of chomp-cli, kept so existing scripts and agents
//! still work: runs chomp-cli.exe from the same folder with the same arguments
//! and returns its exit code. Output goes straight through; a notice on stderr
//! says it is a stub that will go away. Remove it in a later build.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    // stderr, so the JSON on stdout stays clean for scripts
    eprintln!("cement-cli: this is a stub that runs chomp-cli.exe. cement-cli will be removed in a later build: use chomp-cli instead.");
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("chomp-cli.exe")));
    let Some(exe) = exe else { return ExitCode::from(1) };
    match Command::new(&exe).args(std::env::args_os().skip(1)).status() {
        Ok(st) => ExitCode::from(st.code().unwrap_or(1) as u8),
        Err(_) => {
            eprintln!("cement-cli: chomp-cli.exe not found next to cement-cli.exe ({})", exe.display());
            ExitCode::from(1)
        }
    }
}
