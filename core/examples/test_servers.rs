// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The tests' throwaway SFTP and FTPS servers as a program, for test_app.py:
//! serves <folder> (user pi, password secret), prints "sftp <port>" and
//! "ftps <port>", and runs until killed.

#[allow(dead_code)]
#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: test_servers <folder>"));
    let sftp = common::sftpd::start(&root, 1, 2);
    let ftps = common::ftpd::start(&root, common::ftpd::Options { mlsd: true, pasv: true, tls: true, ..Default::default() });
    println!("sftp {}", sftp.port);
    println!("ftps {}", ftps.port);
    loop {
        std::thread::park();
    }
}
