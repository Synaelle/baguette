// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The protocols other than Cement's, against throwaway servers started by the
//! test: SFTP (password, key, host key checks) and FTP / FTPS servers that
//! need a login, have no MLSD or no PASV.

mod common;

use std::path::{Path, PathBuf};

use baguette_core::ftp::{Error, Ftp};
use baguette_core::remote::{self, Protocol, Remote, Site};
use common::{ftpd, sftpd};

fn temp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("baguette-proto-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn site(protocol: Protocol, port: u16) -> Site {
    Site { protocol, host: "127.0.0.1".into(), port, user: "pi".into(), pass: "secret".into(), ..Default::default() }
}

fn none(_: u64) -> baguette_core::ftp::Result<()> {
    Ok(())
}

/// what every protocol must do the same way, in /home/pi of `root`
fn exercise(r: &mut dyn Remote, root: &Path, local: &Path) {
    let home = r.home().unwrap();
    assert_eq!(home, "/home/pi");
    let disk = root.join("home").join("pi");
    std::fs::create_dir_all(&disk).unwrap();

    r.mkdir("/home/pi/new dir").unwrap();
    assert!(disk.join("new dir").is_dir(), "mkdir");

    let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let src = local.join("up.bin");
    std::fs::write(&src, &data).unwrap();
    let mut seen = 0;
    r.upload(src.to_str().unwrap(), "/home/pi/new dir/up.bin", 0, &mut |n| {
        seen += n;
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, data.len() as u64, "progress reports every byte");
    assert_eq!(std::fs::read(disk.join("new dir").join("up.bin")).unwrap(), data, "uploaded");

    // resume: the remote has the first 100000 bytes
    std::fs::write(disk.join("half.bin"), &data[..100_000]).unwrap();
    assert_eq!(r.size("/home/pi/half.bin").unwrap(), Some(100_000));
    r.upload(src.to_str().unwrap(), "/home/pi/half.bin", 100_000, &mut none).unwrap();
    assert_eq!(std::fs::read(disk.join("half.bin")).unwrap(), data, "upload resumed");

    let list = r.list("/home/pi/new dir").unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].name.as_str(), list[0].size, list[0].dir), ("up.bin", 300_000, false));
    assert!(!list[0].modified.is_empty(), "modified time listed");
    let top = r.list("/home/pi").unwrap();
    assert!(top.iter().any(|e| e.name == "new dir" && e.dir), "folders listed");

    let dst = local.join("down.bin");
    std::fs::write(&dst, &data[..50_000]).unwrap();
    r.download("/home/pi/new dir/up.bin", dst.to_str().unwrap(), 50_000, &mut none).unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), data, "download resumed");

    assert_eq!(r.size("/home/pi/missing").unwrap(), None);
    assert!(r.mtime("/home/pi/half.bin").unwrap().is_some());
    r.rename("/home/pi/half.bin", "/home/pi/whole.bin").unwrap();
    assert!(disk.join("whole.bin").is_file(), "renamed");

    // cancelled upload: no truncated file left
    let r2 = r.upload(src.to_str().unwrap(), "/home/pi/cancel.bin", 0, &mut |_| Err(Error::Cancel));
    assert_eq!(r2, Err(Error::Cancel));
    assert!(!disk.join("cancel.bin").exists(), "cancelled upload removed");
    r.keepalive().unwrap();

    r.delete("/home/pi/new dir/up.bin", false).unwrap();
    r.delete("/home/pi/new dir", true).unwrap();
    r.delete("/home/pi/whole.bin", false).unwrap();
    assert!(!disk.join("new dir").exists() && !disk.join("whole.bin").exists(), "deleted");
    assert!(matches!(r.delete("/home/pi/nothing", false), Err(Error::Ftp(_))), "refusal is not a lost connection");
    assert!(r.cement().is_none(), "not Cement");
}

