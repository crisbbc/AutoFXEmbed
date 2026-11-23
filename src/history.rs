//! History of converted links, with the embed data (title / description) of
//! each one so the user can tell what they are.
//!
//! Entries are kept newest first, capped at [`MAX_ENTRIES`], and persisted as
//! JSON under the OS config dir. The embed metadata is read from the OpenGraph
//! tags of the rewritten URL by a single background worker, so the clipboard
//! loop never waits on the network. A link whose page is genuinely not an embed
//! (4xx / redirect to unrelated page / no OG tags) is removed again; transient
//! network or 5xx/429 errors keep the entry so it can be retried later.

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use autofxembed_core::embed::{is_embed, parse_og, Og};

use crate::storage::{config_file, write_atomic};
use crate::transform::Rewrite;

/// Most entries kept; the oldest are dropped first.
const MAX_ENTRIES: usize = 100;
/// Default number of entries shown in the panel's Recent section.
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
    /// True once the metadata fetch has succeeded.
    #[serde(default)]
    pub fetched: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum FetchOutcome {
    Ok(Og),
    /// Confirmed non-embed (4xx other than 429, unrelated redirect, or no OG data).
    NotEmbed,
    /// Offline, timeout, 429 rate-limit, or 5xx server error: keep and retry later.
    TransientError,
}

static HISTORY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static FETCH_QUEUE: OnceLock<Sender<String>> = OnceLock::new();
type ChangeHook = Box<dyn Fn() + Send + Sync>;
static ON_CHANGE: OnceLock<ChangeHook> = OnceLock::new();

fn history_path() -> Option<PathBuf> {
    config_file("history.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn lock() -> std::sync::MutexGuard<'static, Vec<Entry>> {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner())
}

fn in_flight_insert(embed: &str) -> bool {
    let mut guard = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(HashSet::new)
        .insert(embed.to_string())
}

fn in_flight_remove(embed: &str) {
    let mut guard = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(set) = guard.as_mut() {
        set.remove(embed);
    }
}

/// Register a callback run whenever the history changes (used to push live
/// updates to an open tray panel). Only the first call wins.
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
    *lock() = entries;
    retry_pending();
}

fn save(entries: &[Entry]) {
    let Some(path) = history_path() else {
        return;
    };
    let result = serde_json::to_string(entries)
        .map_err(std::io::Error::other)
        .and_then(|json| write_atomic(&path, &json));
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

/// Retry metadata fetches for any entries that previously failed due to
/// transient errors (e.g. while offline). Deduplicates in-flight URLs.
pub fn retry_pending() {
    let pending: Vec<String> = lock()
        .iter()
        .filter(|entry| !entry.fetched)
        .map(|entry| entry.embed.clone())
        .collect();
    for embed in pending {
        queue_fetch(embed);
    }
}

/// The newest `n` entries.
pub fn recent(n: usize) -> Vec<Entry> {
    lock().iter().take(n).cloned().collect()
}

/// Every stored history entry (up to [`MAX_ENTRIES`]), newest first.
pub fn all() -> Vec<Entry> {
    lock().clone()
}

/// Forget a single entry by its embed URL.
pub fn remove(embed: &str) {
    let mut entries = lock();
    let before = entries.len();
    entries.retain(|e| e.embed != embed);
    if entries.len() == before {
        return;
    }
    save(&entries);
    drop(entries);
    notify_changed();
}

/// Forget every entry.
pub fn clear() {
    let mut entries = lock();
    entries.clear();
    save(&entries);
    drop(entries);
    notify_changed();
}

/// One-line label: `title — description`, or the embed URL until metadata arrives.
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
    if !in_flight_insert(&embed) {
        return;
    }
    let sender = FETCH_QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(FETCH_TIMEOUT))
                .user_agent(USER_AGENT)
                .build()
                .into();
            while let Ok(embed) = receiver.recv() {
                let outcome = fetch_og(&agent, &embed);
                in_flight_remove(&embed);
                store_metadata(&embed, outcome);
            }
        });
        sender
    });
    if sender.send(embed.clone()).is_err() {
        in_flight_remove(&embed);
    }
}

/// Read the embed data of `url`, distinguishing permanent non-embeds from
/// transient offline/5xx/429 failures.
fn fetch_og(agent: &ureq::Agent, url: &str) -> FetchOutcome {
    use ureq::ResponseExt;
    let response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) && code != 429 => {
            return FetchOutcome::NotEmbed;
        }
        Err(_) => return FetchOutcome::TransientError,
    };
    let final_uri = response.get_uri().to_string();
    let mut bytes = Vec::new();
    if response
        .into_body()
        .into_reader()
        .take(MAX_BODY_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return FetchOutcome::TransientError;
    }
    let og = parse_og(&String::from_utf8_lossy(&bytes));
    if is_embed(url, &final_uri, &og) {
        FetchOutcome::Ok(og)
    } else {
        FetchOutcome::NotEmbed
    }
}

/// Apply a finished fetch to the list: fill in the metadata, drop confirmed
/// non-embeds, or leave untouched on transient errors. Returns true if changed.
fn apply_metadata(entries: &mut Vec<Entry>, embed: &str, outcome: FetchOutcome) -> bool {
    // The entry may have been cleared or trimmed while the fetch ran.
    let Some(index) = entries.iter().position(|e| e.embed == embed) else {
        return false;
    };
    match outcome {
        FetchOutcome::Ok(og) => {
            let entry = &mut entries[index];
            entry.fetched = true;
            entry.site = og.site;
            entry.title = og.title;
            entry.description = og.description;
            entry.image = og.image;
            true
        }
        FetchOutcome::NotEmbed => {
            entries.remove(index);
            true
        }
        FetchOutcome::TransientError => false,
    }
}

fn store_metadata(embed: &str, outcome: FetchOutcome) {
    {
        let mut entries = lock();
        if !apply_metadata(&mut entries, embed, outcome) {
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
    fn permanent_non_embeds_drop_the_entry_while_transient_errors_keep_it() {
        let mut entries = Vec::new();
        push(&mut entries, &link(1), 10);
        push(&mut entries, &link(2), 20);
        // Transient error keeps the entry unfetched for later retry.
        assert!(!apply_metadata(
            &mut entries,
            &link(1).embed,
            FetchOutcome::TransientError
        ));
        assert_eq!(entries.len(), 2);
        assert!(!entries[1].fetched);

        // Confirmed non-embed drops it.
        assert!(apply_metadata(
            &mut entries,
            &link(1).embed,
            FetchOutcome::NotEmbed
        ));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].embed, link(2).embed);
        // Already gone: nothing to do.
        assert!(!apply_metadata(
            &mut entries,
            &link(1).embed,
            FetchOutcome::NotEmbed
        ));
    }

    #[test]
    fn successful_reads_fill_in_the_entry() {
        let mut entries = Vec::new();
        push(&mut entries, &link(1), 10);
        assert!(apply_metadata(
            &mut entries,
            &link(1).embed,
            FetchOutcome::Ok(og_with_title())
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
