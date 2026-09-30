/// Rewrites shared by every X/Twitter target.
const COMMON_RULES: &[(&str, &str)] = &[
    ("bsky.app", "fxbsky.app"),
    ("instagram.com", "instagram7.com"),
    ("tiktok.com", "tnktok.com"),
];

/// X/Twitter (original host, embed-friendly host) rules per target.
pub(crate) const FIXUP_RULES: &[(&str, &str)] =
    &[("twitter.com", "fxtwitter.com"), ("x.com", "fixupx.com")];
pub(crate) const BOY_RULES: &[(&str, &str)] =
    &[("twitter.com", "boypussyx.com"), ("x.com", "boypussyx.com")];
pub(crate) const MPREG_RULES: &[(&str, &str)] =
    &[("twitter.com", "mpregx.com"), ("x.com", "mpregx.com")];

fn rules() -> &'static [(&'static str, &'static str)] {
    crate::config::x_target().rules()
}

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
fn host_matches(host: &str, domain: &str) -> bool {
    if host.eq_ignore_ascii_case(domain) {
        return true;
    }
    let Some(suffix_start) = host.len().checked_sub(domain.len()) else {
        return false;
    };
    suffix_start > 0
        && host.as_bytes().get(suffix_start - 1) == Some(&b'.')
        && host
            .get(suffix_start..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(domain))
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

/// Same as [`transform_clipboard`] but with an explicit X/Twitter rule set (for
/// tests); the shared Bluesky/Instagram/TikTok rules always apply.
pub(crate) fn transform_clipboard_with(text: &str, x_rules: &[(&str, &str)]) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output).
    let scheme_len = if trimmed
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        8
    } else if trimmed
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
    {
        7
    } else {
        0
    };
    let (scheme, after_scheme) = trimmed.split_at(scheme_len);

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
    // Without a scheme, `@` means an email address (or `mailto:`), not user-info.
    if scheme_len == 0 && authority.contains('@') {
        return None;
    }
    let (host_start, host_end) = authority_host_range(authority)?;
    let host = &authority[host_start..host_end];

    for &(domain, replacement) in x_rules.iter().chain(COMMON_RULES) {
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
    transform_text_links(text).map(|(out, _)| out)
}

/// One link that was rewritten: the URL as copied and its embed-friendly form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewrite {
    pub original: String,
    pub embed: String,
}

/// Like [`transform_text`], but also reports every link that was rewritten.
pub fn transform_text_links(text: &str) -> Option<(String, Vec<Rewrite>)> {
    // Fast path: the whole input is one clean URL (trims surrounding ws).
    if let Some(out) = transform_clipboard(text) {
        let link = Rewrite {
            original: text.trim().to_string(),
            embed: out.clone(),
        };
        return Some((out, vec![link]));
    }
    transform_embedded(text)
}

/// Rewrite one whitespace-delimited token, tolerating wrapping punctuation such
/// as `(url)`, `[text](url)`, `"url"`, `<url>` or a trailing `.` / `,`.
fn transform_token(token: &str) -> Option<(String, Rewrite)> {
    if let Some(out) = transform_clipboard(token) {
        let link = Rewrite {
            original: token.to_string(),
            embed: out.clone(),
        };
        return Some((out, link));
    }

    // Prefix: everything before an explicit scheme, else leading opener chars.
    let lower = token.to_ascii_lowercase();
    let prefix_len = match (lower.find("https://"), lower.find("http://")) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => token
            .find(|c: char| !matches!(c, '(' | '[' | '<' | '"' | '\''))
            .unwrap_or(token.len()),
    };
    let (prefix, rest) = token.split_at(prefix_len);

    // Suffix: peel trailing punctuation. A `)` is only peeled when unbalanced,
    // so `x.com/wiki/A_(b)` stays intact.
    let mut end = rest.len();
    while let Some(last) = rest[..end].chars().next_back() {
        let peel = match last {
            ']' | '>' | '"' | '\'' | ',' | '.' | '!' | '?' | ';' | ':' => true,
            ')' => {
                let head = &rest[..end];
                head.matches(')').count() > head.matches('(').count()
            }
            _ => false,
        };
        if !peel {
            break;
        }
        end -= last.len_utf8();
    }
    let (core, suffix) = rest.split_at(end);
    if core.is_empty() || core.len() == token.len() {
        return None;
    }

    transform_clipboard(core).map(|rewritten| {
        let link = Rewrite {
            original: core.to_string(),
            embed: rewritten.clone(),
        };
        (format!("{prefix}{rewritten}{suffix}"), link)
    })
}

