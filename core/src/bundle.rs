// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Many small files as one stream for Cement (SITE BUNDLE, protocol 3): a
//! plain tar (ustar, PAX headers for long names and huge files), never
//! compressed, so the 3DS spends no CPU on it. It is built while it is sent:
//! nothing is written to the PC's disk.

use std::fs::File;
use std::io::{self, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const BLOCK: usize = 512;
const MAX_OCTAL_SIZE: u64 = 0o777_7777_7777; // what the 11-digit size field holds

/// one file or folder of the bundle: where it is on the PC, where it goes
/// (relative to the bundle's folder, with '/')
#[derive(Clone, Debug)]
pub struct Item {
    pub local: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

fn octal(field: &mut [u8], v: u64) {
    // digits, then a NUL, right-aligned with leading zeros
    let s = format!("{:0w$o}", v, w = field.len() - 1);
    field[..s.len()].copy_from_slice(s.as_bytes());
    field[field.len() - 1] = 0;
}

fn header(name: &[u8], size: u64, kind: u8) -> [u8; BLOCK] {
    let mut h = [0u8; BLOCK];
    h[..name.len().min(100)].copy_from_slice(&name[..name.len().min(100)]);
    octal(&mut h[100..108], if kind == b'5' { 0o755 } else { 0o644 });
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size.min(MAX_OCTAL_SIZE));
    octal(&mut h[136..148], 0);
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    // checksum: the header's bytes with the checksum field as spaces
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|&b| b as u32).sum();
    let s = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(s.as_bytes());
    h
}

/// "<length> <key>=<value>\n", the length counting itself
fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = format!(" {key}={value}\n");
    let mut n = body.len() + 1;
    while n.to_string().len() + body.len() != n {
        n = n.to_string().len() + body.len();
    }
    format!("{n}{body}").into_bytes()
}

fn padding(n: u64) -> usize {
    ((BLOCK as u64 - n % BLOCK as u64) % BLOCK as u64) as usize
}

/// the headers of one item (a PAX header first when needed)
fn headers(it: &Item) -> Vec<u8> {
    let mut name = it.path.trim_start_matches('/').to_string();
    if it.dir && !name.ends_with('/') {
        name.push('/');
    }
    let size = if it.dir { 0 } else { it.size };
    let mut out = Vec::new();
    let mut pax = Vec::new();
    if name.len() > 100 {
        pax.extend(pax_record("path", &name));
    }
    if size > MAX_OCTAL_SIZE {
        pax.extend(pax_record("size", &size.to_string()));
    }
    if !pax.is_empty() {
        out.extend(header(b"PaxHeader", pax.len() as u64, b'x'));
        let len = pax.len() as u64;
        out.extend(pax);
        out.extend(std::iter::repeat_n(0u8, padding(len)));
    }
    out.extend(header(name.as_bytes(), size, if it.dir { b'5' } else { b'0' }));
    out
}

/// the bundle's exact size, for ALLO and the progress bar
pub fn total_size(items: &[Item]) -> u64 {
    items.iter().map(|it| headers(it).len() as u64 + if it.dir { 0 } else { it.size + padding(it.size) as u64 }).sum::<u64>()
        + 2 * BLOCK as u64
}

/// reads the bundle, file after file
pub struct Reader {
    items: Vec<Item>,
    next: usize,
    /// bytes waiting to be read (headers, padding, the end blocks)
    pending: Vec<u8>,
    pos: usize,
    file: Option<(File, u64, u64)>, // open file, bytes left, its padding
    done: bool,
    /// index of the item being read (for "file x of n"), readable while it sends
    pub current: Arc<AtomicUsize>,
}

impl Reader {
    pub fn new(items: Vec<Item>) -> Reader {
        Reader { items, next: 0, pending: Vec::new(), pos: 0, file: None, done: false, current: Arc::default() }
    }

