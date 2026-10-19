// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! A throwaway SFTP server on 127.0.0.1 serving a folder, for the tests:
//! password "secret" or one client key for user "pi".

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version};

pub fn key(seed: u8) -> PrivateKey {
    PrivateKey::from(Ed25519Keypair::from_seed(&[seed; 32]))
}

pub struct Sftpd {
    pub port: u16,
    _rt: tokio::runtime::Runtime,
}

/// host key from `host_seed`; the client key from `client_seed` is accepted
pub fn start(root: &Path, host_seed: u8, client_seed: u8) -> Sftpd {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(russh::server::Config {
        keys: vec![key(host_seed)],
        auth_rejection_time: std::time::Duration::from_millis(10),
        auth_rejection_time_initial: Some(std::time::Duration::ZERO),
        ..Default::default()
    });
    let root = root.to_path_buf();
    let client = key(client_seed).public_key().clone();
    rt.spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { return };
            let h = Ssh { root: root.clone(), client: client.clone(), channels: HashMap::new() };
            let config = config.clone();
            tokio::spawn(async move {
                if let Ok(s) = russh::server::run_stream(config, sock, h).await {
                    let _ = s.await;
                }
            });
        }
    });
    Sftpd { port, _rt: rt }
}

struct Ssh {
    root: PathBuf,
    client: PublicKey,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl russh::server::Handler for Ssh {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "pi" && password == "secret" { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(if user == "pi" && key.key_data() == self.client.key_data() { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(&mut self, id: ChannelId, name: &str, session: &mut Session) -> Result<(), Self::Error> {
        match (name, self.channels.remove(&id)) {
            ("sftp", Some(ch)) => {
                session.channel_success(id)?;
                let fs = Fs { root: self.root.clone(), files: HashMap::new(), dirs: HashMap::new(), next: 0 };
                tokio::spawn(russh_sftp::server::run(ch.into_stream(), fs));
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }
}

struct Fs {
    root: PathBuf,
    files: HashMap<String, std::fs::File>,
    dirs: HashMap<String, Option<Vec<File>>>,
    next: u32,
}

fn ok(id: u32) -> Status {
    Status { id, status_code: StatusCode::Ok, error_message: "Ok".into(), language_tag: "en".into() }
}

fn io(e: std::io::Error) -> StatusCode {
    if e.kind() == std::io::ErrorKind::NotFound { StatusCode::NoSuchFile } else { StatusCode::Failure }
}

impl Fs {
    fn path(&self, p: &str) -> PathBuf {
        let mut out = self.root.clone();
        for part in p.split('/').filter(|s| !s.is_empty() && *s != ".") {
            if part == ".." {
                if out != self.root {
                    out.pop();
                }
            } else {
                out.push(part);
            }
        }
        out
    }

    fn handle(&mut self) -> String {
        self.next += 1;
        format!("h{}", self.next)
    }
}

impl russh_sftp::server::Handler for Fs {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _: u32, _: HashMap<String, String>) -> Result<Version, StatusCode> {
        Ok(Version::new())
    }

    async fn open(&mut self, id: u32, name: String, flags: OpenFlags, _: FileAttributes) -> Result<Handle, StatusCode> {
        let f = std::fs::OpenOptions::new()
            .read(flags.contains(OpenFlags::READ))
            .write(flags.contains(OpenFlags::WRITE))
            .create(flags.contains(OpenFlags::CREATE))
            .truncate(flags.contains(OpenFlags::TRUNCATE))
            .open(self.path(&name))
            .map_err(io)?;
        let h = self.handle();
        self.files.insert(h.clone(), f);
        Ok(Handle { id, handle: h })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, StatusCode> {
        self.files.remove(&handle);
        self.dirs.remove(&handle);
        Ok(ok(id))
    }

    async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, StatusCode> {
        let f = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        f.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut data = vec![0; len as usize];
        let n = f.read(&mut data).map_err(io)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(n);
        Ok(Data { id, data })
    }

    async fn write(&mut self, id: u32, handle: String, offset: u64, data: Vec<u8>) -> Result<Status, StatusCode> {
        let f = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        f.seek(SeekFrom::Start(offset)).map_err(io)?;
        f.write_all(&data).map_err(io)?;
        Ok(ok(id))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        self.stat(id, path).await
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let m = std::fs::metadata(self.path(&path)).map_err(io)?;
        Ok(Attrs { id, attrs: FileAttributes::from(&m) })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        let f = self.files.get(&handle).ok_or(StatusCode::Failure)?;
        Ok(Attrs { id, attrs: FileAttributes::from(&f.metadata().map_err(io)?) })
    }

    async fn setstat(&mut self, id: u32, _: String, _: FileAttributes) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }

    async fn fsetstat(&mut self, id: u32, _: String, _: FileAttributes) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, StatusCode> {
        let mut files = Vec::new();
        for e in std::fs::read_dir(self.path(&path)).map_err(io)? {
            let e = e.map_err(io)?;
            let m = e.metadata().map_err(io)?;
            files.push(File::new(e.file_name().to_string_lossy(), FileAttributes::from(&m)));
        }
        let h = self.handle();
        self.dirs.insert(h.clone(), Some(files));
        Ok(Handle { id, handle: h })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
        match self.dirs.get_mut(&handle).and_then(Option::take) {
            Some(files) => Ok(Name { id, files }),
            None => Err(StatusCode::Eof),
        }
    }

    async fn remove(&mut self, id: u32, name: String) -> Result<Status, StatusCode> {
        std::fs::remove_file(self.path(&name)).map_err(io)?;
        Ok(ok(id))
    }

    async fn mkdir(&mut self, id: u32, path: String, _: FileAttributes) -> Result<Status, StatusCode> {
        std::fs::create_dir(self.path(&path)).map_err(io)?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        std::fs::remove_dir(self.path(&path)).map_err(io)?;
        Ok(ok(id))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        // "." is the home folder: /home/pi
        let p = if path == "." || path.is_empty() { "/home/pi".to_string() } else { path };
        Ok(Name { id, files: vec![File::dummy(p)] })
    }

    async fn rename(&mut self, id: u32, from: String, to: String) -> Result<Status, StatusCode> {
        std::fs::rename(self.path(&from), self.path(&to)).map_err(io)?;
        Ok(ok(id))
    }
}
