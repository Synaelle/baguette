// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Chomp's engine: the FTP client, Cement's status extras, the saved settings
//! and the two-connection transfer workers. Used by Chomp.exe and chomp-cli.exe.

pub mod config;
pub mod ftp;
pub mod listing;
pub mod remote;
pub mod sites;
#[cfg(feature = "sftp")]
pub mod sftp;
pub mod status;
#[cfg(feature = "tls")]
pub mod tls;
pub mod util;
pub mod win;
pub mod workers;

/// set by the build: <VERSION file>.<commit count>-<commit>[-dev]
pub const VERSION: &str = match option_env!("CHOMP_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// The 3DS app must report at least this protocol level (SITE STATUS
/// "protocol="). Bump it together with PROTOCOL in Cement (src/main.rs), and only
/// for changes that really need the new 3DS build: Chomp warns about an outdated
/// 3DS app only when it is below this level.
pub const REQUIRED_PROTOCOL: i32 = 1;

/// Installing CIAs and the synced accent colour need this level of Cement
/// (SITE INSTALL, SITE COLOR). Older ones still work, without those.
pub const RC2_PROTOCOL: i32 = 2;