#[test]
fn sftp() {
    let (root, local) = (temp("sftp-root"), temp("sftp-local"));
    let srv = sftpd::start(&root, 1, 2);
    let mut s = site(Protocol::Sftp, srv.port);
    let mut r = remote::for_site(&s);

    // first connection: the host key has to be trusted
    let fp = match r.open(&s) {
        Err(Error::Untrusted { fingerprint, detail }) => {
            assert!(detail.contains("First connection"), "{detail}");
            assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
            fingerprint
        }
        other => panic!("expected an untrusted host key, got {other:?}"),
    };
    s.trusted.push(fp.clone());
    s.pass = "wrong".into();
    assert!(matches!(r.open(&s), Err(Error::Ftp(m)) if m.contains("Login refused")), "wrong password");
    s.pass = "secret".into();
    r.open(&s).unwrap();
    assert!(r.connected());
    exercise(r.as_mut(), &root, &local);
    r.close();
    assert!(!r.connected());

    // key login
    let key = local.join("id_ed25519");
    std::fs::write(&key, sftpd::key(2).to_openssh(russh::keys::ssh_key::LineEnding::LF).unwrap().as_bytes()).unwrap();
    let mut k = s.clone();
    k.pass.clear();
    k.key = key.to_str().unwrap().into();
    r.open(&k).unwrap();
    assert_eq!(r.home().unwrap(), "/home/pi", "logged in with the key");
    r.close();

    // abort from another thread ends a blocked call
    r.open(&s).unwrap();
    r.aborter().abort();
    assert!(r.list("/home/pi").is_err());
    r.close();

    // the server comes back with another host key: refused, and said so
    let other = sftpd::start(&root, 9, 2);
    let mut moved = s.clone();
    moved.port = other.port;
    match r.open(&moved) {
        Err(Error::Untrusted { fingerprint, detail }) => {
            assert!(detail.contains("changed"), "{detail}");
            assert_ne!(fingerprint, fp);
        }
        other => panic!("expected a changed host key, got {other:?}"),
    }
}

#[test]
fn ftp_login_list_epsv() {
    let (root, local) = (temp("ftp-root"), temp("ftp-local"));
    let srv = ftpd::start(&root, ftpd::Options { mlsd: false, pasv: false, ..Default::default() });
    let mut s = site(Protocol::Ftp, srv.port);
    let mut r = remote::for_site(&s);
    s.pass = "nope".into();
    assert!(matches!(r.open(&s), Err(Error::Ftp(m)) if m.contains("Login refused")), "wrong password");
    s.pass = "secret".into();
    r.open(&s).unwrap();
    exercise(r.as_mut(), &root, &local);
}

#[test]
fn ftps_explicit() {
    let (root, local) = (temp("ftpes-root"), temp("ftpes-local"));
    let srv = ftpd::start(&root, ftpd::Options { mlsd: true, pasv: true, tls: true, ..Default::default() });
    let mut s = site(Protocol::Ftps, srv.port);
    let mut r = remote::for_site(&s);
    match r.open(&s) {
        Err(Error::Untrusted { fingerprint, .. }) => {
            assert_eq!(fingerprint, baguette_core::tls::fingerprint(&srv.cert));
            s.trusted.push(fingerprint);
        }
        other => panic!("expected an untrusted certificate, got {other:?}"),
    }
    r.open(&s).unwrap();
    exercise(r.as_mut(), &root, &local);

    // a plain FTP server refuses AUTH TLS: a clear message, no plain login
    let plain = ftpd::start(&root, ftpd::Options { mlsd: true, pasv: true, ..Default::default() });
    let mut p = s.clone();
    p.port = plain.port;
    assert!(matches!(r.open(&p), Err(Error::Ftp(m)) if m.contains("doesn't offer FTPS")));
}

#[test]
fn ftps_implicit() {
    let (root, local) = (temp("ftps-root"), temp("ftps-local"));
    let srv = ftpd::start(&root, ftpd::Options { mlsd: false, pasv: true, implicit: true, ..Default::default() });
    let mut s = site(Protocol::FtpsImplicit, srv.port);
    s.trusted.push(baguette_core::tls::fingerprint(&srv.cert));
    let mut r = remote::for_site(&s);
    r.open(&s).unwrap();
    exercise(r.as_mut(), &root, &local);
}

#[test]
fn cement_style_ftp_still_works_without_login() {
    // the plain Ftp API baguette-cli uses keeps anonymous logins
    let root = temp("anon-root");
    let srv = ftpd::start(&root, ftpd::Options { mlsd: true, pasv: true, ..Default::default() });
    let mut f = Ftp::new();
    assert!(!f.connect("127.0.0.1", srv.port), "this server wants pi/secret, anonymous is refused");
}
