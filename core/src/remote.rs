// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! One connection API for every protocol the window speaks (FTP, FTPS, SFTP),
//! so the workers don't care which one a site uses. Cement's extras are only
//! reachable through `cement()`, which is None on any other server.

use crate::ftp::{Aborter, Entry, Error, Ftp, Login, Result, Tls};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    #[default]
    Ftp,
    /// AUTH TLS on the FTP port
    Ftps,
    /// TLS from the first byte, usually port 990
    FtpsImplicit,
    Sftp,
}

impl Protocol {
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Ftp => "ftp",
            Protocol::Ftps => "ftps",
            Protocol::FtpsImplicit => "ftps-implicit",
            Protocol::Sftp => "sftp",
        }
    }

    pub fn parse(s: &str) -> Option<Protocol> {
        Some(match s.to_ascii_lowercase().as_str() {
            "ftp" => Protocol::Ftp,
            "ftps" | "ftpes" => Protocol::Ftps,
            "ftps-implicit" => Protocol::FtpsImplicit,
            "sftp" | "ssh" => Protocol::Sftp,
            _ => return None,
        })
    }

    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Ftp | Protocol::Ftps => 21,
            Protocol::FtpsImplicit => 990,
            Protocol::Sftp => 22,
        }
    }
}

/// where and how to connect
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub user: String, // empty: anonymous (FTP)
    pub pass: String,
    pub key: String, // SFTP: private key file, tried before the password
    pub passphrase: String,
    /// certificate (FTPS) or host key (SFTP) fingerprints the user accepted
    pub trusted: Vec<String>,
    pub start_dir: String, // empty: the server's home folder
}

impl Default for Site {
    fn default() -> Site {
        Site {
            protocol: Protocol::Ftp,
            host: String::new(),
            port: 5000,
            user: String::new(),
            pass: String::new(),
            key: String::new(),
            passphrase: String::new(),
            trusted: Vec::new(),
            start_dir: String::new(),
        }
    }
}

impl Site {
    /// Cement at host:port (what baguette.ini held before sites)
    pub fn cement(host: &str, port: u16) -> Site {
        Site { host: host.into(), port, ..Default::default() }
    }

    fn login(&self) -> Login {
        let mut l = Login::default();
        if !self.user.is_empty() {
            l.user = self.user.clone();
            l.pass = self.pass.clone();
        }
        l.tls = match self.protocol {
            Protocol::Ftps => Tls::Explicit,
            Protocol::FtpsImplicit => Tls::Implicit,
            _ => Tls::None,
        };
        l.trusted = self.trusted.clone();
        l
    }
}

pub type Progress<'a> = &'a mut dyn FnMut(u64) -> Result<()>;

pub trait Remote: Send {
    /// Net: unreachable (try again later). Ftp: refused (wrong password...).
    /// Untrusted: the certificate or host key needs the user's OK.
    fn open(&mut self, site: &Site) -> Result<()>;
    fn connected(&self) -> bool;
    fn close(&mut self);
    /// for another thread: makes any blocking call fail at once
    fn aborter(&self) -> Aborter;
    /// keeps the connection open and notices when it went away
    fn keepalive(&mut self) -> Result<()>;
    /// the folder to show first
    fn home(&mut self) -> Result<String>;
    fn list(&mut self, path: &str) -> Result<Vec<Entry>>;
    /// None if the file does not exist
    fn size(&mut self, path: &str) -> Result<Option<u64>>;
    /// "YYYYMMDDhhmmss", None if missing or unknown
    fn mtime(&mut self, path: &str) -> Result<Option<String>>;
    fn mkdir(&mut self, path: &str) -> Result<()>;
    fn rename(&mut self, from: &str, to: &str) -> Result<()>;
    /// one file, or one empty folder
    fn delete(&mut self, path: &str, dir: bool) -> Result<()>;
    fn upload(&mut self, local: &str, remote: &str, offset: u64, progress: Progress) -> Result<()>;
    fn download(&mut self, remote: &str, local: &str, offset: u64, progress: Progress) -> Result<()>;
    /// the FTP connection, when the server is Cement (battery, SD space,
    /// batches, Send to PC)
    fn cement(&mut self) -> Option<&mut Ftp>;
}

