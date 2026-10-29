//! History of converted links, with the embed data (title / description) of
//! each one so the user can tell what they are.
//!
//! Entries are kept newest first, capped at [`MAX_ENTRIES`], and persisted as
//! JSON under the OS config dir. The embed metadata is read from the OpenGraph
//! tags of the rewritten URL by a single background worker, so the clipboard
//! loop never waits on the network. A link whose read fails (request error,
//! redirect to an unrelated page, no embed data) is removed again.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use autofxembed_core::embed::{is_embed, parse_og, Og};

use crate::transform::Rewrite;

/// Most entries kept; the oldest are dropped first.
const MAX_ENTRIES: usize = 100;
/// Entries shown in the tray's Recent submenu.
pub const MENU_ENTRIES: usize = 10;
const MAX_BODY_BYTES: u64 = 512 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// The fx* hosts only serve their OpenGraph page to link-preview bots.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The link as it was copied.
    pub original: String,
    /// The embed-friendly link it was rewritten to.
    pub embed: String,
    /// Unix time (seconds) of the latest copy.
    pub copied_at: u64,
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    /// True once the metadata fetch has succeeded; entries whose fetch fails
    /// are dropped from the history.
    #[serde(default)]
    pub fetched: bool,
}

static HISTORY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static FETCH_QUEUE: OnceLock<Sender<String>> = OnceLock::new();
type ChangeHook = Box<dyn Fn() + Send + Sync>;
static ON_CHANGE: OnceLock<ChangeHook> = OnceLock::new();

fn history_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("autofxembed").join("history.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn lock() -> std::sync::MutexGuard<'static, Vec<Entry>> {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Register a callback run whenever the history changes from a background
/// thread (used by the Linux tray to refresh its menu). Only the first call wins.
pub fn set_on_change(hook: impl Fn() + Send + Sync + 'static) {
    let _ = ON_CHANGE.set(Box::new(hook));
}

fn notify_changed() {
    if let Some(hook) = ON_CHANGE.get() {
        hook();
    }
}

/// Load the persisted history (call once at startup). Missing/corrupt file →
/// empty history. Entries whose metadata never arrived are fetched again.
pub fn load() {
    let Some(path) = history_path() else {
        return;
    };
    let entries: Vec<Entry> = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
            eprintln!("AutoFxEmbed: ignoring unreadable history: {error}");
            Vec::new()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            eprintln!("AutoFxEmbed: unable to read history: {error}");
            return;
        }
    };
    let pending: Vec<String> = entries
        .iter()
        .filter(|entry| !entry.fetched)
        .map(|entry| entry.embed.clone())
        .collect();
    *lock() = entries;
    for embed in pending {
        queue_fetch(embed);
    }
}

fn save(entries: &[Entry]) {
    let Some(path) = history_path() else {
        return;
    };
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(entries).map_err(std::io::Error::other)?;
        // Write-then-rename so a crash never leaves a half-written file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)
    })();
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: unable to save history: {error}");
    }
}

/// Add `link` at the top of `entries` (moving an existing entry for the same
/// embed URL instead of duplicating it) and trim to [`MAX_ENTRIES`].
/// Returns true if the entry still needs its metadata fetched.
fn push(entries: &mut Vec<Entry>, link: &Rewrite, copied_at: u64) -> bool {
    let mut entry = match entries.iter().position(|e| e.embed == link.embed) {
        Some(index) => entries.remove(index),
        None => Entry {
            embed: link.embed.clone(),
            ..Entry::default()
        },
    };
    entry.original = link.original.clone();
    entry.copied_at = copied_at;
    let needs_fetch = !entry.fetched;
    entries.insert(0, entry);
    entries.truncate(MAX_ENTRIES);
    needs_fetch
}

/// Record links that were just rewritten on the clipboard.
pub fn record(links: &[Rewrite]) {
    if links.is_empty() {
        return;
    }
    let time = now();
    let mut to_fetch = Vec::new();
    {
        let mut entries = lock();
        // Reverse so the first link of the text ends up on top.
        for link in links.iter().rev() {
            if push(&mut entries, link, time) {
                to_fetch.push(link.embed.clone());
            }
        }
        save(&entries);
    }
    for embed in to_fetch {
        queue_fetch(embed);
    }
    notify_changed();
}

/// The newest `n` entries.
pub fn recent(n: usize) -> Vec<Entry> {
    lock().iter().take(n).cloned().collect()
}

