// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Folder listings: MLSD lines, and the Unix or DOS style LIST output of
//! servers without MLSD.

use crate::ftp::Entry;
use crate::util::parse_u64;

/// "type=file;size=12;modify=20260101120000; name"
pub fn parse_mlsd(raw: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Some(sp) = line.find("; ") else { continue };
        let (facts, name) = (&line[..=sp], &line[sp + 2..]);
        let mut e = Entry { name: name.to_string(), ..Default::default() };
        let mut skip = false;
        for fact in facts.split(';') {
            let Some((k, v)) = fact.split_once('=') else { continue };
            let (k, v) = (k.to_ascii_lowercase(), v.to_ascii_lowercase());
            match k.as_str() {
                "type" => {
                    e.dir = v == "dir";
                    skip = v == "cdir" || v == "pdir";
                }
                "size" => e.size = parse_u64(&v),
                "modify" if v.len() >= 14 => e.modified = v[..14].to_string(),
                _ => {}
            }
        }
        if !skip && !e.name.is_empty() {
            out.push(e);
        }
    }
    out
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// the n first whitespace-separated words and the rest of the line (the name
/// may hold spaces)
fn split_words(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut words = Vec::with_capacity(n);
    let mut rest = line.trim_start();
    for _ in 0..n {
        let end = rest.find(char::is_whitespace)?;
        words.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    Some((words, rest))
}

/// "Jan 5 12:30" (this year or last) or "Jan 5 2024"
fn unix_date(mon: &str, day: &str, yt: &str, this_year: u32, this_month: u32) -> String {
    let Some(m) = MONTHS.iter().position(|x| mon.eq_ignore_ascii_case(x)) else { return String::new() };
    let m = m as u32 + 1;
    let d = parse_u64(day) as u32;
    if let Some((h, mi)) = yt.split_once(':') {
        // no year: the last 6 months, so a later month is last year
        let y = if m > this_month + 1 { this_year - 1 } else { this_year };
        format!("{y:04}{m:02}{d:02}{:02}{:02}00", parse_u64(h), parse_u64(mi))
    } else {
        format!("{:04}{m:02}{d:02}000000", parse_u64(yt))
    }
}

fn parse_unix(line: &str, this_year: u32, this_month: u32) -> Option<Entry> {
    let kind = line.chars().next()?;
    if !"-dlbcps".contains(kind) {
        return None;
    }
    let (w, name) = split_words(line, 8)?;
    if !w[4].bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let name = if kind == 'l' { name.split(" -> ").next().unwrap_or(name) } else { name };
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(Entry {
        name: name.to_string(),
        dir: kind == 'd',
        size: if kind == 'd' { 0 } else { parse_u64(w[4]) },
        modified: unix_date(w[5], w[6], w[7], this_year, this_month),
    })
}

/// "01-05-24  12:30PM  <DIR>  name" or "... 1234 name"
fn parse_dos(line: &str) -> Option<Entry> {
    let (w, name) = split_words(line, 3)?;
    let date: Vec<u64> = w[0].split(['-', '/']).map(parse_u64).collect();
    if date.len() != 3 || name.is_empty() {
        return None;
    }
    let dir = w[2].eq_ignore_ascii_case("<dir>");
    if !dir && !w[2].bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let t = w[1].to_ascii_lowercase();
    let (pm, am) = (t.ends_with("pm"), t.ends_with("am"));
    let (h, mi) = t.trim_end_matches(['a', 'p', 'm']).split_once(':').map(|(h, m)| (parse_u64(h), parse_u64(m))).unwrap_or((0, 0));
    let h = if pm && h < 12 { h + 12 } else if am && h == 12 { 0 } else { h };
    let y = if date[2] < 70 { 2000 + date[2] } else if date[2] < 100 { 1900 + date[2] } else { date[2] };
    Some(Entry {
        name: name.to_string(),
        dir,
        size: if dir { 0 } else { parse_u64(w[2]) },
        modified: format!("{y:04}{:02}{:02}{h:02}{mi:02}00", date[0], date[1]),
    })
}

/// LIST output; lines that look like neither format ("total 12") are skipped
pub fn parse_list(raw: &str, this_year: u32, this_month: u32) -> Vec<Entry> {
    raw.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .filter_map(|l| parse_unix(l, this_year, this_month).or_else(|| parse_dos(l)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mlsd() {
        let v = parse_mlsd("type=cdir; .\r\ntype=dir;modify=20260101120000; luma\r\ntype=file;size=12; a b.txt\r\n");
        assert_eq!(v.len(), 2);
        assert!(v[0].dir && v[0].name == "luma" && v[0].modified == "20260101120000");
        assert_eq!((v[1].name.as_str(), v[1].size), ("a b.txt", 12));
    }

    #[test]
    fn unix() {
        let raw = "total 8\r\n\
            drwxr-xr-x    2 pi   pi       4096 Mar  3 09:15 my dir\r\n\
            -rw-r--r--    1 pi   pi     123456 Dec 24  2024 song.mp3\r\n\
            lrwxrwxrwx    1 pi   pi          7 Oct  1 10:00 link -> /target\r\n\
            drwxr-xr-x    2 pi   pi       4096 Oct  1 10:00 ..\r\n";
        let v = parse_list(raw, 2026, 10);
        assert_eq!(v.len(), 3);
        assert!(v[0].dir && v[0].name == "my dir" && v[0].modified == "20260303091500");
        assert_eq!((v[1].size, v[1].modified.as_str()), (123456, "20241224000000"));
        assert_eq!(v[2].name, "link");
        // a month after the current one without a year is last year
        assert_eq!(parse_list("-rw-r--r-- 1 a b 1 Dec  1 10:00 x", 2026, 10)[0].modified, "20251201100000");
    }

    #[test]
    fn dos() {
        let v = parse_list("01-05-24  12:30PM       <DIR>          Program Files\r\n10-19-26  09:05AM              2048 a.txt\r\n", 2026, 10);
        assert_eq!(v.len(), 2);
        assert!(v[0].dir && v[0].name == "Program Files" && v[0].modified == "20240105123000");
        assert_eq!((v[1].size, v[1].modified.as_str()), (2048, "20261019090500"));
    }
}
