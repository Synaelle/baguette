// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The FTP client: passive mode, binary, UTF-8, resumable transfers, FTPS
//! (with the `tls` feature), plus the extras Cement understands (SITE STATUS,
//! SITE BATCH, ALLO, AVBL).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::windows::io::AsRawSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::listing::{parse_list, parse_mlsd};
use crate::status::{ServerInfo, parse_server_info};
use crate::util::parse_u64;

/// why an FTP call failed
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// the connection dropped: reconnect and try again
    Net,
    /// the user cancelled (from the progress callback)
    Cancel,
    /// the server or the local system said no; the text says why
    Ftp(String),
    /// the server's certificate or host key isn't trusted yet: ask the user, then
    /// connect again with this fingerprint trusted
    Untrusted { fingerprint: String, detail: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Error {
        Error::Net
    }
}

/// first line of a server reply or error text
pub fn first_line(s: &str) -> &str {
    s.split(['\r', '\n']).next().unwrap_or("")
}

/// a remote folder entry
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: String, // "YYYYMMDDhhmmss", empty if unknown
}

/// how a connection is protected
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Tls {
    #[default]
    None,
    /// AUTH TLS on the usual port, then PROT P for the data connections
    Explicit,
    /// TLS from the first byte (port 990)
    Implicit,
}

/// how a connection logs in
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub user: String,
    pub pass: String,
    pub tls: Tls,
    /// certificate fingerprints the user accepted for this site
    pub trusted: Vec<String>,
}

impl Default for Login {
    /// what Cement expects: anonymous, plain FTP
    fn default() -> Login {
        Login { user: "anonymous".into(), pass: "chomp".into(), tls: Tls::None, trusted: Vec::new() }
    }
}

/// stops every blocking call of an Ftp from another thread
#[derive(Clone, Default)]
pub struct Aborter(Arc<Mutex<Vec<TcpStream>>>);

impl Aborter {
    pub fn abort(&self) {
        for s in self.0.lock().unwrap().iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    pub(crate) fn set(&self, socks: Vec<&TcpStream>) {
        *self.0.lock().unwrap() = socks.into_iter().filter_map(|s| s.try_clone().ok()).collect();
    }
}

/// a control or data connection, plain or TLS
enum Conn {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<crate::tls::Stream>),
}

impl Conn {
    fn tcp(&self) -> &TcpStream {
        match self {
            Conn::Plain(s) => s,
            #[cfg(feature = "tls")]
            Conn::Tls(s) => &s.sock,
        }
    }

    fn into_tcp(self) -> TcpStream {
        match self {
            Conn::Plain(s) => s,
            #[cfg(feature = "tls")]
            Conn::Tls(s) => s.sock,
        }
    }

    /// end of an upload: TLS says goodbye first, then the write side closes
    fn finish(&mut self) {
        #[cfg(feature = "tls")]
        if let Conn::Tls(s) = self {
            crate::tls::finish(s);
        }
        let _ = self.tcp().shutdown(Shutdown::Write);
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            #[cfg(feature = "tls")]
            Conn::Tls(s) => crate::tls::read(s, buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            #[cfg(feature = "tls")]
            Conn::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            #[cfg(feature = "tls")]
            Conn::Tls(s) => s.flush(),
        }
    }
}

#[derive(Default)]
pub struct Ftp {
    ctrl: Option<Conn>,
    data: Option<Conn>,
    host: String,
    rbuf: Vec<u8>,
    aborter: Aborter,
    login: Login,
    #[cfg(feature = "tls")]
    tls: Option<Arc<rustls::ClientConfig>>,
    prot_p: bool,    // the data connections use TLS too
    no_pasv: bool,   // PASV refused: EPSV only
    no_mlsd: bool,   // MLSD refused: LIST
    is_cement: bool, // SITE STATUS answered like the 3DS app
}