/// Scan `text` for URLs embedded in prose and rewrite each one in place.
/// Whitespace (spaces, tabs, newlines, …) splits tokens and is preserved
/// verbatim; each non-whitespace token is offered to [`transform_clipboard`].
/// Returns `None` when no token changed (so the clipboard is left alone and we
/// avoid a re-write loop).
fn transform_embedded(text: &str) -> Option<(String, Vec<Rewrite>)> {
    // Cheap dry run: most clipboard changes contain no supported URL, so
    // detect that up front and return without allocating in that common case.
    if !text
        .split_whitespace()
        .any(|token| transform_token(token).is_some())
    {
        return None;
    }

    // Rebuild the text, rewriting each matching token and preserving all
    // surrounding whitespace verbatim.
    let mut out = String::with_capacity(text.len() + 8);
    let mut links = Vec::new();
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
        match transform_token(&rest[..end]) {
            Some((rewritten, link)) => {
                out.push_str(&rewritten);
                links.push(link);
            }
            None => out.push_str(&rest[..end]),
        }
        rest = &rest[end..];
    }
    Some((out, links))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpregx_rules_rewrite_x_and_twitter() {
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", MPREG_RULES),
            Some("https://mpregx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mobile.twitter.com/user/status/1", MPREG_RULES),
            Some("https://mobile.mpregx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mpregx.com/user/status/1", MPREG_RULES),
            None
        );
    }

    #[test]
    fn reports_each_rewritten_link() {
        let (out, links) =
            transform_text_links("a (https://x.com/u/status/1) b <https://bsky.app/p/2> c")
                .unwrap();
        assert_eq!(
            out,
            "a (https://fixupx.com/u/status/1) b <https://fxbsky.app/p/2> c"
        );
        assert_eq!(
            links,
            vec![
                Rewrite {
                    original: "https://x.com/u/status/1".into(),
                    embed: "https://fixupx.com/u/status/1".into(),
                },
                Rewrite {
                    original: "https://bsky.app/p/2".into(),
                    embed: "https://fxbsky.app/p/2".into(),
                },
            ]
        );
    }

    #[test]
    fn single_link_is_reported_trimmed() {
        let (_, links) = transform_text_links("  https://x.com/u/status/1\n").unwrap();
        assert_eq!(links[0].original, "https://x.com/u/status/1");
    }

    #[test]
    fn every_target_has_distinct_rules() {
        use crate::config::XTarget;
        for (i, a) in XTarget::ALL.iter().enumerate() {
            for b in &XTarget::ALL[i + 1..] {
                assert_ne!(a.rules(), b.rules());
            }
        }
    }

    #[test]
    fn common_rules_apply_to_every_target() {
        for rules in [FIXUP_RULES, BOY_RULES, MPREG_RULES] {
            assert_eq!(
                transform_clipboard_with("https://www.tiktok.com/@a/video/1", rules),
                Some("https://www.tnktok.com/@a/video/1".to_string())
            );
        }
    }

    #[test]
    fn email_addresses_are_not_rewritten() {
        assert_eq!(
            transform_clipboard_with("press@twitter.com", FIXUP_RULES),
            None
        );
        assert_eq!(
            transform_clipboard_with("mailto:press@x.com", FIXUP_RULES),
            None
        );
    }
}
