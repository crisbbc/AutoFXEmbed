/// (original host, embed-friendly host) rewrite rules, in priority order.
pub(crate) const FIXUP_RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
    ("instagram.com", "instagram7.com"),
    ("tiktok.com", "tnktok.com"),
];
pub(crate) const BOY_RULES: &[(&str, &str)] = &[
    ("twitter.com", "boypussyx.com"),
    ("x.com", "boypussyx.com"),
    ("bsky.app", "fxbsky.app"),
    ("instagram.com", "instagram7.com"),
    ("tiktok.com", "tnktok.com"),
];

fn rules() -> &'static [(&'static str, &'static str)] {
    if crate::config::is_boypussyx() {
        BOY_RULES
    } else {
        FIXUP_RULES
    }
}

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
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

/// If `text` is a single supported social-media URL, return its embed-friendly form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    transform_clipboard_with(text, rules())
}

/// Same as [`transform_clipboard`] but with an explicit rule set (for tests).
pub(crate) fn transform_clipboard_with(text: &str, rules: &[(&str, &str)]) -> Option<String> {
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

    for &(domain, replacement) in rules {
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
    // Cheap dry run: most clipboard changes contain no supported URL, so
    // detect that up front and return without allocating in that common case.
    if !text
        .split_whitespace()
        .any(|token| transform_clipboard(token).is_some())
    {
        return None;
    }

    // Rebuild the text, rewriting each matching token and preserving all
    // surrounding whitespace verbatim.
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while !rest.is_empty() {
        // Leading whitespace run: copy it verbatim.
        let after_ws = rest.trim_start_matches(char::is_whitespace);
        out.push_str(&rest[..rest.len() - after_ws.len()]);
        rest = after_ws;
        if rest.is_empty() {
            break;
        }
        // Next token: everything up to the next whitespace char (or end).
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        match transform_clipboard(&rest[..end]) {
            Some(rewritten) => out.push_str(&rewritten),
            None => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    Some(out)
}