impl Drop for Ftp {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn connect_to(host: &str, port: u16, timeout: Duration) -> Option<TcpStream> {
    let addrs: Vec<SocketAddr> = (host, port).to_socket_addrs().ok()?.collect();
    // IPv4 first: Cement and most home servers only listen there
    let addr = addrs.iter().find(|a| a.is_ipv4()).or(addrs.first())?;
    TcpStream::connect_timeout(addr, timeout).ok()
}

/// acknowledge every reply at once: while a file goes over the data connection
/// nothing else is sent on this one, and a delayed ACK of "150" made the server's
/// stack hold "226" back (~200 ms per file)
fn ack_every_segment(s: &TcpStream) {
    use windows_sys::Win32::Networking::WinSock::{SIO_TCP_SET_ACK_FREQUENCY, WSAIoctl};
    let every: u32 = 1;
    let mut unused: u32 = 0;
    unsafe {
        WSAIoctl(
            s.as_raw_socket() as usize,
            SIO_TCP_SET_ACK_FREQUENCY,
            (&raw const every).cast(),
            4,
            std::ptr::null_mut(),
            0,
            &mut unused,
            std::ptr::null_mut(),
            None,
        );
    }
}

/// close with a reset, so the server sees the transfer as broken
fn reset(s: TcpStream) {
    use windows_sys::Win32::Networking::WinSock::{LINGER, SO_LINGER, SOL_SOCKET, setsockopt};
    let l = LINGER { l_onoff: 1, l_linger: 0 };
    unsafe {
        setsockopt(s.as_raw_socket() as usize, SOL_SOCKET, SO_LINGER, (&raw const l).cast(), size_of::<LINGER>() as i32);
    }
}

impl Ftp {
    pub fn new() -> Ftp {
        Ftp::default()
    }

    /// for another thread: makes any blocking call of this connection fail at once
    pub fn aborter(&self) -> Aborter {
        self.aborter.clone()
    }

    pub fn connected(&self) -> bool {
        self.ctrl.is_some()
    }

    /// Cement: anonymous, plain FTP
    pub fn connect(&mut self, host: &str, port: u16) -> bool {
        self.open(host, port, Login::default()).is_ok()
    }

    /// Net: unreachable (try again later). Ftp: the server refused the login.
    /// Untrusted: the certificate needs the user's OK.
    pub fn open(&mut self, host: &str, port: u16, login: Login) -> Result<()> {
        self.close();
        let Some(s) = connect_to(host, port, Duration::from_secs(3)) else { return Err(Error::Net) };
        let _ = s.set_nodelay(true);
        ack_every_segment(&s);
        let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = s.set_write_timeout(Some(Duration::from_secs(10)));
        self.aborter.set(vec![&s]);
        self.host = host.to_string();
        self.rbuf.clear();
        self.login = login;
        #[cfg(feature = "tls")]
        {
            self.tls = None;
        }
        self.prot_p = false;
        self.no_pasv = false;
        self.no_mlsd = false;
        self.is_cement = false;
        let r = self.handshake(s);
        if r.is_err() {
            self.close();
        }
        r
    }

    fn handshake(&mut self, s: TcpStream) -> Result<()> {
        let tls = self.login.tls.clone();
        self.ctrl = Some(if tls == Tls::Implicit { self.tls_wrap(s)? } else { Conn::Plain(s) });
        self.sync_aborter();
        if self.reply(None)? != 220 {
            return Err(Error::Net);
        }
        if tls == Tls::Explicit {
            let (code, text) = self.cmd_text("AUTH TLS")?;
            if code != 234 {
                return Err(Error::Ftp(format!("The server doesn't offer FTPS: {}", first_line(&text))));
            }
            let plain = self.ctrl.take().ok_or(Error::Net)?.into_tcp();
            self.ctrl = Some(self.tls_wrap(plain)?);
            self.sync_aborter();
        }
        let (user, pass) = (self.login.user.clone(), self.login.pass.clone());
        let (mut code, mut text) = self.cmd_text(&format!("USER {user}"))?;
        if code == 331 {
            (code, text) = self.cmd_text(&format!("PASS {pass}"))?;
        }
        if code != 230 {
            return Err(Error::Ftp(format!("Login refused: {}", first_line(&text))));
        }
        if tls != Tls::None {
            self.cmd("PBSZ 0", None)?;
            self.prot_p = self.cmd("PROT P", None)? / 100 == 2;
        }
        self.cmd("TYPE I", None)?;
        self.cmd("OPTS UTF8 ON", None)?;
        Ok(())
    }

