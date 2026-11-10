// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a transfer moves, the same for the window's workers and baguette-cli:
//! the files and folders under what was picked, where an interrupted file
//! resumes, and whether an upload arrived whole.

use crate::ftp::{Entry, Error};
use crate::util::{base_name, join_local, join_remote, part_path};

/// one file or folder of a transfer: a folder comes before what it holds (after
/// it, for a delete)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    pub local: String,
    pub remote: String,
    pub dir: bool,
    pub size: u64,
}

/// a PC file or folder and everything in it, to upload into `remote_dir`, in
/// name order. Err when `local` doesn't exist.
pub fn local_items(local: &str, remote_dir: &str, items: &mut Vec<Item>) -> Result<(), String> {
    let meta = std::fs::metadata(local).map_err(|_| format!("Local path not found: {local}"))?;
    add_local(local, &join_remote(remote_dir, base_name(local)), meta, items);
    Ok(())
}

fn add_local(local: &str, remote: &str, meta: std::fs::Metadata, items: &mut Vec<Item>) {
    if !meta.is_dir() {
        items.push(Item { local: local.into(), remote: remote.into(), dir: false, size: meta.len() });
        return;
    }
    items.push(Item { local: local.into(), remote: remote.into(), dir: true, size: 0 });
    let mut names: Vec<String> = std::fs::read_dir(local)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    for name in names {
        let path = crate::os::join(local, &name);
        if let Ok(m) = std::fs::metadata(&path) {
            add_local(&path, &join_remote(remote, &name), m, items);
        }
    }
}

/// a server file or folder (`entry`, at `remote`) and everything in it, to
/// download to `local`. `list` lists a folder over the caller's connection.
pub fn remote_items<E: From<Error>>(
    list: &mut dyn FnMut(&str) -> Result<Vec<Entry>, E>,
    remote: &str,
    entry: &Entry,
    local: &str,
    items: &mut Vec<Item>,
) -> Result<(), E> {
    items.push(Item { local: local.into(), remote: remote.into(), dir: entry.dir, size: if entry.dir { 0 } else { entry.size } });
    if entry.dir {
        for child in list(remote)? {
            remote_items(list, &join_remote(remote, &child.name), &child, &join_local(local, &child.name)?, items)?;
        }
    }
    Ok(())
}

/// a server file or folder to delete: what a folder holds first, the folder last
pub fn delete_items<E>(list: &mut dyn FnMut(&str) -> Result<Vec<Entry>, E>, path: &str, dir: bool, items: &mut Vec<Item>) -> Result<(), E> {
    if dir {
        for child in list(path)? {
            delete_items(list, &join_remote(path, &child.name), child.dir, items)?;
        }
    }
    items.push(Item { local: String::new(), remote: path.into(), dir, size: 0 });
    Ok(())
}

/// where an interrupted upload resumes: what the server already has (`have`),
/// unless that is more than the file
pub fn upload_offset(have: Option<u64>, it: &Item) -> u64 {
    have.filter(|h| *h <= it.size).unwrap_or(0)
}

/// where an interrupted download resumes: what its .part file holds
pub fn download_offset(it: &Item) -> u64 {
    std::fs::metadata(part_path(&it.local)).map_or(0, |m| it.size.min(m.len()))
}

/// after an upload, with the server's size of it: Err(why) when that isn't the
/// file's (a server that gives no size has nothing to check)
pub fn check_uploaded(have: Option<u64>, it: &Item) -> Result<(), String> {
    match have {
        Some(h) if h != it.size => Err(format!("{} has {h} of {} bytes on the server", it.remote, it.size)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> String {
        let p = std::env::temp_dir().join(format!("baguette-plan-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn local_tree_in_name_order() {
        let base = temp("local");
        let top = crate::os::join(&base, "top");
        std::fs::create_dir_all(crate::os::join(&top, "b dir")).unwrap();
        std::fs::write(crate::os::join(&top, "c.txt"), b"ccc").unwrap();
        std::fs::write(crate::os::join(&top, "a.txt"), b"a").unwrap();
        let mut items = Vec::new();
        local_items(&top, "/sd", &mut items).unwrap();
        let got: Vec<(&str, bool, u64)> = items.iter().map(|i| (i.remote.as_str(), i.dir, i.size)).collect();
        assert_eq!(got, [("/sd/top", true, 0), ("/sd/top/a.txt", false, 1), ("/sd/top/b dir", true, 0), ("/sd/top/c.txt", false, 3)]);
        assert!(local_items(&crate::os::join(&base, "gone"), "/sd", &mut items).is_err_and(|e| e.contains("not found")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remote_tree_and_delete_order() {
        let tree = |p: &str| -> Result<Vec<Entry>, Error> {
            Ok(match p {
                "/a" => vec![Entry { name: "f".into(), size: 4, ..Default::default() }, Entry { name: "s".into(), dir: true, ..Default::default() }],
                "/a/s" => vec![Entry { name: "g".into(), size: 2, ..Default::default() }],
                _ => Vec::new(),
            })
        };
        let mut items = Vec::new();
        let top = Entry { name: "a".into(), dir: true, ..Default::default() };
        remote_items(&mut |p| tree(p), "/a", &top, "/pc/a", &mut items).unwrap();
        let got: Vec<(&str, &str)> = items.iter().map(|i| (i.remote.as_str(), i.local.as_str())).collect();
        assert_eq!(got, [("/a", "/pc/a"), ("/a/f", "/pc/a/f"), ("/a/s", "/pc/a/s"), ("/a/s/g", "/pc/a/s/g")]);
        let mut del = Vec::new();
        delete_items(&mut |p| tree(p), "/a", true, &mut del).unwrap();
        let order: Vec<&str> = del.iter().map(|i| i.remote.as_str()).collect();
        assert_eq!(order, ["/a/f", "/a/s/g", "/a/s", "/a"], "contents first, folders last");
    }

    #[test]
    fn resume_and_size_check() {
        let it = Item { local: "/nowhere/x".into(), remote: "/x".into(), dir: false, size: 10 };
        assert_eq!(upload_offset(Some(4), &it), 4);
        assert_eq!(upload_offset(Some(11), &it), 0, "bigger than the file: start again");
        assert_eq!(upload_offset(None, &it), 0);
        assert_eq!(download_offset(&it), 0, "no part file");
        assert!(check_uploaded(Some(10), &it).is_ok());
        assert!(check_uploaded(None, &it).is_ok());
        assert!(check_uploaded(Some(9), &it).is_err_and(|e| e.contains("9 of 10")));
    }
}
