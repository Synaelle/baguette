// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! A small FTP server on 127.0.0.1 serving a folder, for the tests. It plays
//! servers unlike Cement: a real login, LIST without MLSD, EPSV without PASV,
//! and FTPS (explicit or implicit) with a self-signed certificate.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

#[derive(Clone, Default)]
pub struct Options {
    pub mlsd: bool,
    pub pasv: bool,
    pub tls: bool,      // AUTH TLS offered
    pub implicit: bool, // TLS from the first byte
}

pub struct Ftpd {
    pub port: u16,
    /// DER of the certificate (for its fingerprint)
    pub cert: Vec<u8>,
}

enum S {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ServerConnection, TcpStream>>),
}

impl Read for S {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            S::Plain(s) => s.read(b),
            S::Tls(s) => s.read(b),
        }
    }
}

impl Write for S {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            S::Plain(s) => s.write(b),
            S::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            S::Plain(s) => s.flush(),
            S::Tls(s) => s.flush(),
        }
    }
}

impl S {
    fn into_tcp(self) -> TcpStream {
        match self {
            S::Plain(s) => s,
            S::Tls(s) => s.sock,
        }
    }
    fn close(self) {
        if let S::Tls(mut s) = self {
            s.conn.send_close_notify();
            let _ = s.flush();
        }
    }
}

fn tls(cfg: &Arc<ServerConfig>, tcp: TcpStream) -> Option<S> {
    let conn = ServerConnection::new(cfg.clone()).ok()?;
    let mut s = StreamOwned::new(conn, tcp);
    while s.conn.is_handshaking() {
        s.conn.complete_io(&mut s.sock).ok()?;
    }
    Some(S::Tls(Box::new(s)))
}

pub fn start(root: &Path, opt: Options) -> Ftpd {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
    let der = cert.cert.der().to_vec();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()));
    let cfg = Arc::new(
        ServerConfig::builder().with_no_client_auth().with_single_cert(vec![CertificateDer::from(der.clone())], key).unwrap(),
    );
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let root = root.to_path_buf();
    thread::spawn(move || {
        for c in l.incoming().flatten() {
            let (root, opt, cfg) = (root.clone(), opt.clone(), cfg.clone());
            thread::spawn(move || session(c, &root, &opt, &cfg));
        }
    });
    Ftpd { port, cert: der }
}

fn path_of(root: &Path, cwd: &str, arg: &str) -> PathBuf {
    let full = if arg.starts_with('/') { arg.to_string() } else { format!("{cwd}/{arg}") };
    let mut p = root.to_path_buf();
    for part in full.split('/').filter(|s| !s.is_empty() && *s != ".") {
        if part == ".." {
            if p != root {
                p.pop();
            }
        } else {
            p.push(part);
        }
    }
    p
}