    /// TLS over a new connection; the data connections reuse the control
    /// connection's config, so their TLS session resumes
    #[cfg(feature = "tls")]
    fn tls_wrap(&mut self, s: TcpStream) -> Result<Conn> {
        let refused: crate::tls::Refused = Default::default();
        let cfg = match &self.tls {
            Some(c) => c.clone(),
            None => {
                let c = crate::tls::config(self.login.trusted.clone(), refused.clone())
                    .map_err(|e| Error::Ftp(format!("TLS: {e}")))?;
                self.tls = Some(c.clone());
                c
            }
        };
        let host = self.host.clone();
        match crate::tls::wrap(&cfg, &host, s) {
            Ok(t) => Ok(Conn::Tls(Box::new(t))),
            Err(e) => match refused.lock().unwrap().take() {
                Some((fingerprint, detail)) => Err(Error::Untrusted { fingerprint, detail }),
                None if e.kind() == std::io::ErrorKind::InvalidData => Err(Error::Ftp(format!("TLS: {e}"))),
                None => Err(Error::Net),
            },
        }
    }

    #[cfg(not(feature = "tls"))]
    fn tls_wrap(&mut self, _: TcpStream) -> Result<Conn> {
        Err(Error::Ftp("FTPS isn't available in this build".into()))
    }

    /// the last SITE STATUS was answered like Cement does
    pub fn is_cement(&self) -> bool {
        self.is_cement
    }

    pub fn close(&mut self) {
        self.data = None;
        self.ctrl = None;
        self.aborter.set(vec![]);
    }

    fn sync_aborter(&self) {
        self.aborter.set(self.ctrl.iter().chain(self.data.iter()).map(Conn::tcp).collect());
    }

    /// sends a command; returns the reply code (and the whole reply text)
    pub fn cmd(&mut self, line: &str, text: Option<&mut String>) -> Result<i32> {
        let ctrl = self.ctrl.as_mut().ok_or(Error::Net)?;
        ctrl.write_all(format!("{line}\r\n").as_bytes())?;
        ctrl.flush()?;
        self.reply(text)
    }

    pub fn cmd_text(&mut self, line: &str) -> Result<(i32, String)> {
        let mut t = String::new();
        let c = self.cmd(line, Some(&mut t))?;
        Ok((c, t))
    }

    fn read_line(&mut self) -> Result<String> {
        loop {
            if let Some(nl) = self.rbuf.iter().position(|&c| c == b'\n') {
                let mut line: Vec<u8> = self.rbuf.drain(..=nl).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            let ctrl = self.ctrl.as_mut().ok_or(Error::Net)?;
            let mut buf = [0u8; 4096];
            match ctrl.read(&mut buf) {
                Ok(0) | Err(_) => return Err(Error::Net),
                Ok(n) => self.rbuf.extend_from_slice(&buf[..n]),
            }
        }
    }

    /// reads one (possibly multi-line) reply
    pub fn reply(&mut self, text: Option<&mut String>) -> Result<i32> {
        let mut all = String::new();
        loop {
            let line = self.read_line()?;
            all.push_str(&line);
            all.push('\n');
            let b = line.as_bytes();
            if b.len() >= 4 && b[..3].iter().all(u8::is_ascii_digit) && b[3] == b' ' {
                if let Some(t) = text {
                    *t = all;
                }
                return Ok(line[..3].parse().unwrap_or(0));
            }
        }
    }

    /// the command must succeed with the same first digit as `expect`
    pub fn simple(&mut self, line: &str, expect: i32) -> Result<()> {
        let (code, text) = self.cmd_text(line)?;
        if code / 100 != expect / 100 {
            return Err(Error::Ftp(text));
        }
        Ok(())
    }

    /// the data port from PASV, or EPSV when PASV is refused; the address is
    /// always the control connection's host, which also works behind NAT
    fn data_port(&mut self) -> Result<u16> {
        if !self.no_pasv {
            let (code, text) = self.cmd_text("PASV")?;
            if code == 227 {
                // "227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)"
                let nums: Vec<u32> = text
                    .find('(')
                    .map(|o| text[o + 1..].split([',', ')']).take(6).map(|v| parse_u64(v) as u32).collect())
                    .unwrap_or_default();
                if nums.len() != 6 {
                    return Err(Error::Ftp(text));
                }
                return Ok((nums[4] * 256 + nums[5]) as u16);
            }
            self.no_pasv = true;
        }
        // "229 Entering Extended Passive Mode (|||port|)"
        let (code, text) = self.cmd_text("EPSV")?;
        let port = text.split('|').nth(3).map(parse_u64).unwrap_or(0);
        if code != 229 || port == 0 || port > 65535 {
            return Err(Error::Ftp(text));
        }
        Ok(port as u16)
    }

    fn open_data(&mut self) -> Result<()> {
        self.close_data();
        let port = self.data_port()?;
        let s = connect_to(&self.host.clone(), port, Duration::from_secs(5)).ok_or(Error::Net)?;
        let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
        self.data = Some(Conn::Plain(s));
        self.sync_aborter();
        Ok(())
    }

    /// once the server accepted the transfer command: TLS on the data connection
    fn secure_data(&mut self) -> Result<()> {
        if !self.prot_p {
            return Ok(());
        }
        let plain = self.data.take().ok_or(Error::Net)?.into_tcp();
        let c = self.tls_wrap(plain)?;
        self.data = Some(c);
        self.sync_aborter();
        Ok(())
    }

    fn close_data(&mut self) {
        if self.data.take().is_some() {
            self.sync_aborter();
        }
    }

    fn reset_data(&mut self) {
        if let Some(s) = self.data.take() {
            reset(s.into_tcp());
            self.sync_aborter();
        }
    }

    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>> {
        if !self.no_mlsd {
            match self.list_with("MLSD", path) {
                Ok(raw) => return Ok(parse_mlsd(&raw)),
                Err(Error::Ftp(t)) if t.starts_with("500") || t.starts_with("502") => self.no_mlsd = true,
                Err(e) => return Err(e),
            }
        }
        let raw = self.list_with("LIST", path)?;
        let (y, m) = crate::util::this_year_month();
        Ok(parse_list(&raw, y, m))
    }

    fn list_with(&mut self, verb: &str, path: &str) -> Result<String> {
        self.open_data()?;
        let (code, text) = self.cmd_text(&format!("{verb} {path}"))?;
        if code != 150 && code != 125 {
            self.close_data();
            return Err(Error::Ftp(text));
        }
        self.secure_data()?;
        let mut raw = Vec::new();
        {
            let data = self.data.as_mut().ok_or(Error::Net)?;
            let mut buf = vec![0u8; 65536];
            loop {
                match data.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => raw.extend_from_slice(&buf[..n]),
                    Err(_) => return Err(Error::Net),
                }
            }
        }
        self.close_data();
        let mut text = String::new();
        if self.reply(Some(&mut text))? / 100 != 2 {
            return Err(Error::Ftp(text));
        }
        Ok(String::from_utf8_lossy(&raw).into_owned())
    }

