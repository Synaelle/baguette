// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Cement's extras: what SITE STATUS says about the 3DS (version, protocol
//! level, SD card space, battery, other transfers).

use crate::util::{parse_i64, parse_u64};

/// protocol level from a SITE STATUS reply; builds from before the field existed
/// already had everything of level 1, a server without SITE STATUS is 0
pub fn protocol_level(code: i32, text: &str) -> i32 {
    if code != 211 {
        return 0;
    }
    match text.find(" protocol=") {
        None => 1,
        Some(p) => parse_i64(&text[p + 10..]) as i32,
    }
}

/// a number field (" name=123") from a SITE STATUS reply's summary line
pub fn status_number(text: &str, name: &str) -> Option<u64> {
    let key = format!(" {name}=");
    text.find(&key).map(|p| parse_u64(&text[p + key.len()..]))
}

/// the 3DS battery, from SITE STATUS (3DS apps from before it: known == false)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub known: bool,
    pub percent: i32, // -1: only the 0-5 bars are known
    pub bars: i32,
    pub charging: bool,
    pub plugged: bool,
    pub low: bool, // the 3DS refuses new transfers until it is plugged in
}

impl Default for Battery {
    fn default() -> Battery {
        Battery { known: false, percent: -1, bars: -1, charging: false, plugged: false, low: false }
    }
}

pub fn parse_battery(text: &str) -> Battery {
    if !text.contains(" battery=") {
        return Battery::default();
    }
    let num = |name: &str| -> i64 {
        let key = format!(" {name}=");
        text.find(&key).map_or(-1, |p| parse_i64(&text[p + key.len()..]))
    };
    Battery {
        known: true,
        percent: num("battery") as i32,
        bars: num("battery_bars") as i32,
        charging: num("charging") == 1,
        plugged: num("plugged") == 1,
        low: num("battery_low") == 1,
    }
}

/// "87%", "3/5 bars", plus ", charging" / ", plugged in"
pub fn battery_text(b: &Battery) -> String {
    let t = if b.percent >= 0 {
        format!("{}%", b.percent)
    } else if b.bars >= 0 {
        format!("{}/5 bars", b.bars)
    } else {
        "unknown".into()
    };
    t + if b.charging {
        ", charging"
    } else if b.plugged {
        ", plugged in"
    } else {
        ""
    }
}

/// the 3DS refused a transfer because its battery is too low (retrying won't help)
pub fn battery_refusal(msg: &str) -> bool {
    msg.contains("battery too low")
}

/// version ("" if it does not say), protocol level, SD space and battery of the 3DS app
#[derive(Clone, Debug, Default)]
pub struct ServerInfo {
    pub cement: bool, // SITE STATUS answered: the server is the 3DS app
    pub version: String,
    pub protocol: i32,
    pub has_space: bool, // the 3DS app reports SD space (sd_free, sd_total)
    pub sd_free: u64,
    pub sd_total: u64,
    pub battery: Battery,
    /// everything else SITE STATUS said (CIAs, installs, accent)
    pub full: Status,
}

pub fn parse_server_info(code: i32, text: &str) -> ServerInfo {
    let mut info = ServerInfo { protocol: protocol_level(code, text), ..Default::default() };
    if code != 211 {
        return info;
    }
    info.cement = true;
    info.full = parse_status(code, text);
    if let (Some(free), Some(total)) = (status_number(text, "sd_free"), status_number(text, "sd_total")) {
        info.sd_free = free;
        info.sd_total = total;
        info.has_space = total != 0;
    }
    info.battery = parse_battery(text);
    if let Some(p) = text.find(" version=") {
        let rest = &text[p + 9..];
        let end = rest.find([' ', '\r', '\n']).unwrap_or(rest.len());
        info.version = rest[..end].to_string();
    }
    info
}