impl Remote for Ftp {
    fn open(&mut self, site: &Site) -> Result<()> {
        Ftp::open(self, &site.host, site.port, site.login())?;
        let _ = self.server_info(); // tells Cement apart from other servers
        Ok(())
    }

    fn connected(&self) -> bool {
        Ftp::connected(self)
    }

    fn close(&mut self) {
        Ftp::close(self)
    }

    fn aborter(&self) -> Aborter {
        Ftp::aborter(self)
    }

    fn keepalive(&mut self) -> Result<()> {
        match self.cmd("NOOP", None)? {
            c if c / 100 == 2 => Ok(()),
            _ => Err(Error::Net),
        }
    }

    fn home(&mut self) -> Result<String> {
        // 257 "/home/pi" is the current directory
        let (code, text) = self.cmd_text("PWD")?;
        let quoted = text.split('"').nth(1).filter(|p| !p.is_empty());
        Ok(match quoted {
            Some(p) if code == 257 => p.replace("\"\"", "\""),
            _ => "/".into(),
        })
    }

    fn list(&mut self, path: &str) -> Result<Vec<Entry>> {
        Ftp::list(self, path)
    }

    fn size(&mut self, path: &str) -> Result<Option<u64>> {
        Ftp::size(self, path)
    }

    fn mtime(&mut self, path: &str) -> Result<Option<String>> {
        self.mdtm(path)
    }

    fn mkdir(&mut self, path: &str) -> Result<()> {
        self.simple(&format!("MKD {path}"), 257)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        self.simple(&format!("RNFR {from}"), 350)?;
        self.simple(&format!("RNTO {to}"), 250)
    }

    fn delete(&mut self, path: &str, dir: bool) -> Result<()> {
        self.simple(&format!("{} {path}", if dir { "RMD" } else { "DELE" }), 250)
    }

    fn upload(&mut self, local: &str, remote: &str, offset: u64, progress: Progress) -> Result<()> {
        Ftp::upload(self, local, remote, offset, progress)
    }

    fn download(&mut self, remote: &str, local: &str, offset: u64, progress: Progress) -> Result<()> {
        Ftp::download(self, remote, local, offset, progress)
    }

    fn cement(&mut self) -> Option<&mut Ftp> {
        if self.is_cement() { Some(self) } else { None }
    }
}

/// a new, unconnected connection for this site's protocol
pub fn for_site(site: &Site) -> Box<dyn Remote> {
    match site.protocol {
        #[cfg(feature = "sftp")]
        Protocol::Sftp => Box::new(crate::sftp::Sftp::new()),
        #[cfg(not(feature = "sftp"))]
        Protocol::Sftp => Box::new(Unsupported),
        _ => Box::new(Ftp::new()),
    }
}

#[cfg(not(feature = "sftp"))]
struct Unsupported;

#[cfg(not(feature = "sftp"))]
impl Remote for Unsupported {
    fn open(&mut self, _: &Site) -> Result<()> {
        Err(Error::Ftp("SFTP isn't available in this build".into()))
    }
    fn connected(&self) -> bool {
        false
    }
    fn close(&mut self) {}
    fn aborter(&self) -> Aborter {
        Aborter::default()
    }
    fn keepalive(&mut self) -> Result<()> {
        Err(Error::Net)
    }
    fn home(&mut self) -> Result<String> {
        Err(Error::Net)
    }
    fn list(&mut self, _: &str) -> Result<Vec<Entry>> {
        Err(Error::Net)
    }
    fn size(&mut self, _: &str) -> Result<Option<u64>> {
        Err(Error::Net)
    }
    fn mtime(&mut self, _: &str) -> Result<Option<String>> {
        Err(Error::Net)
    }
    fn mkdir(&mut self, _: &str) -> Result<()> {
        Err(Error::Net)
    }
    fn rename(&mut self, _: &str, _: &str) -> Result<()> {
        Err(Error::Net)
    }
    fn delete(&mut self, _: &str, _: bool) -> Result<()> {
        Err(Error::Net)
    }
    fn upload(&mut self, _: &str, _: &str, _: u64, _: Progress) -> Result<()> {
        Err(Error::Net)
    }
    fn download(&mut self, _: &str, _: &str, _: u64, _: Progress) -> Result<()> {
        Err(Error::Net)
    }
    fn cement(&mut self) -> Option<&mut Ftp> {
        None
    }
}
