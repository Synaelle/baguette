// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! A plain ini file ([section], key=value, ; comments), read and changed the way
//! Windows' GetPrivateProfileString and WritePrivateProfileString do it: names in
//! any case, values trimmed and unquoted. The settings use it where Windows'
//! functions don't exist.

fn section_name(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']').map(str::trim)
}

fn key_value(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start();
    if t.starts_with(';') || t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    Some((k.trim(), v.trim()))
}

/// the value of `key` in `section`; None when it isn't there
pub fn get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if let Some(s) = section_name(line) {
            inside = s.eq_ignore_ascii_case(section);
        } else if inside {
            if let Some((_, v)) = key_value(line).filter(|(k, _)| k.eq_ignore_ascii_case(key)) {
                let unquoted = v.len() >= 2 && (v.starts_with('"') && v.ends_with('"') || v.starts_with('\'') && v.ends_with('\''));
                return Some(if unquoted { v[1..v.len() - 1].to_string() } else { v.to_string() });
            }
        }
    }
    None
}

/// as GetPrivateProfileInt: `default` when missing, the leading digits otherwise (0 when none)
pub fn get_int(text: &str, section: &str, key: &str, default: i32) -> i32 {
    let Some(v) = get(text, section, key) else { return default };
    let (neg, digits) = match v.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, v.as_str()),
    };
    let n: i64 = digits.chars().take_while(char::is_ascii_digit).fold(0, |n, c| (n * 10 + (c as i64 - 48)).min(i32::MAX as i64));
    (if neg { -n } else { n }) as i32
}

/// the text with `key` set to `value` in `section` (both created when missing).
/// value None removes the key, key None removes the whole section.
pub fn set(text: &str, section: &str, key: Option<&str>, value: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    let mut found_section = false;
    let mut done = false;
    for line in text.lines() {
        if let Some(s) = section_name(line) {
            if inside && !done {
                if let (Some(k), Some(v)) = (key, value) {
                    insert_before_blanks(&mut out, format!("{k}={v}"));
                }
                done = true;
            }
            inside = s.eq_ignore_ascii_case(section);
            if inside {
                found_section = true;
                if key.is_none() {
                    continue; // dropping the whole section: its header goes too
                }
            }
        } else if inside {
            if key.is_none() {
                continue;
            }
            if key_value(line).is_some_and(|(k, _)| k.eq_ignore_ascii_case(key.unwrap())) {
                if !done {
                    if let Some(v) = value {
                        out.push(format!("{}={v}", key.unwrap()));
                    }
                    done = true;
                }
                continue;
            }
        }
        out.push(line.to_string());
    }
    if let (Some(k), Some(v)) = (key, value) {
        if inside && !done {
            insert_before_blanks(&mut out, format!("{k}={v}"));
        } else if !found_section {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{section}]"));
            out.push(format!("{k}={v}"));
        }
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// a new key goes after the section's last entry, not after the blank lines that
/// separate it from the next one
fn insert_before_blanks(out: &mut Vec<String>, line: String) {
    let at = out.iter().rposition(|l| !l.trim().is_empty()).map_or(out.len(), |i| i + 1);
    out.insert(at, line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read() {
        let t = "; settings\r\n[Server]\r\nhost = 192.168.1.10 \r\nport=5000\r\n\r\n[ui]\r\naccent=\"b8283c\"\r\ncount=-3x\r\nbad=abc\r\n";
        assert_eq!(get(t, "server", "HOST").as_deref(), Some("192.168.1.10"));
        assert_eq!(get(t, "ui", "accent").as_deref(), Some("b8283c"), "quotes dropped");
        assert_eq!(get(t, "ui", "host"), None, "only in its section");
        assert_eq!(get_int(t, "server", "port", 1), 5000);
        assert_eq!(get_int(t, "ui", "count", 1), -3);
        assert_eq!(get_int(t, "ui", "bad", 1), 0);
        assert_eq!(get_int(t, "ui", "missing", 7), 7);
    }

    #[test]
    fn write() {
        let t = set("", "server", Some("host"), Some("10.0.0.2"));
        assert_eq!(t, "[server]\nhost=10.0.0.2\n");
        let t = set(&t, "server", Some("port"), Some("5000"));
        let t = set(&t, "ui", Some("dark"), Some("1"));
        assert_eq!(t, "[server]\nhost=10.0.0.2\nport=5000\n\n[ui]\ndark=1\n");
        let t = set(&t, "SERVER", Some("Host"), Some("pi.local"));
        assert_eq!(get(&t, "server", "host").as_deref(), Some("pi.local"));
        assert_eq!(t.matches("host").count() + t.matches("Host").count(), 1, "replaced, not added");
        let t = set(&t, "server", Some("port"), None);
        assert_eq!(get(&t, "server", "port"), None, "key removed");
        let t = set(&t, "server", Some("x"), Some("1"));
        assert_eq!(t, "[server]\nHost=pi.local\nx=1\n\n[ui]\ndark=1\n", "added before the blank line");
        let t = set(&t, "server", None, None);
        assert_eq!(t, "[ui]\ndark=1\n", "section removed, with its blank line");
        assert_eq!(get(&t, "ui", "dark").as_deref(), Some("1"));
    }
}
