// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The FTP client: passive mode, binary, UTF-8, resumable transfers, plus the
//! extras Cement understands (SITE STATUS, SITE BATCH, ALLO, AVBL).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::windows::io::AsRawSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
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

    fn set(&self, socks: Vec<&TcpStream>) {
        *self.0.lock().unwrap() = socks.into_iter().filter_map(|s| s.try_clone().ok()).collect();
    }
}

#[derive(Default)]
pub struct Ftp {
    ctrl: Option<TcpStream>,
    data: Option<TcpStream>,
    host: String,
    rbuf: Vec<u8>,
    aborter: Aborter,
}

impl Drop for Ftp {
    fn drop(&mut self) {
        self.close();
    }
}

fn connect_to(host: &str, port: u16, timeout: Duration) -> Option<TcpStream> {
    let addr: SocketAddr = (host, port).to_socket_addrs().ok()?.find(|a| a.is_ipv4())?;
    TcpStream::connect_timeout(&addr, timeout).ok()
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

    pub fn connect(&mut self, host: &str, port: u16) -> bool {
        self.close();
        let Some(s) = connect_to(host, port, Duration::from_secs(3)) else { return false };
        let _ = s.set_nodelay(true);
        ack_every_segment(&s);
        let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = s.set_write_timeout(Some(Duration::from_secs(10)));
        self.aborter.set(vec![&s]);
        self.ctrl = Some(s);
        self.host = host.to_string();
        self.rbuf.clear();
        let login = (|| -> Result<()> {
            if self.reply(None)? != 220 {
                return Err(Error::Net);
            }
            let mut code = self.cmd("USER anonymous", None)?;
            if code == 331 {
                code = self.cmd("PASS chomp", None)?;
            }
            if code != 230 {
                return Err(Error::Net);
            }
            self.cmd("TYPE I", None)?;
            self.cmd("OPTS UTF8 ON", None)?;
            Ok(())
        })();
        if login.is_err() {
            self.close();
            return false;
        }
        true
    }

    pub fn close(&mut self) {
        self.data = None;
        self.ctrl = None;
        self.aborter.set(vec![]);
    }

    fn sync_aborter(&self) {
        self.aborter.set(self.ctrl.iter().chain(self.data.iter()).collect());
    }

    /// sends a command; returns the reply code (and the whole reply text)
    pub fn cmd(&mut self, line: &str, text: Option<&mut String>) -> Result<i32> {
        let ctrl = self.ctrl.as_mut().ok_or(Error::Net)?;
        ctrl.write_all(format!("{line}\r\n").as_bytes())?;
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

    fn open_data(&mut self) -> Result<()> {
        self.close_data();
        let (code, text) = self.cmd_text("PASV")?;
        if code != 227 {
            return Err(Error::Ftp(text));
        }
        // "227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)": use the control host
        let nums: Vec<u32> = text
            .find('(')
            .map(|o| text[o + 1..].split([',', ')']).take(6).map(|v| parse_u64(v) as u32).collect())
            .unwrap_or_default();
        if nums.len() != 6 {
            return Err(Error::Ftp(text));
        }
        let port = (nums[4] * 256 + nums[5]) as u16;
        let s = connect_to(&self.host.clone(), port, Duration::from_secs(5)).ok_or(Error::Net)?;
        let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
        self.data = Some(s);
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
            reset(s);
            self.sync_aborter();
        }
    }

    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>> {
        self.open_data()?;
        let (code, text) = self.cmd_text(&format!("MLSD {path}"))?;
        if code != 150 && code != 125 {
            self.close_data();
            return Err(Error::Ftp(text));
        }
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
        let (code, text) = {
            let mut t = String::new();
            let c = self.reply(Some(&mut t))?;
            (c, t)
        };
        if code / 100 != 2 {
            return Err(Error::Ftp(text));
        }

        let raw = String::from_utf8_lossy(&raw);
        let mut out = Vec::new();
        for line in raw.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let Some(sp) = line.find("; ") else { continue };
            let (facts, name) = (&line[..=sp], &line[sp + 2..]);
            let mut e = Entry { name: name.to_string(), dir: false, size: 0 };
            let mut skip = false;
            for fact in facts.split(';') {
                let Some((k, v)) = fact.split_once('=') else { continue };
                let (k, v) = (k.to_ascii_lowercase(), v.to_ascii_lowercase());
                if k == "type" {
                    e.dir = v == "dir";
                    skip = v == "cdir" || v == "pdir";
                } else if k == "size" {
                    e.size = parse_u64(&v);
                }
            }
            if !skip && !e.name.is_empty() {
                out.push(e);
            }
        }
        Ok(out)
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
        Ok(parse_server_info(code, &text))
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
        if let Some(d) = &self.data {
            let _ = d.shutdown(Shutdown::Write);
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
