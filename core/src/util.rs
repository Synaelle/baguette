// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
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

/// a name from a server listing that is safe as one part of a local path: not
/// empty, not . or .., and no separator or NUL (nor a drive or stream colon on
/// Windows)
pub fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0']) && !(cfg!(windows) && name.contains(':'))
}

/// dir + a name that came from the server, refused when it could reach outside dir
pub fn join_local(dir: &str, name: &str) -> crate::ftp::Result<String> {
    if safe_name(name) {
        Ok(crate::os::join(dir, name))
    } else {
        Err(crate::ftp::Error::Ftp(format!("Refused the server's file name {name:?}")))
    }
}

/// where a download is written until it is complete, so a failed or cancelled
/// one never touches a file that was already there
pub fn part_path(local: &str) -> String {
    format!("{local}.part")
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

/// "/a/b" form: forward slashes, a leading slash, no doubled or trailing slash,
/// and . and .. resolved (.. stops at the root)
pub fn normalize_remote(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
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

/// (year, month) today (UTC), for listings that leave the year out
pub fn this_year_month() -> (u32, u32) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (y, m, _) = civil_from_days((secs / 86400) as i64);
    (y as u32, m)
}

/// days since 1970-01-01 to (year, month, day)
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
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
        assert_eq!(normalize_remote("/3ds/../boot.firm"), "/boot.firm");
        assert_eq!(normalize_remote("/a/./b/.."), "/a");
        assert_eq!(normalize_remote("/../../x"), "/x");
        assert_eq!(normalize_remote("/a/..."), "/a/...");
    }

    #[test]
    fn server_names() {
        for bad in ["", ".", "..", "../x", "a/b", "..\\..\\x", "a\0b"] {
            assert!(!safe_name(bad), "{bad:?}");
            assert!(join_local("/tmp/d", bad).is_err(), "{bad:?}");
        }
        for good in ["a", "...", "a b.txt", ".hidden", "x..y"] {
            assert!(safe_name(good), "{good:?}");
        }
        assert_eq!(safe_name("C:x"), !cfg!(windows));
        assert_eq!(join_local("/tmp/d", "a").unwrap(), crate::os::join("/tmp/d", "a"));
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
