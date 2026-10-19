// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Small helpers: remote paths and sizes.

/// "/a" + "b" = "/a/b"
pub fn join_remote(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") }
}

/// "/a/b" -> "/a", "/a" -> "/"
pub fn parent_remote(dir: &str) -> String {
    match dir.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => dir[..i].to_string(),
    }
}

/// last part of a remote or local path
pub fn base_name(path: &str) -> &str {
    path.rfind(['/', '\\']).map_or(path, |i| &path[i + 1..])
}

/// "1.23 GB", "4.5 MB", "12.0 KB", "512 B"
pub fn format_size(v: u64) -> String {
    if v >= 1 << 30 {
        format!("{:.2} GB", v as f64 / 1073741824.0)
    } else if v >= 1 << 20 {
        format!("{:.1} MB", v as f64 / 1048576.0)
    } else if v >= 1024 {
        format!("{:.1} KB", v as f64 / 1024.0)
    } else {
        format!("{v} B")
    }
}

/// "/a/b" form: forward slashes, a leading slash, no doubled or trailing slash
pub fn normalize_remote(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let mut out = String::new();
    for c in format!("/{p}").chars() {
        if !(c == '/' && out.ends_with('/')) {
            out.push(c);
        }
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// strtoull: leading spaces, digits, anything after ignored; 0 when there are none
pub fn parse_u64(s: &str) -> u64 {
    let t = s.trim_start();
    let t = t.strip_prefix('+').unwrap_or(t);
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

/// atoi / atoll: optional sign, then digits
pub fn parse_i64(s: &str) -> i64 {
    let t = s.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let v = parse_u64(t) as i64;
    if neg { -v } else { v }
}

/// a JSON string literal
pub fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                o.push('\\');
                o.push(c);
            }
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// "YYYYMMDDhhmmss" -> "YYYY-MM-DD hh:mm:ss"
pub fn stamp_text(s: &str) -> String {
    if s.len() < 14 || !s.is_ascii() {
        return "unknown time".into();
    }
    format!("{}-{}-{} {}:{}:{}", &s[0..4], &s[4..6], &s[6..8], &s[8..10], &s[10..12], &s[12..14])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(join_remote("/", "a"), "/a");
        assert_eq!(join_remote("/a", "b"), "/a/b");
        assert_eq!(parent_remote("/a/b"), "/a");
        assert_eq!(parent_remote("/a"), "/");
        assert_eq!(normalize_remote(" a\\\\b// "), "/a/b");
        assert_eq!(normalize_remote(""), "/");
    }

    #[test]
    fn sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(3 << 20), "3.0 MB");
        assert_eq!(format_size(5 << 30), "5.00 GB");
    }

    #[test]
    fn json() {
        assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }
}