/// Forget every entry.
pub fn clear() {
    let mut entries = lock();
    entries.clear();
    save(&entries);
    drop(entries);
    notify_changed();
}

/// One-line menu label: `title — description`, or the embed URL until the
/// metadata is known.
pub fn menu_label(entry: &Entry) -> String {
    autofxembed_core::embed::label(
        entry.title.as_deref(),
        entry.description.as_deref(),
        &entry.embed,
    )
}

// ---------------------------------------------------------------------------
// Metadata fetching
// ---------------------------------------------------------------------------

fn queue_fetch(embed: String) {
    let sender = FETCH_QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(FETCH_TIMEOUT))
                .user_agent(USER_AGENT)
                .build()
                .into();
            while let Ok(embed) = receiver.recv() {
                let og = fetch_og(&agent, &embed);
                store_metadata(&embed, og);
            }
        });
        sender
    });
    let _ = sender.send(embed);
}

/// Read the embed data of `url`. `None` means the read failed: the request
/// errored, it was redirected to an unrelated page (a bare `fixupx.com` bounces
/// to GitHub), or the page has no embed data.
fn fetch_og(agent: &ureq::Agent, url: &str) -> Option<Og> {
    use ureq::ResponseExt;
    let response = agent.get(url).call().ok()?;
    let final_uri = response.get_uri().to_string();
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_BODY_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let og = parse_og(&String::from_utf8_lossy(&bytes));
    is_embed(url, &final_uri, &og).then_some(og)
}

/// Apply a finished fetch to the list: fill in the metadata, or drop the entry
/// when the read failed. Returns true if the list changed.
fn apply_metadata(entries: &mut Vec<Entry>, embed: &str, og: Option<Og>) -> bool {
    // The entry may have been cleared or trimmed while the fetch ran.
    let Some(index) = entries.iter().position(|e| e.embed == embed) else {
        return false;
    };
    match og {
        Some(og) => {
            let entry = &mut entries[index];
            entry.fetched = true;
            entry.site = og.site;
            entry.title = og.title;
            entry.description = og.description;
            entry.image = og.image;
        }
        None => {
            entries.remove(index);
        }
    }
    true
}

fn store_metadata(embed: &str, og: Option<Og>) {
    {
        let mut entries = lock();
        if !apply_metadata(&mut entries, embed, og) {
            return;
        }
        save(&entries);
    }
    notify_changed();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(n: u32) -> Rewrite {
        Rewrite {
            original: format!("https://x.com/u/status/{n}"),
            embed: format!("https://fixupx.com/u/status/{n}"),
        }
    }

    #[test]
    fn push_moves_duplicates_to_the_top_and_keeps_metadata() {
        let mut entries = Vec::new();
        assert!(push(&mut entries, &link(1), 10));
        assert!(push(&mut entries, &link(2), 20));
        entries[1].fetched = true;
        entries[1].title = Some("kept".into());
        assert!(!push(&mut entries, &link(1), 30));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].embed, link(1).embed);
        assert_eq!(entries[0].copied_at, 30);
        assert_eq!(entries[0].title.as_deref(), Some("kept"));
    }

    fn og_with_title() -> Og {
        Og {
            title: Some("Jane (@jane)".into()),
            ..Og::default()
        }
    }

    #[test]
    fn failed_reads_drop_the_entry() {
        let mut entries = Vec::new();
        push(&mut entries, &link(1), 10);
        push(&mut entries, &link(2), 20);
        assert!(apply_metadata(&mut entries, &link(1).embed, None));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].embed, link(2).embed);
        // Already gone: nothing to do.
        assert!(!apply_metadata(&mut entries, &link(1).embed, None));
    }

    #[test]
    fn successful_reads_fill_in_the_entry() {
        let mut entries = Vec::new();
        push(&mut entries, &link(1), 10);
        assert!(apply_metadata(
            &mut entries,
            &link(1).embed,
            Some(og_with_title())
        ));
        assert!(entries[0].fetched);
        assert_eq!(entries[0].title.as_deref(), Some("Jane (@jane)"));
    }

    #[test]
    fn push_truncates_to_the_cap() {
        let mut entries = Vec::new();
        for n in 0..(MAX_ENTRIES as u32 + 5) {
            push(&mut entries, &link(n), 0);
        }
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].embed, link(MAX_ENTRIES as u32 + 4).embed);
    }
}
