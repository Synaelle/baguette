// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! SFTP over SSH (russh, pure Rust). The library is async: each connection
//! owns a small single-thread runtime and blocks on it, so the workers use it
//! like the FTP client. Host keys are checked against the site's trusted
//! fingerprints: an unknown or changed key is refused with its fingerprint, so
//! the window can ask.

use std::io::SeekFrom;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{OpenFlags, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::ftp::{Aborter, Entry, Error, Ftp, Result, connect_to};
use crate::remote::{Progress, Remote, Site};
use crate::util::civil_from_days;

/// the host key the server showed: (fingerprint, accepted)
type Seen = Arc<Mutex<Option<(String, bool)>>>;

struct Keys {
    trusted: Vec<String>,
    seen: Seen,
}

impl client::Handler for Keys {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> std::result::Result<bool, Self::Error> {
        let data = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.key_data().clone(),
            PublicKeyOrCertificate::Certificate(c) => c.public_key().clone(),
        };
        let fp = data.fingerprint(HashAlg::Sha256).to_string();
        let ok = self.trusted.iter().any(|t| *t == fp);
        *self.seen.lock().unwrap() = Some((fp, ok));
        Ok(ok)
    }
}

struct Conn {
    ssh: Handle<Keys>,
    sftp: SftpSession,
}

pub struct Sftp {
    rt: tokio::runtime::Runtime,
    conn: Option<Conn>,
    aborter: Aborter,
}

impl Default for Sftp {
    fn default() -> Sftp {
        Sftp::new()
    }
}

/// "YYYYMMDDhhmmss" (UTC) of a Unix time
fn stamp(t: u32) -> String {
    let t = t as i64;
    let (y, m, d) = civil_from_days(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    format!("{y:04}{m:02}{d:02}{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn sftp_err(e: SftpError) -> Error {
    match e {
        SftpError::Status(s) if s.status_code == StatusCode::ConnectionLost || s.status_code == StatusCode::NoConnection => {
            Error::Net
        }
        SftpError::Status(s) => Error::Ftp(if s.error_message.is_empty() { format!("{:?}", s.status_code) } else { s.error_message }),
        SftpError::IO(_) | SftpError::Timeout => Error::Net,
        e => Error::Ftp(e.to_string()),
    }
}

fn missing(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile)
}

impl Sftp {
    pub fn new() -> Sftp {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        Sftp { rt, conn: None, aborter: Aborter::default() }
    }

    fn sftp(&self) -> Result<&SftpSession> {
        self.conn.as_ref().map(|c| &c.sftp).ok_or(Error::Net)
    }

    async fn login(ssh: &mut Handle<Keys>, site: &Site) -> Result<()> {
        let net = |_| Error::Net;
        let user = if site.user.is_empty() { "root".to_string() } else { site.user.clone() };
        if !site.key.is_empty() {
            let pass = (!site.passphrase.is_empty()).then_some(site.passphrase.as_str());
            let key = load_secret_key(&site.key, pass).map_err(|e| {
                Error::Ftp(format!("Can't read the key file {} ({e}): check the passphrase", site.key))
            })?;
            let hash = ssh.best_supported_rsa_hash().await.map_err(net)?.flatten();
            let r = ssh.authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await.map_err(net)?;
            if r.success() {
                return Ok(());
            }
        }
        if !site.pass.is_empty() {
            if ssh.authenticate_password(&user, &site.pass).await.map_err(net)?.success() {
                return Ok(());
            }
            // servers that only take passwords through keyboard-interactive
            let mut r = ssh.authenticate_keyboard_interactive_start(&user, None).await.map_err(net)?;
            for _ in 0..4 {
                match r {
                    KeyboardInteractiveAuthResponse::Success => return Ok(()),
                    KeyboardInteractiveAuthResponse::Failure { .. } => break,
                    KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                        let answers = prompts.iter().map(|_| site.pass.clone()).collect();
                        r = ssh.authenticate_keyboard_interactive_respond(answers).await.map_err(net)?;
                    }
                }
            }
        }
        Err(Error::Ftp(if site.key.is_empty() && site.pass.is_empty() {
            "Login refused: the server wants a password or a key".into()
        } else {
            "Login refused: wrong user name, password or key".into()
        }))
    }
}

