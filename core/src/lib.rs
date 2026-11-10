// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Baguette's engine: the FTP client, Cement's status extras, the saved settings
//! and the two-connection transfer workers. Used by Baguette.exe and baguette-cli.exe.

pub mod bundle;
pub mod config;
pub mod ftp;
pub mod ini;
#[cfg(not(windows))]
#[allow(unsafe_code)] // libc calls
pub mod linux;
pub mod listing;
pub mod png;
pub mod protect;
pub mod remote;
pub mod sites;
#[cfg(feature = "sftp")]
pub mod sftp;
pub mod status;
#[cfg(feature = "tls")]
pub mod tls;
pub mod util;
#[cfg(windows)]
#[allow(unsafe_code)] // Win32 calls
pub mod win;
pub mod workers;

/// this system's side: its folders, the Recycle Bin or trash, drives or mount
/// points, opening files, the saved passwords' encryption
#[cfg(not(windows))]
pub use linux as os;
#[cfg(windows)]
pub use win as os;

/// set by the build: the VERSION file (major.minor.micro), plus -<commit>-dev for a build with uncommitted changes
pub const VERSION: &str = match option_env!("BAGUETTE_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// The 3DS app must report at least this protocol level (SITE STATUS
/// "protocol="). Bump it together with PROTOCOL in Cement (src/main.rs), and only
/// for changes that really need the new 3DS build: Baguette warns about an outdated
/// 3DS app only when it is below this level.
pub const REQUIRED_PROTOCOL: i32 = 3;

/// The newest Cement protocol level this Baguette speaks. The apps have their
/// own version numbers: this is what tells which Baguette goes with which
/// Cement. Raise it with each protocol level Baguette learns.
pub const PROTOCOL: i32 = 3;

/// an upload of at least this many files goes as one bundle (when allowed)
pub const BUNDLE_MIN_FILES: usize = 8;
