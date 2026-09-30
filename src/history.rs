//! History of converted links, with the embed data (title / description) of
//! each one so the user can tell what they are.
//!
//! Entries are kept newest first, capped at [`MAX_ENTRIES`], and persisted as
//! JSON under the OS config dir. The embed metadata is read from the OpenGraph
//! tags of the rewritten URL by a single background worker, so the clipboard
//! loop never waits on the network. A link whose read fails (request error,
//! redirect to another host, no embed data) is removed again.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::transform::Rewrite;

/// Most entries kept; the oldest are dropped first.
const MAX_ENTRIES: usize = 100;
/// Entries shown in the tray's Recent submenu.
pub const MENU_ENTRIES: usize = 10;
const MAX_LABEL_CHARS: usize = 60;
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
    let text = match (non_empty(&entry.title), non_empty(&entry.description)) {
        (Some(title), Some(description)) => format!("{title} — {description}"),
        (Some(only), None) | (None, Some(only)) => only.to_string(),
        (None, None) => entry.embed.clone(),
    };
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_LABEL_CHARS {
        return one_line;
    }
    let mut label: String = one_line.chars().take(MAX_LABEL_CHARS - 1).collect();
    label.push('…');
    label
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
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
/// errored, it was redirected to another host (a bare `fixupx.com` bounces to
/// GitHub), or the page has no embed data.
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

fn host_of(url: &str) -> Option<String> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    uri.host().map(str::to_ascii_lowercase)
}

/// Whether a fetched page is a usable embed of `embed`: it stayed on the same
/// host and carries a title or description.
fn is_embed(embed: &str, final_uri: &str, og: &Og) -> bool {
    host_of(embed).is_some_and(|host| host_of(final_uri).as_deref() == Some(host.as_str()))
        && (og.title.is_some() || og.description.is_some())
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

#[derive(Debug, Default, PartialEq, Eq)]
struct Og {
    site: Option<String>,
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
}

/// Pull the embed data out of an HTML page: `og:*` meta tags, falling back to
/// `twitter:*` tags and then `<title>`.
fn parse_og(html: &str) -> Og {
    let lower = html.to_ascii_lowercase();
    let mut og = Og::default();
    let (mut tw_title, mut tw_description) = (None, None);

    let mut from = 0;
    while let Some(found) = lower[from..].find("<meta") {
        let start = from + found + "<meta".len();
        let end = tag_end(html, start);
        from = end;
        let (mut key, mut content) = (None, None);
        for (name, value) in attributes(&html[start..end]) {
            match name.as_str() {
                "property" | "name" => key = Some(value.to_ascii_lowercase()),
                "content" => content = Some(decode_entities(&value)),
                _ => {}
            }
        }
        let (Some(key), Some(content)) = (key, content) else {
            continue;
        };
        let slot = match key.as_str() {
            "og:title" => &mut og.title,
            "og:description" => &mut og.description,
            "og:image" => &mut og.image,
            "og:site_name" => &mut og.site,
            "twitter:title" => &mut tw_title,
            "twitter:description" => &mut tw_description,
            _ => continue,
        };
        if slot.is_none() && !content.is_empty() {
            *slot = Some(content);
        }
    }

    og.title = og.title.or(tw_title).or_else(|| page_title(html, &lower));
    og.description = og.description.or(tw_description);
    og
}

fn page_title(html: &str, lower: &str) -> Option<String> {
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let title = decode_entities(html[start..end].trim());
    (!title.is_empty()).then_some(title)
}

/// Index of the `>` closing the tag whose attributes start at `from` (quotes
/// may contain `>`), or the end of the input.
fn tag_end(html: &str, from: usize) -> usize {
    let mut quote = None;
    for (offset, c) in html[from..].char_indices() {
        match (quote, c) {
            (None, '>') => return from + offset,
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            _ => {}
        }
    }
    html.len()
}

/// Attribute `(lowercase name, raw value)` pairs of a tag body.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return out;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let (value, remaining) = match after_eq.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let body = &after_eq[1..];
                match body.find(q) {
                    Some(close) => (&body[..close], &body[close + 1..]),
                    None => (body, ""),
                }
            }
            _ => {
                let end = after_eq.find(char::is_whitespace).unwrap_or(after_eq.len());
                (&after_eq[..end], &after_eq[end..])
            }
        };
        if !name.is_empty() {
            out.push((name, value.to_string()));
        }
        rest = remaining;
    }
}

/// Decode the named and numeric HTML entities found in meta content.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &rest[1..semi];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name.strip_prefix('#').and_then(|num| {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }),
            }?;
            Some((c, semi + 1))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
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
    fn embeds_must_stay_on_host_and_have_data() {
        let embed = "https://fixupx.com/";
        assert!(is_embed(
            embed,
            "https://fixupx.com/u/status/1",
            &og_with_title()
        ));
        assert!(is_embed(embed, "https://FixupX.com/", &og_with_title()));
        assert!(!is_embed(
            embed,
            "https://github.com/FxEmbed/FxEmbed",
            &og_with_title()
        ));
        assert!(!is_embed(embed, embed, &Og::default()));
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

    #[test]
    fn parses_a_fxtwitter_style_page() {
        let html = r#"<html><head>
            <meta property="og:site_name" content="FixupX">
            <meta property="og:title" content="Jane (@jane)" />
            <meta property="og:description" content="Fish &amp; chips &#39;n&#x27; peas &gt; all">
            <meta property="og:image" content="https://pbs.example/img.jpg">
            <title>ignored</title></head></html>"#;
        assert_eq!(
            parse_og(html),
            Og {
                site: Some("FixupX".into()),
                title: Some("Jane (@jane)".into()),
                description: Some("Fish & chips 'n' peas > all".into()),
                image: Some("https://pbs.example/img.jpg".into()),
            }
        );
    }

    #[test]
    fn parses_reversed_attributes_and_single_quotes() {
        let html = "<META CONTENT='Hi > there' NAME='twitter:title'><meta content=x name=twitter:description>";
        let og = parse_og(html);
        assert_eq!(og.title.as_deref(), Some("Hi > there"));
        assert_eq!(og.description.as_deref(), Some("x"));
    }

    #[test]
    fn falls_back_to_the_page_title_and_handles_empty_pages() {
        assert_eq!(
            parse_og("<title> A &amp; B </title>").title.as_deref(),
            Some("A & B")
        );
        assert_eq!(parse_og("<html></html>"), Og::default());
        assert_eq!(parse_og(""), Og::default());
    }

    #[test]
    fn entity_decoding_leaves_unknown_entities_alone() {
        assert_eq!(
            decode_entities("a &nope; b & c &#xZZ;"),
            "a &nope; b & c &#xZZ;"
        );
    }

    #[test]
    fn labels_are_one_line_and_bounded() {
        let mut entry = Entry {
            embed: "https://fixupx.com/a".into(),
            ..Entry::default()
        };
        assert_eq!(menu_label(&entry), "https://fixupx.com/a");
        entry.title = Some("Jane (@jane)".into());
        entry.description = Some("line one\nline two".into());
        assert_eq!(menu_label(&entry), "Jane (@jane) — line one line two");
        entry.description = Some("x".repeat(200));
        let label = menu_label(&entry);
        assert_eq!(label.chars().count(), MAX_LABEL_CHARS);
        assert!(label.ends_with('…'));
    }
}