    /// None if the remote file does not exist
    pub fn size(&mut self, path: &str) -> Result<Option<u64>> {
        let (code, text) = self.cmd_text(&format!("SIZE {path}"))?;
        Ok((code == 213).then(|| parse_u64(text.get(4..).unwrap_or(""))))
    }

    /// last modification time "YYYYMMDDhhmmss"; None if missing or unsupported.
    /// The 3DS reports its own clock, which is local time.
    pub fn mdtm(&mut self, path: &str) -> Result<Option<String>> {
        let (code, text) = self.cmd_text(&format!("MDTM {path}"))?;
        Ok((code == 213 && text.len() >= 18).then(|| text[4..18].to_string()))
    }

    pub fn server_info(&mut self) -> Result<ServerInfo> {
        let (code, text) = self.cmd_text("SITE STATUS")?;
        let info = parse_server_info(code, &text);
        self.is_cement = info.cement;
        Ok(info)
    }

    /// the accent colour of Cement's screens ("b8283c"), protocol 2
    pub fn set_accent(&mut self, hex: &str) -> Result<()> {
        self.simple(&format!("SITE COLOR #{}", hex.trim_start_matches('#')), 200)
    }

    /// starts (or queues) installing a CIA already on the SD card, protocol 2;
    /// Ok(the server's answer)
    pub fn install(&mut self, path: &str) -> Result<String> {
        let (code, text) = self.cmd_text(&format!("SITE INSTALL {path}"))?;
        if code / 100 != 2 {
            return Err(Error::Ftp(text));
        }
        Ok(first_line(&text).get(4..).unwrap_or("").to_string())
    }

    /// free bytes on the SD card (AVBL); None if the server doesn't say
    pub fn free_bytes(&mut self) -> Result<Option<u64>> {
        let (code, text) = self.cmd_text("AVBL")?;
        Ok((code == 213).then(|| parse_u64(text.get(4..).unwrap_or(""))))
    }

