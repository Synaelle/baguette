// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Baguette's engine: the FTP client, Cement's status extras, the saved settings
//! and the two-connection transfer workers. Used by Baguette.exe and baguette-cli.exe.

pub mod bundle;
pub mod config;
pub mod ftp;
pub mod listing;
pub mod protect;
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

/// set by the build: the VERSION file (major.minor.micro), plus -<commit>-dev for a build with uncommitted changes
pub const VERSION: &str = match option_env!("BAGUETTE_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// The 3DS app must report at least this protocol level (SITE STATUS
/// "protocol="). Bump it together with PROTOCOL in Cement (src/main.rs), and only
/// for changes that really need the new 3DS build: Baguette warns about an outdated
/// 3DS app only when it is below this level.
pub const REQUIRED_PROTOCOL: i32 = 1;

/// The newest Cement protocol level this Baguette speaks. The apps have their
/// own version numbers: this is what tells which Baguette goes with which
/// Cement. Raise it with each protocol level Baguette learns.
pub const PROTOCOL: i32 = 3;

/// Installing CIAs and the synced accent colour need this level of Cement
/// (SITE INSTALL, SITE COLOR). Older ones still work, without those.
pub const RC2_PROTOCOL: i32 = 2;

/// Sending many files as one bundle (SITE BUNDLE) needs this level of Cement.
pub const BUNDLE_PROTOCOL: i32 = 3;

/// Cement's own user name and password (SITE LOGIN) need this level too.
pub const LOGIN_PROTOCOL: i32 = 3;

/// an upload of at least this many files goes as one bundle (when allowed)
pub const BUNDLE_MIN_FILES: usize = 8;
