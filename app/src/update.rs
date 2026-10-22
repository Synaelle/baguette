// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! Is there a newer Baguette? One request to GitHub's latest-release API when
//! the window opens (unless turned off). Nothing is downloaded: the window
//! offers to open the release page in the browser.

use std::cmp::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const LATEST: &str = "https://api.github.com/repos/Synaelle/baguette/releases/latest";
pub const RELEASES: &str = "https://github.com/Synaelle/baguette/releases";

#[derive(Serialize, Clone)]
pub struct Update {
    pub version: String, // "2.0"
    pub url: String,     // its release page
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

/// this build's version without its dev mark: "2.0.1-abc1234-dev" -> "2.0.1"
fn local_base(v: &str) -> &str {
    match v.strip_suffix("-dev") {
        Some(stripped) => stripped.rsplit_once('-').map_or(stripped, |(head, _hash)| head),
        None => v,
    }
}

/// "2.0", "2.0.1", "2.0-RC3": (major, minor, patch, release candidate or None
/// for a final release)
fn parse(v: &str) -> Option<(u32, u32, u32, Option<u32>)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let (main, pre) = match v.split_once('-') {
        Some((m, p)) => (m, Some(p)),
        None => (v, None),
    };
    let nums: Vec<u32> = main.split('.').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    let rc = match pre {
        None => None,
        Some(p) if p.len() > 2 && p[..2].eq_ignore_ascii_case("rc") => Some(p[2..].parse().ok()?),
        Some(_) => return None,
    };
    let get = |i: usize| nums.get(i).copied().unwrap_or(0);
    if nums.is_empty() || nums.len() > 3 {
        return None;
    }
    Some((get(0), get(1), get(2), rc))
}

/// is this build (`local`, with its build number) older than the release `tag`?
fn older(local: &str, tag: &str) -> bool {
    let (Some(a), Some(b)) = (parse(local_base(local)), parse(tag)) else { return false };
    match (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)) {
        Ordering::Less => true,
        Ordering::Greater => false,
        // same version: a release candidate comes before the final release
        Ordering::Equal => match (a.3, b.3) {
            (Some(x), Some(y)) => x < y,
            (Some(_), None) => true,
            _ => false,
        },
    }
}

/// Some(update) when GitHub has a newer release than `current`; None when
/// this one is up to date, newer, or GitHub can't be reached
pub fn check(current: &str) -> Option<Update> {
    if current == "dev" {
        return None;
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .user_agent(format!("Baguette/{current}"))
        .build()
        .new_agent();
    // tests answer from a local server
    let latest = std::env::var("BAGUETTE_UPDATE_URL").unwrap_or_else(|_| LATEST.into());
    let mut resp = agent.get(&latest).header("Accept", "application/vnd.github+json").call().ok()?;
    let r: Release = resp.body_mut().read_json().ok()?;
    if r.draft || !older(current, &r.tag_name) {
        return None;
    }
    let version = r.tag_name.trim_start_matches(['v', 'V']).to_string();
    let url = if r.html_url.starts_with(RELEASES) { r.html_url } else { RELEASES.into() };
    Some(Update { version, url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(local_base("2.0.1"), "2.0.1");
        assert_eq!(local_base("2.0.1-abc1234-dev"), "2.0.1");
        assert!(older("2.0.1", "v2.0.2"));
        assert!(older("2.0.1", "v2.1"));
        assert!(older("2.0-RC3", "v2.0"), "an RC before the final release");
        assert!(older("2.0-RC2", "v2.0-RC3"));
        assert!(!older("2.0.1", "v2.0.1"));
        assert!(!older("2.0.1-abc1234-dev", "v2.0.1"), "a dev build of the same version");
        assert!(!older("2.1.0", "v2.0.1"));
        assert!(!older("2.0.1", "v2.0"));
        assert!(!older("2.0.1", "nightly"), "unknown tags are ignored");
    }
}