fn session(c: TcpStream, root: &Path, opt: &Options, cfg: &Arc<ServerConfig>) {
    let ctrl = if opt.implicit {
        match tls(cfg, c) {
            Some(s) => s,
            None => return,
        }
    } else {
        S::Plain(c)
    };
    let mut r = BufReader::new(ctrl);
    let say = |r: &mut BufReader<S>, l: &str| {
        let _ = r.get_mut().write_all(format!("{l}\r\n").as_bytes());
        let _ = r.get_mut().flush();
    };
    say(&mut r, "220 test server");
    let (mut user, mut logged, mut prot, mut rest, mut from) = (String::new(), false, false, 0u64, PathBuf::new());
    let mut pasv: Option<TcpListener> = None;
    let cwd = "/home/pi".to_string();
    std::fs::create_dir_all(path_of(root, "/", &cwd)).unwrap();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end();
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let verb = verb.to_ascii_uppercase();
        if !logged && !["USER", "PASS", "AUTH", "QUIT", "PBSZ", "PROT"].contains(&verb.as_str()) {
            say(&mut r, "530 Please log in");
            continue;
        }
        // data connection for the next transfer command
        let data = |pasv: &mut Option<TcpListener>, prot: bool| -> Option<S> {
            let l = pasv.take()?;
            let (s, _) = l.accept().ok()?;
            if prot { tls(cfg, s) } else { Some(S::Plain(s)) }
        };
        match verb.as_str() {
            "USER" => {
                user = arg.to_string();
                say(&mut r, "331 Password");
            }
            "PASS" => {
                if user == "pi" && arg == "secret" {
                    logged = true;
                    say(&mut r, "230 Logged in");
                } else {
                    say(&mut r, "530 Login incorrect");
                }
            }
            "AUTH" if opt.tls && !opt.implicit => {
                say(&mut r, "234 Go");
                let plain = r.into_inner().into_tcp();
                match tls(cfg, plain) {
                    Some(s) => r = BufReader::new(s),
                    None => return,
                }
            }
            "PBSZ" => say(&mut r, "200 PBSZ=0"),
            "PROT" => {
                prot = arg.eq_ignore_ascii_case("P");
                say(&mut r, "200 Protection set");
            }
            "TYPE" | "OPTS" | "NOOP" => say(&mut r, "200 OK"),
            "ALLO" => say(&mut r, "202 Not needed"),
            "PWD" => say(&mut r, &format!("257 \"{cwd}\" is the current directory")),
            "PASV" if opt.pasv => {
                let l = TcpListener::bind("127.0.0.1:0").unwrap();
                let p = l.local_addr().unwrap().port();
                pasv = Some(l);
                say(&mut r, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", p / 256, p % 256));
            }
            "EPSV" => {
                let l = TcpListener::bind("127.0.0.1:0").unwrap();
                let p = l.local_addr().unwrap().port();
                pasv = Some(l);
                say(&mut r, &format!("229 Entering Extended Passive Mode (|||{p}|)"));
            }
            "MLSD" if opt.mlsd => {
                let p = path_of(root, &cwd, arg);
                say(&mut r, "150 Here it comes");
                let Some(mut d) = data(&mut pasv, prot) else { return };
                for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                    let m = e.metadata().unwrap();
                    let t = if m.is_dir() { "dir".to_string() } else { format!("file;size={}", m.len()) };
                    let _ = write!(d, "type={t};modify=20260101120000; {}\r\n", e.file_name().to_string_lossy());
                }
                d.close();
                say(&mut r, "226 Done");
            }
            "LIST" => {
                let p = path_of(root, &cwd, arg.trim_start_matches("-a").trim());
                say(&mut r, "150 Here it comes");
                let Some(mut d) = data(&mut pasv, prot) else { return };
                let _ = write!(d, "total 8\r\n");
                for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                    let m = e.metadata().unwrap();
                    let kind = if m.is_dir() { 'd' } else { '-' };
                    let _ =
                        write!(d, "{kind}rw-r--r--    1 pi   pi   {:>10} Mar  3 09:15 {}\r\n", m.len(), e.file_name().to_string_lossy());
                }
                d.close();
                say(&mut r, "226 Done");
            }
            "REST" => {
                rest = arg.parse().unwrap_or(0);
                say(&mut r, "350 Restarting");
            }
            "STOR" => {
                let p = path_of(root, &cwd, arg);
                let mut f = OpenOptions::new().write(true).create(true).truncate(rest == 0).open(&p).unwrap();
                f.seek(SeekFrom::Start(rest)).unwrap();
                rest = 0;
                say(&mut r, "150 Send it");
                let Some(mut d) = data(&mut pasv, prot) else { return };
                let mut buf = vec![0; 65536];
                loop {
                    match d.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => f.write_all(&buf[..n]).unwrap(),
                    }
                }
                drop(d);
                say(&mut r, "226 Stored");
            }
            "RETR" => {
                let p = path_of(root, &cwd, arg);
                let Ok(mut f) = File::open(&p) else {
                    say(&mut r, "550 No such file");
                    continue;
                };
                f.seek(SeekFrom::Start(rest)).unwrap();
                rest = 0;
                say(&mut r, "150 Here it comes");
                let Some(mut d) = data(&mut pasv, prot) else { return };
                let mut buf = vec![0; 65536];
                loop {
                    let n = f.read(&mut buf).unwrap();
                    if n == 0 || d.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                d.close();
                say(&mut r, "226 Sent");
            }
            "SIZE" => match std::fs::metadata(path_of(root, &cwd, arg)) {
                Ok(m) if m.is_file() => say(&mut r, &format!("213 {}", m.len())),
                _ => say(&mut r, "550 No such file"),
            },
            "MDTM" => match std::fs::metadata(path_of(root, &cwd, arg)) {
                Ok(_) => say(&mut r, "213 20260101120000"),
                _ => say(&mut r, "550 No such file"),
            },
            "MKD" => match std::fs::create_dir(path_of(root, &cwd, arg)) {
                Ok(_) => say(&mut r, "257 Created"),
                _ => say(&mut r, "550 Can't create"),
            },
            "RMD" => match std::fs::remove_dir(path_of(root, &cwd, arg)) {
                Ok(_) => say(&mut r, "250 Removed"),
                _ => say(&mut r, "550 Can't remove"),
            },
            "DELE" => match std::fs::remove_file(path_of(root, &cwd, arg)) {
                Ok(_) => say(&mut r, "250 Deleted"),
                _ => say(&mut r, "550 Can't delete"),
            },
            "RNFR" => {
                from = path_of(root, &cwd, arg);
                say(&mut r, "350 Ready");
            }
            "RNTO" => match std::fs::rename(&from, path_of(root, &cwd, arg)) {
                Ok(_) => say(&mut r, "250 Renamed"),
                _ => say(&mut r, "550 Can't rename"),
            },
            "QUIT" => {
                say(&mut r, "221 Bye");
                return;
            }
            _ => say(&mut r, "500 Unknown command"),
        }
    }
}
