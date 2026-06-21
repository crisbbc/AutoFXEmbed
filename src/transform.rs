/// (original host, FxEmbed host) rewrite rules, in priority order.
const RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
];

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{}", domain))
}

/// If `text` is a single X/Twitter/Bluesky URL, return the FxEmbed form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output).
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http://", rest)
    } else {
        ("", trimmed)
    };

    // Only rewrite clean single URLs (no internal whitespace of any kind —
    // spaces, tabs, newlines, …). Anything with internal whitespace is left
    // for `transform_embedded` to handle token by token.
    if after_scheme.chars().any(|c| c.is_whitespace()) {
        return None;
    }

    // Host = everything up to the first '/' (or the whole thing if no path).
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..path_start];

    for &(domain, replacement) in RULES {
        if host_matches(host, domain) {
            let new_host = host.replacen(domain, replacement, 1);
            let mut result = String::with_capacity(trimmed.len() + 4);
            result.push_str(scheme);
            result.push_str(&new_host);
            result.push_str(&after_scheme[path_start..]);
            return Some(result);
        }
    }
    None
}

/// If `text` is a single supported URL, OR contains one or more supported URLs
/// embedded in surrounding text, return `text` with every such URL rewritten to
/// its FxEmbed form (surrounding text and whitespace preserved). Otherwise
/// return `None` (leave the clipboard untouched). Never rewrites an
/// already-transformed host.
///
/// Single-link inputs are trimmed and handled by [`transform_clipboard`]; prose
/// with embedded links preserves all surrounding text verbatim.
pub fn transform_text(text: &str) -> Option<String> {
    // Fast path: the whole input is one clean URL (trims surrounding ws).
    if let Some(out) = transform_clipboard(text) {
        return Some(out);
    }
    transform_embedded(text)
}

/// Scan `text` for URLs embedded in prose and rewrite each one in place.
/// Whitespace (spaces, tabs, newlines, …) splits tokens and is preserved
/// verbatim; each non-whitespace token is offered to [`transform_clipboard`].
/// Returns `None` when no token changed (so the clipboard is left alone and we
/// avoid a re-write loop).
fn transform_embedded(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut changed = false;
    let mut rest = text;

    while !rest.is_empty() {
        // Leading whitespace run: copy it verbatim.
        let after_ws = rest.trim_start_matches(|c: char| c.is_whitespace());
        let ws_len = rest.len() - after_ws.len();
        out.push_str(&rest[..ws_len]);
        rest = after_ws;
        if rest.is_empty() {
            break;
        }
        // Next token: everything up to the next whitespace char (or end).
        let tok_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let token = &rest[..tok_end];
        match transform_clipboard(token) {
            Some(rewritten) => {
                out.push_str(&rewritten);
                changed = true;
            }
            None => out.push_str(token),
        }
        rest = &rest[tok_end..];
    }

    if changed { Some(out) } else { None }
}
