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

/// Return the byte range of the hostname within a URL authority.
///
/// The supported hosts are ordinary DNS names, so an optional user-info prefix
/// and port can be handled without pulling in a full URL parser.
fn authority_host_range(authority: &str) -> Option<(usize, usize)> {
    let host_start = authority.rfind('@').map_or(0, |index| index + 1);
    let host_port = &authority[host_start..];
    let host_len = host_port.find(':').unwrap_or(host_port.len());
    if host_len == 0 {
        None
    } else {
        Some((host_start, host_start + host_len))
    }
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

    // Authority = everything up to the path, query, or fragment.
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let (host_start, host_end) = authority_host_range(authority)?;
    let host = &authority[host_start..host_end];

    for &(domain, replacement) in RULES {
        if host_matches(host, domain) {
            let domain_start = host_end - domain.len();
            let mut result =
                String::with_capacity(trimmed.len() + replacement.len() - domain.len());
            result.push_str(scheme);
            result.push_str(&authority[..domain_start]);
            result.push_str(replacement);
            result.push_str(&authority[host_end..]);
            result.push_str(&after_scheme[authority_end..]);
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

    if changed {
        Some(out)
    } else {
        None
    }
}
