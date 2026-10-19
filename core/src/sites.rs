// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! The saved sites, in chomp.ini next to the exe like everything else.
//! Passwords and key passphrases are encrypted for the Windows user (DPAPI):
//! copied to another PC or user, the file still opens, and those sites just
//! ask for their password again.

use crate::config::{config_path, escape, get_int, get_str, put, unescape};
use crate::remote::{Protocol, Site};
use crate::win::{protect, unprotect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: u32, // stable across renames and reordering
    pub name: String,
    pub site: Site,
    /// the saved password could not be decrypted here (other PC or user)
    pub needs_password: bool,
}

/// the sites in their saved order. The first time, the Cement address of
/// earlier versions becomes a "My 3DS" site.
pub fn load() -> Vec<Saved> {
    let path = config_path();
    let count = get_int(&path, "sites", "count", -1);
    if count < 0 {
        let host = get_str(&path, "server", "host", "");
        if host.is_empty() {
            return Vec::new();
        }
        let port = get_int(&path, "server", "port", 5000) as u16;
        let first = vec![Saved { id: 1, name: "My 3DS".into(), site: Site::cement(&host, port), needs_password: false }];
        save(&first);
        return first;
    }
    let mut out = Vec::new();
    for i in 0..count {
        let sec = format!("site{i}");
        let s = |k: &str| unescape(&get_str(&path, &sec, k, ""));
        let protocol = Protocol::parse(&s("protocol")).unwrap_or_default();
        let pass = unprotect(&s("pass"));
        let passphrase = unprotect(&s("passphrase"));
        let needs_password = pass.is_none() || passphrase.is_none();
        let site = Site {
            protocol,
            host: s("host"),
            port: get_int(&path, &sec, "port", protocol.default_port() as i32) as u16,
            user: s("user"),
            pass: pass.unwrap_or_default(),
            key: s("key"),
            passphrase: passphrase.unwrap_or_default(),
            trusted: s("trusted").split('|').filter(|t| !t.is_empty()).map(String::from).collect(),
            start_dir: s("start"),
        };
        if site.host.is_empty() {
            continue;
        }
        let id = get_int(&path, &sec, "id", 0) as u32;
        out.push(Saved { id: if id == 0 { i as u32 + 1 } else { id }, name: s("name"), site, needs_password });
    }
    out
}

pub fn save(sites: &[Saved]) {
    let path = config_path();
    let old = get_int(&path, "sites", "count", 0).max(0);
    for i in 0..old {
        put(&path, &format!("site{i}"), None, None);
    }
    put(&path, "sites", Some("count"), Some(&sites.len().to_string()));
    for (i, sv) in sites.iter().enumerate() {
        let sec = format!("site{i}");
        let p = |k: &str, v: &str| put(&path, &sec, Some(k), Some(&escape(v)));
        let s = &sv.site;
        p("id", &sv.id.to_string());
        p("name", &sv.name);
        p("protocol", s.protocol.name());
        p("host", &s.host);
        p("port", &s.port.to_string());
        p("user", &s.user);
        p("pass", &protect(&s.pass));
        p("key", &s.key);
        p("passphrase", &protect(&s.passphrase));
        p("trusted", &s.trusted.join("|"));
        p("start", &s.start_dir);
    }
}

/// a free id for a new site
pub fn next_id(sites: &[Saved]) -> u32 {
    sites.iter().map(|s| s.id).max().unwrap_or(0) + 1
}

/// "sftp://user@host:port/dir" (quick connect); the scheme defaults to sftp
/// when there is a user, ftp otherwise
pub fn parse_url(url: &str) -> Option<Site> {
    let url = url.trim();
    let (scheme, rest) = match url.split_once("://") {
        Some((s, r)) => (Some(Protocol::parse(s)?), r),
        None => (None, url),
    };
    let (auth, rest) = match rest.rsplit_once('@') {
        Some((a, r)) => (Some(a), r),
        None => (None, rest),
    };
    let (hostport, dir) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (user, pass) = match auth {
        Some(a) => match a.split_once(':') {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (a.to_string(), String::new()),
        },
        None => (String::new(), String::new()),
    };
    let protocol = scheme.unwrap_or(if user.is_empty() { Protocol::Ftp } else { Protocol::Sftp });
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') || h.starts_with('[') => (h.trim_matches(['[', ']']).to_string(), p.parse().ok()?),
        _ => (hostport.trim_matches(['[', ']']).to_string(), protocol.default_port()),
    };
    if host.is_empty() {
        return None;
    }
    Some(Site { protocol, host, port, user, pass, start_dir: dir.to_string(), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let s = parse_url("sftp://pi@raspberrypi.local/home/pi").unwrap();
        assert_eq!((s.protocol, s.host.as_str(), s.port, s.user.as_str(), s.start_dir.as_str()),
            (Protocol::Sftp, "raspberrypi.local", 22, "pi", "/home/pi"));
        let s = parse_url("pi@10.0.0.2:2222").unwrap();
        assert_eq!((s.protocol, s.port), (Protocol::Sftp, 2222));
        let s = parse_url("192.168.1.10:5000").unwrap();
        assert_eq!((s.protocol, s.host.as_str(), s.port), (Protocol::Ftp, "192.168.1.10", 5000));
        let s = parse_url("ftps://me:pw@nas").unwrap();
        assert_eq!((s.protocol, s.port, s.pass.as_str()), (Protocol::Ftps, 21, "pw"));
        assert!(parse_url("gopher://x").is_none());
        assert!(parse_url("").is_none());
    }

    #[test]
    fn dpapi_round_trip() {
        let h = protect("hunter2 é");
        assert!(!h.is_empty() && !h.contains("hunter2"));
        assert_eq!(unprotect(&h).as_deref(), Some("hunter2 é"));
        assert_eq!(unprotect("00ff").as_deref(), None);
    }

    #[test]
    fn save_and_load() {
        let file = std::env::temp_dir().join(format!("chomp-sites-{}.ini", std::process::id()));
        let _ = std::fs::remove_file(&file);
        std::fs::write(&file, "[server]\r\nhost=192.168.1.10\r\nport=5000\r\n").unwrap();
        unsafe { std::env::set_var("CHOMP_CONFIG", &file) };
        let first = load();
        assert_eq!(first.len(), 1, "the old Cement address became a site");
        assert_eq!((first[0].name.as_str(), first[0].site.port), ("My 3DS", 5000));
        let mut all = first.clone();
        all.push(Saved {
            id: next_id(&all),
            name: "Pi été".into(),
            site: Site {
                protocol: Protocol::Sftp,
                host: "pi.local".into(),
                port: 22,
                user: "pi".into(),
                pass: "secret".into(),
                trusted: vec!["SHA256:abc".into(), "SHA256:def".into()],
                ..Default::default()
            },
            needs_password: false,
        });
        save(&all);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("secret"), "password not in clear");
        assert_eq!(load(), all);
        save(&all[1..]);
        assert_eq!(load(), all[1..].to_vec(), "removed sites are gone");
        let _ = std::fs::remove_file(&file);
    }
}