/// one transfer another connection is doing
#[derive(Clone, Debug, Default)]
pub struct Transfer {
    pub peer: String,
    pub dir: String,
    pub file: String,
    pub batch: String,
    pub done: u64,
    pub size: u64,
    pub bps: u64,
    pub eta: i64,
    pub batch_files_done: u64,
    pub batch_files: u64,
    pub batch_bytes_done: u64,
    pub batch_bytes: u64,
    pub batch_eta: i64,
}

/// everything SITE STATUS says
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub known: bool, // false: server without SITE STATUS
    pub busy: bool,
    pub installing: bool,
    pub idle_ms: u64,
    pub connections: i32,
    pub protocol: i32,
    pub version: String,
    pub has_space: bool,
    pub sd_free: u64,
    pub sd_total: u64,
    pub battery: Battery,
    pub transfers: Vec<Transfer>,
    pub install_done: u64,
    pub install_size: u64,
    pub install_file: String,
    /// accent colour of the 3DS screens, "b8283c" (protocol 2)
    pub accent: String,
    /// CIA files waiting to be installed (protocol 2)
    pub cias: Vec<Cia>,
    /// the last install that finished (protocol 2)
    pub install_result: Option<InstallResult>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cia {
    pub size: u64,
    pub file: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstallResult {
    pub n: u64, // goes up by one per finished install
    pub ok: bool,
    pub code: String,
    pub file: String,
    pub error: String,
}

/// "key=value key=value ... file=<rest of the line>"
fn fields(line: &str) -> Vec<(String, String)> {
    let mut kv = Vec::new();
    let mut pos = 0;
    let b = line.as_bytes();
    while pos < line.len() {
        while pos < line.len() && b[pos] == b' ' {
            pos += 1;
        }
        let Some(eq) = line[pos..].find('=').map(|e| e + pos) else { break };
        let key = &line[pos..eq];
        let end = if key == "file" { line.len() } else { line[eq..].find(' ').map_or(line.len(), |e| e + eq) };
        kv.push((key.to_string(), line[eq + 1..end].to_string()));
        pos = end;
    }
    kv
}

pub fn parse_status(code: i32, text: &str) -> Status {
    let mut st = Status::default();
    if code != 211 {
        return st;
    }
    st.known = true;
    st.protocol = protocol_level(code, text);
    st.battery = parse_battery(text);
    for line in text.split('\n') {
        let Some(line) = line.strip_prefix(' ') else { continue };
        let line = line.trim_end_matches('\r');
        let kind = line.split(' ').next().unwrap_or("");
        if kind.starts_with("busy=") {
            for (k, v) in fields(line) {
                match k.as_str() {
                    "busy" => st.busy = v == "1",
                    "idle_ms" => st.idle_ms = parse_u64(&v),
                    "installing" => st.installing = v == "1",
                    "connections" => st.connections = parse_u64(&v) as i32,
                    "version" => st.version = v,
                    "accent" => st.accent = v.to_ascii_lowercase(),
                    "sd_free" => st.sd_free = parse_u64(&v),
                    "sd_total" => {
                        st.sd_total = parse_u64(&v);
                        st.has_space = st.sd_total > 0;
                    }
                    _ => {}
                }
            }
        } else if kind == "transfer" {
            let mut t = Transfer { eta: -1, batch_eta: -1, ..Default::default() };
            for (k, v) in fields(line.get(9..).unwrap_or("")) {
                match k.as_str() {
                    "peer" => t.peer = v,
                    "dir" => t.dir = v,
                    "done" => t.done = parse_u64(&v),
                    "size" => t.size = parse_u64(&v),
                    "bps" => t.bps = parse_u64(&v),
                    "eta" => t.eta = parse_i64(&v),
                    "batch" => t.batch = v,
                    "batch_files_done" => t.batch_files_done = parse_u64(&v),
                    "batch_files" => t.batch_files = parse_u64(&v),
                    "batch_bytes_done" => t.batch_bytes_done = parse_u64(&v),
                    "batch_bytes" => t.batch_bytes = parse_u64(&v),
                    "batch_eta" => t.batch_eta = parse_i64(&v),
                    "file" => t.file = v,
                    _ => {}
                }
            }
            st.transfers.push(t);
        } else if kind == "cia" {
            let mut c = Cia::default();
            for (k, v) in fields(line.get(4..).unwrap_or("")) {
                match k.as_str() {
                    "size" => c.size = parse_u64(&v),
                    "file" => c.file = v,
                    _ => {}
                }
            }
            st.cias.push(c);
        } else if kind == "install_result" {
            let mut r = InstallResult::default();
            for (k, v) in fields(line.get(15..).unwrap_or("")) {
                match k.as_str() {
                    "n" => r.n = parse_u64(&v),
                    "ok" => r.ok = v == "1",
                    "code" => r.code = v,
                    "file" => r.file = v,
                    _ => {}
                }
            }
            st.install_result = Some(r);
        } else if kind == "install_error" {
            if let Some(r) = &mut st.install_result {
                r.error = line.get(14..).unwrap_or("").to_string();
            }
        } else if kind == "install" {
            for (k, v) in fields(line.get(8..).unwrap_or("")) {
                match k.as_str() {
                    "done" => st.install_done = parse_u64(&v),
                    "size" => st.install_size = parse_u64(&v),
                    "file" => st.install_file = v,
                    _ => {}
                }
            }
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = "211-Status\n busy=1 idle_ms=0 installing=0 connections=2 protocol=1 sd_free=1000 sd_total=4000 battery=87 battery_bars=4 charging=1 plugged=1 battery_low=0 version=1.2.32-abc\n transfer peer=192.168.1.10 dir=up done=10 size=100 bps=5 eta=18 batch=up batch_files_done=1 batch_files=3 batch_bytes_done=10 batch_bytes=300 batch_eta=58 file=/a b/c.bin\n211 End\n";

    #[test]
    fn status() {
        let st = parse_status(211, REPLY);
        assert!(st.known && st.busy && st.has_space);
        assert_eq!((st.sd_free, st.sd_total, st.connections, st.protocol), (1000, 4000, 2, 1));
        assert_eq!(st.version, "1.2.32-abc");
        assert_eq!(st.transfers.len(), 1);
        let t = &st.transfers[0];
        assert_eq!((t.peer.as_str(), t.file.as_str(), t.batch_eta), ("192.168.1.10", "/a b/c.bin", 58));
        assert_eq!(battery_text(&st.battery), "87%, charging");
        let info = parse_server_info(211, REPLY);
        assert_eq!(info.version, "1.2.32-abc");
        assert!(info.has_space && info.battery.known && !info.battery.low);
    }

    #[test]
    fn protocol_2() {
        let reply = "211-Status
 busy=0 idle_ms=5 installing=1 connections=1 protocol=2 sd_free=1 sd_total=2 battery=50 battery_bars=3 charging=0 plugged=0 battery_low=0 accent=B8283C version=2.0
 cia size=680384 file=/Cement.cia
 cia size=12 file=/cias/a b.cia
 install done=10 size=20 file=/Cement.cia
 install_result n=3 ok=0 code=d8e08027 file=/old.cia
 install_error Install write failed
211 End
";
        let st = parse_status(211, reply);
        assert_eq!(st.protocol, 2);
        assert_eq!(st.accent, "b8283c");
        assert_eq!(st.cias, vec![Cia { size: 680384, file: "/Cement.cia".into() }, Cia { size: 12, file: "/cias/a b.cia".into() }]);
        assert_eq!((st.install_done, st.install_size, st.install_file.as_str()), (10, 20, "/Cement.cia"));
        let r = st.install_result.unwrap();
        assert_eq!((r.n, r.ok, r.code.as_str(), r.file.as_str(), r.error.as_str()), (3, false, "d8e08027", "/old.cia", "Install write failed"));
        assert_eq!(parse_server_info(211, reply).full.cias.len(), 2);
    }

    #[test]
    fn old_servers() {
        assert_eq!(protocol_level(211, "211-Status\n busy=0\n211 End"), 1);
        assert_eq!(protocol_level(502, "502 no"), 0);
        assert!(!parse_battery("211 busy=0").known);
    }
}