    fn refill(&mut self) -> io::Result<()> {
        if let Some((_, 0, pad)) = &self.file {
            self.pending = vec![0u8; *pad as usize];
            self.pos = 0;
            self.file = None;
            return Ok(());
        }
        if self.next < self.items.len() {
            let it = self.items[self.next].clone();
            self.current.store(self.next, Ordering::Relaxed);
            self.next += 1;
            self.pending = headers(&it);
            self.pos = 0;
            if !it.dir {
                let f = File::open(&it.local)
                    .map_err(|e| io::Error::new(e.kind(), format!("Can't open local file {}", it.local)))?;
                self.file = Some((f, it.size, padding(it.size) as u64));
            }
        } else if !self.done {
            self.done = true;
            self.pending = vec![0u8; 2 * BLOCK];
            self.pos = 0;
        }
        Ok(())
    }
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.pos < self.pending.len() {
                let n = (self.pending.len() - self.pos).min(buf.len());
                buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
                self.pos += n;
                return Ok(n);
            }
            if let Some((f, left, _)) = &mut self.file {
                if *left > 0 {
                    let want = (*left).min(buf.len() as u64) as usize;
                    let n = f.read(&mut buf[..want])?;
                    if n == 0 {
                        // the file shrank since it was listed: keep the stream's sizes right
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "a file changed while it was sent"));
                    }
                    *left -= n as u64;
                    return Ok(n);
                }
            }
            if self.done && self.file.is_none() && self.pos >= self.pending.len() && self.next >= self.items.len() {
                return Ok(0);
            }
            self.refill()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(items: Vec<Item>) -> Vec<u8> {
        let mut out = Vec::new();
        Reader::new(items).read_to_end(&mut out).unwrap();
        out
    }

    fn checksum_ok(h: &[u8]) -> bool {
        let stored = u32::from_str_radix(std::str::from_utf8(&h[148..154]).unwrap(), 8).unwrap();
        let sum: u32 = h.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { b as u32 }).sum();
        stored == sum
    }

    #[test]
    fn layout() {
        let dir = std::env::temp_dir().join(format!("baguette-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.txt");
        std::fs::write(&a, b"hello").unwrap();
        let long = "x".repeat(150);
        let items = vec![
            Item { local: String::new(), path: "folder".into(), dir: true, size: 0 },
            Item { local: a.to_string_lossy().into(), path: "folder/a.txt".into(), dir: false, size: 5 },
            Item { local: a.to_string_lossy().into(), path: format!("folder/{long}.txt"), dir: false, size: 5 },
        ];
        let total = total_size(&items);
        let t = bundle(items);
        assert_eq!(t.len() as u64, total, "the announced size is exact");
        assert_eq!(t.len() % BLOCK, 0);
        // folder header
        assert_eq!(&t[..7], b"folder/");
        assert_eq!(t[156], b'5');
        assert!(checksum_ok(&t[..BLOCK]));
        // file header and data
        let h = &t[BLOCK..2 * BLOCK];
        assert_eq!(&h[..12], b"folder/a.txt");
        assert_eq!(&h[124..135], b"00000000005");
        assert!(checksum_ok(h));
        assert_eq!(&t[2 * BLOCK..2 * BLOCK + 5], b"hello");
        // the long name goes in a PAX header first
        let p = &t[3 * BLOCK..4 * BLOCK];
        assert_eq!(p[156], b'x');
        let rec = std::str::from_utf8(&t[4 * BLOCK..4 * BLOCK + 180]).unwrap();
        let n: usize = rec.split(' ').next().unwrap().parse().unwrap();
        assert!(rec[..n].ends_with(&format!("{long}.txt\n")), "{rec}");
        assert_eq!(rec[..n].len(), n, "the record length counts itself");
        // two zero blocks at the end
        assert!(t[t.len() - 2 * BLOCK..].iter().all(|&b| b == 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pax_lengths() {
        for len in [1, 5, 90, 94, 95, 96, 990, 994, 995] {
            let r = pax_record("path", &"y".repeat(len));
            let n: usize = std::str::from_utf8(&r).unwrap().split(' ').next().unwrap().parse().unwrap();
            assert_eq!(n, r.len(), "value of {len}");
        }
    }
}
