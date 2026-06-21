/// (original host, FxEmbed host) rewrite rules, in priority order.
const RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
];

/// If `text` is a single X/Twitter/Bluesky URL, return the FxEmbed form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output). Only https for now.
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else {
        // No recognised scheme -> treat the whole string as the host+path.
        ("", trimmed)
    };

    // Host = everything up to the first '/' (or the whole thing if no path).
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..path_start];

    for &(domain, replacement) in RULES {
        if host == domain {
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