    /// Do these files (remote path, size) fit on the SD card? False only when they
    /// surely don't: the bytes to write, minus the files they would overwrite, are
    /// more than the free space. True when the server doesn't report free space.
    /// The sizes of existing files are only asked for when the total doesn't fit.
    /// Returns (fits, needed, available).
    pub fn fits(&mut self, files: &[(String, u64)]) -> Result<(bool, u64, u64)> {
        let mut need: u64 = files.iter().map(|f| f.1).sum();
        let Some(avail) = self.free_bytes()? else { return Ok((true, need, 0)) };
        if need <= avail {
            return Ok((true, need, avail));
        }
        for (path, _) in files {
            if let Some(old) = self.size(path)? {
                need -= old.min(need);
            }
            if need <= avail {
                return Ok((true, need, avail));
            }
        }
        Ok((false, need, avail))
    }

    /// tells the 3DS a multi-file job is running (SITE STATUS shows it as busy with an
    /// ETA, even between files); files == 0 ends it. Ignored by other servers.
    pub fn announce_batch(&mut self, files: u64, bytes: u64, kind: &str) -> Result<()> {
        self.cmd(&format!("SITE BATCH {files} {bytes} {kind}"), None).map(|_| ())
    }

    /// ends the batch, when the connection is still there
    pub fn end_batch(&mut self) {
        if self.connected() {
            let _ = self.announce_batch(0, 0, "up");
        }
    }

    /// progress(bytes) is called for every chunk; it returns Err(Cancel) to stop
    pub fn upload(
        &mut self,
        local: &str,
        remote: &str,
        offset: u64,
        progress: &mut dyn FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        let mut file = File::open(local).map_err(|_| Error::Ftp(format!("Can't open local file {local}")))?;
        let total = file.metadata().map(|m| m.len()).unwrap_or(0);
        file.seek(SeekFrom::Start(offset)).map_err(|_| Error::Ftp(format!("Can't read local file {local}")))?;

        self.open_data()?;
        // announce the full size first so the 3DS can report progress and time left
        // (before REST: every other command resets the restart offset)
        self.cmd(&format!("ALLO {total}"), None)?;
        if offset != 0 {
            self.simple(&format!("REST {offset}"), 350)?;
        }
        let (code, text) = self.cmd_text(&format!("STOR {remote}"))?;
        if code != 150 && code != 125 {
            self.close_data();
            return Err(Error::Ftp(text));
        }
        self.secure_data()?;

        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = file.read(&mut buf).map_err(|_| Error::Ftp(format!("Can't read local file {local}")))?;
            if n == 0 {
                break;
            }
            self.data.as_mut().ok_or(Error::Net)?.write_all(&buf[..n])?;
            if let Err(e) = progress(n as u64) {
                if e == Error::Cancel {
                    self.reset_data();
                    self.reply(None)?;
                    self.cmd(&format!("DELE {remote}"), None)?; // don't leave a truncated file behind
                }
                return Err(e);
            }
        }
        if let Some(d) = &mut self.data {
            d.finish();
        }
        let mut text = String::new();
        let code = self.reply(Some(&mut text))?;
        self.close_data();
        if code / 100 != 2 {
            return Err(Error::Ftp(text));
        }
        Ok(())
    }

    pub fn download(
        &mut self,
        remote: &str,
        local: &str,
        offset: u64,
        progress: &mut dyn FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        let cant = || Error::Ftp(format!("Can't create local file {local}"));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(offset == 0)
            .open(local)
            .map_err(|_| cant())?;
        file.set_len(offset).map_err(|_| cant())?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| cant())?;

        self.open_data()?;
        if offset != 0 {
            self.simple(&format!("REST {offset}"), 350)?;
        }
        let (code, text) = self.cmd_text(&format!("RETR {remote}"))?;
        if code != 150 && code != 125 {
            self.close_data();
            return Err(Error::Ftp(text));
        }
        self.secure_data()?;

        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = match self.data.as_mut().ok_or(Error::Net)?.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => return Err(Error::Net),
            };
            file.write_all(&buf[..n]).map_err(|_| Error::Ftp(format!("Can't write local file {local}")))?;
            if let Err(e) = progress(n as u64) {
                if e == Error::Cancel {
                    self.reset_data();
                    self.reply(None)?;
                }
                return Err(e);
            }
        }
        self.close_data();
        let mut text = String::new();
        if self.reply(Some(&mut text))? / 100 != 2 {
            return Err(Error::Ftp(text));
        }
        Ok(())
    }
}