impl Remote for Sftp {
    fn open(&mut self, site: &Site) -> Result<()> {
        self.close();
        let tcp = connect_to(&site.host, site.port, Duration::from_secs(5)).ok_or(Error::Net)?;
        let _ = tcp.set_nodelay(true);
        self.aborter.set(vec![&tcp]);
        tcp.set_nonblocking(true).map_err(|_| Error::Net)?;
        let seen: Seen = Default::default();
        let keys = Keys { trusted: site.trusted.clone(), seen: seen.clone() };
        let result = self.rt.block_on(async {
            let tcp = tokio::net::TcpStream::from_std(tcp).map_err(|_| Error::Net)?;
            let config = Arc::new(client::Config {
                inactivity_timeout: None,
                keepalive_interval: Some(Duration::from_secs(15)),
                ..Default::default()
            });
            let mut ssh = match tokio::time::timeout(Duration::from_secs(15), client::connect_stream(config, tcp, keys)).await {
                Ok(Ok(h)) => h,
                Ok(Err(russh::Error::UnknownKey)) => return Err(Error::Ftp("host key".into())),
                _ => return Err(Error::Net),
            };
            Sftp::login(&mut ssh, site).await?;
            let channel = ssh.channel_open_session().await.map_err(|_| Error::Net)?;
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|_| Error::Ftp("The server has no SFTP".into()))?;
            let sftp = SftpSession::new(channel.into_stream())
                .await
                .map_err(|_| Error::Ftp("The server has no SFTP".into()))?;
            sftp.set_timeout(30);
            Ok(Conn { ssh, sftp })
        });
        match result {
            Ok(c) => {
                self.conn = Some(c);
                Ok(())
            }
            Err(e) => {
                self.aborter.set(vec![]);
                match seen.lock().unwrap().take() {
                    Some((fingerprint, false)) => {
                        let detail = if site.trusted.is_empty() {
                            format!("First connection to {}: check that this is its host key", site.host)
                        } else {
                            format!("The host key of {} changed. It may have been reinstalled, or someone is in the way", site.host)
                        };
                        Err(Error::Untrusted { fingerprint, detail })
                    }
                    _ => Err(e),
                }
            }
        }
    }

    fn connected(&self) -> bool {
        self.conn.as_ref().is_some_and(|c| !c.ssh.is_closed())
    }

    fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = self.rt.block_on(async {
                let _ = tokio::time::timeout(Duration::from_secs(1), c.sftp.close()).await;
                let _ = c.ssh.disconnect(russh::Disconnect::ByApplication, "", "en").await;
            });
        }
        self.aborter.set(vec![]);
    }

    fn aborter(&self) -> Aborter {
        self.aborter.clone()
    }

    fn keepalive(&mut self) -> Result<()> {
        self.home().map(|_| ())
    }

    fn home(&mut self) -> Result<String> {
        let sftp = self.sftp()?;
        self.rt.block_on(sftp.canonicalize(".")).map_err(sftp_err)
    }

    fn list(&mut self, path: &str) -> Result<Vec<Entry>> {
        let sftp = self.sftp()?;
        let dir = self.rt.block_on(sftp.read_dir(path)).map_err(sftp_err)?;
        let mut out = Vec::new();
        for e in dir {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let mut m = e.metadata();
            if m.is_symlink() {
                // show what the link points to (a folder link opens like a folder)
                if let Ok(t) = self.rt.block_on(sftp.metadata(crate::util::join_remote(path, &name))) {
                    m = t;
                }
            }
            out.push(Entry {
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.size.unwrap_or(0) },
                modified: m.mtime.map(stamp).unwrap_or_default(),
                name,
            });
        }
        Ok(out)
    }

    fn size(&mut self, path: &str) -> Result<Option<u64>> {
        let sftp = self.sftp()?;
        match self.rt.block_on(sftp.metadata(path)) {
            Ok(m) => Ok(Some(m.size.unwrap_or(0))),
            Err(e) if missing(&e) => Ok(None),
            Err(e) => Err(sftp_err(e)),
        }
    }

    fn mtime(&mut self, path: &str) -> Result<Option<String>> {
        let sftp = self.sftp()?;
        match self.rt.block_on(sftp.metadata(path)) {
            Ok(m) => Ok(m.mtime.map(stamp)),
            Err(e) if missing(&e) => Ok(None),
            Err(e) => Err(sftp_err(e)),
        }
    }

    fn mkdir(&mut self, path: &str) -> Result<()> {
        let sftp = self.sftp()?;
        self.rt.block_on(sftp.create_dir(path)).map_err(sftp_err)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let sftp = self.sftp()?;
        self.rt.block_on(sftp.rename(from, to)).map_err(sftp_err)
    }

    fn delete(&mut self, path: &str, dir: bool) -> Result<()> {
        let sftp = self.sftp()?;
        self.rt.block_on(async { if dir { sftp.remove_dir(path).await } else { sftp.remove_file(path).await } }).map_err(sftp_err)
    }

    fn upload(&mut self, local: &str, remote: &str, offset: u64, progress: Progress) -> Result<()> {
        use std::io::{Read, Seek};
        let mut file = std::fs::File::open(local).map_err(|_| Error::Ftp(format!("Can't open local file {local}")))?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| Error::Ftp(format!("Can't read local file {local}")))?;
        let sftp = self.sftp()?;
        let flags = OpenFlags::WRITE | OpenFlags::CREATE | if offset == 0 { OpenFlags::TRUNCATE } else { OpenFlags::empty() };
        let r = self.rt.block_on(async {
            let mut out = sftp.open_with_flags(remote, flags).await.map_err(sftp_err)?;
            if offset != 0 {
                out.seek(SeekFrom::Start(offset)).await.map_err(|_| Error::Net)?;
            }
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = file.read(&mut buf).map_err(|_| Error::Ftp(format!("Can't read local file {local}")))?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).await.map_err(|_| Error::Net)?;
                progress(n as u64)?;
            }
            out.shutdown().await.map_err(|_| Error::Net)?;
            Ok(())
        });
        if r == Err(Error::Cancel) {
            let _ = self.rt.block_on(sftp.remove_file(remote)); // don't leave a truncated file behind
        }
        r
    }

    fn download(&mut self, remote: &str, local: &str, offset: u64, progress: Progress) -> Result<()> {
        use std::io::{Seek, Write};
        let cant = || Error::Ftp(format!("Can't create local file {local}"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(offset == 0)
            .open(local)
            .map_err(|_| cant())?;
        file.set_len(offset).map_err(|_| cant())?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| cant())?;
        let sftp = self.sftp()?;
        self.rt.block_on(async {
            let mut input = sftp.open_with_flags(remote, OpenFlags::READ).await.map_err(sftp_err)?;
            if offset != 0 {
                input.seek(SeekFrom::Start(offset)).await.map_err(|_| Error::Net)?;
            }
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = input.read(&mut buf).await.map_err(|_| Error::Net)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n]).map_err(|_| Error::Ftp(format!("Can't write local file {local}")))?;
                progress(n as u64)?;
            }
            Ok(())
        })
    }

    fn cement(&mut self) -> Option<&mut Ftp> {
        None
    }
}

impl Drop for Sftp {
    fn drop(&mut self) {
        self.close();
    }
}
