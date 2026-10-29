/// Settings a rewrite depends on.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    /// X/Twitter `(original host, embed host)` rules, from [`crate::XTarget::rules`]
    /// or built for a custom domain.
    pub x_rules: &'a [(&'a str, &'a str)],
    /// Remove tracking query parameters (see [`crate::trackers`]).
    pub strip_trackers: bool,
}

/// Rewrites shared by every X/Twitter target.
const COMMON_RULES: &[(&str, &str)] = &[("bsky.app", "fxbsky.app"), ("tiktok.com", "tnktok.com")];

/// X/Twitter (original host, embed-friendly host) rules per target.
pub const FIXUP_RULES: &[(&str, &str)] =
    &[("twitter.com", "fxtwitter.com"), ("x.com", "fixupx.com")];
pub const BOY_RULES: &[(&str, &str)] =
    &[("twitter.com", "boypussyx.com"), ("x.com", "boypussyx.com")];
pub const MPREG_RULES: &[(&str, &str)] = &[("twitter.com", "mpregx.com"), ("x.com", "mpregx.com")];
pub const YAOI_RULES: &[(&str, &str)] = &[("twitter.com", "yaoisex.com"), ("x.com", "yaoisex.com")];
pub const FAGGOT_RULES: &[(&str, &str)] =
    &[("twitter.com", "faggotx.com"), ("x.com", "faggotx.com")];

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
pub(crate) fn host_matches(host: &str, domain: &str) -> bool {
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

/// True for X / Twitter itself and every host its links are rewritten to (the
/// built-in targets and the selected custom domain), so already-converted
/// links still get cleaned.
fn is_x_host(host: &str, x_rules: &[(&str, &str)]) -> bool {
    crate::XTarget::ALL
        .iter()
        .flat_map(|target| target.rules())
        .chain(x_rules)
        .any(|&(domain, replacement)| host_matches(host, domain) || host_matches(host, replacement))
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

/// If `text` is a single supported social-media URL, return its embed-friendly
/// form; with tracker removal on, also any `http(s)` URL that carries tracking
/// parameters, cleaned. Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str, opts: &Options) -> Option<String> {
    rewrite_url(text, opts).map(|(url, _)| url)
}

/// [`transform_clipboard`], also telling whether the host was swapped for an
/// embed host (`false`: only trackers were removed).
fn rewrite_url(text: &str, opts: &Options) -> Option<(String, bool)> {
    let x_rules = opts.x_rules;
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
    let tail = &after_scheme[authority_end..];
    let cleaned = opts
        .strip_trackers
        .then(|| crate::trackers::strip(host, is_x_host(host, x_rules), tail))
        .flatten();

    for &(domain, replacement) in x_rules.iter().chain(COMMON_RULES) {
        if host_matches(host, domain) {
            let domain_start = host_end - domain.len();
            let mut result =
                String::with_capacity(trimmed.len() + replacement.len() - domain.len());
            result.push_str(scheme);
            result.push_str(&authority[..domain_start]);
            result.push_str(replacement);
            result.push_str(&authority[host_end..]);
            result.push_str(cleaned.as_deref().unwrap_or(tail));
            return Some((result, true));
        }
    }

    // Any other site: only clean it, and only when it is unmistakably a URL.
    let cleaned = cleaned.filter(|_| scheme_len > 0)?;
    Some((format!("{scheme}{authority}{cleaned}"), false))
}

/// If `text` is a single supported URL, OR contains one or more supported URLs
/// embedded in surrounding text, return `text` with every such URL rewritten to
/// its FxEmbed form (surrounding text and whitespace preserved). Otherwise
/// return `None` (leave the clipboard untouched). Never rewrites an
/// already-transformed host.
///
/// Single-link inputs are trimmed and handled by [`transform_clipboard`]; prose
/// with embedded links preserves all surrounding text verbatim.
pub fn transform_text(text: &str, opts: &Options) -> Option<String> {
    transform_text_links(text, opts).map(|(out, _)| out)
}

/// One link that was rewritten to an embed host: the URL as copied and its
/// embed-friendly form. Links that only lost their trackers are not reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewrite {
    pub original: String,
    pub embed: String,
}

/// Like [`transform_text`], but also reports every link that was rewritten to
/// an embed host.
pub fn transform_text_links(text: &str, opts: &Options) -> Option<(String, Vec<Rewrite>)> {
    // Fast path: the whole input is one clean URL (trims surrounding ws).
    if let Some((out, embedded)) = rewrite_url(text, opts) {
        let links = embedded
            .then(|| Rewrite {
                original: text.trim().to_string(),
                embed: out.clone(),
            })
            .into_iter()
            .collect();
        return Some((out, links));
    }
    transform_embedded(text, opts)
}

/// The [`Rewrite`] for `original`, if its host was swapped for an embed host.
fn report(original: &str, out: &str, embedded: bool) -> Option<Rewrite> {
    embedded.then(|| Rewrite {
        original: original.to_string(),
        embed: out.to_string(),
    })
}

/// Rewrite one whitespace-delimited token, tolerating wrapping punctuation such
/// as `(url)`, `[text](url)`, `"url"`, `<url>` or a trailing `.` / `,`.
fn transform_token(token: &str, opts: &Options) -> Option<(String, Option<Rewrite>)> {
    if let Some((out, embedded)) = rewrite_url(token, opts) {
        let link = report(token, &out, embedded);
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

    rewrite_url(core, opts).map(|(rewritten, embedded)| {
        let link = report(core, &rewritten, embedded);
        (format!("{prefix}{rewritten}{suffix}"), link)
    })
}

/// Scan `text` for URLs embedded in prose and rewrite each one in place.
/// Whitespace (spaces, tabs, newlines, …) splits tokens and is preserved
/// verbatim; each non-whitespace token is offered to [`transform_clipboard`].
/// Returns `None` when no token changed (so the clipboard is left alone and we
/// avoid a re-write loop).
fn transform_embedded(text: &str, opts: &Options) -> Option<(String, Vec<Rewrite>)> {
    // Cheap dry run: most clipboard changes contain no supported URL, so
    // detect that up front and return without allocating in that common case.
    if !text
        .split_whitespace()
        .any(|token| transform_token(token, opts).is_some())
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
        match transform_token(&rest[..end], opts) {
            Some((rewritten, link)) => {
                out.push_str(&rewritten);
                links.extend(link);
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

    const FIXUP: Options = Options {
        x_rules: FIXUP_RULES,
        strip_trackers: true,
    };

    fn transform_clipboard_with(
        text: &str,
        x_rules: &[(&str, &str)],
        strip_trackers: bool,
    ) -> Option<String> {
        let opts = Options {
            x_rules,
            strip_trackers,
        };
        transform_clipboard(text, &opts)
    }

    #[test]
    fn mpregx_rules_rewrite_x_and_twitter() {
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", MPREG_RULES, true),
            Some("https://mpregx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with(
                "https://mobile.twitter.com/user/status/1",
                MPREG_RULES,
                true
            ),
            Some("https://mobile.mpregx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mpregx.com/user/status/1", MPREG_RULES, true),
            None
        );
    }

    #[test]
    fn custom_domain_rules_rewrite_x_and_twitter() {
        // Same shape `config::with_x_rules` builds for a custom selection.
        let domain = "myfx.com";
        let rules = [("twitter.com", domain), ("x.com", domain)];
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", &rules, true),
            Some("https://myfx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mobile.twitter.com/user/status/1", &rules, true),
            Some("https://mobile.myfx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://myfx.com/user/status/1", &rules, true),
            None
        );
    }

    #[test]
    fn yaoisex_rules_rewrite_x_and_twitter() {
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", YAOI_RULES, true),
            Some("https://yaoisex.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mobile.twitter.com/user/status/1", YAOI_RULES, true),
            Some("https://mobile.yaoisex.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://yaoisex.com/user/status/1", YAOI_RULES, true),
            None
        );
    }

    #[test]
    fn faggotx_rules_rewrite_x_and_twitter() {
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", FAGGOT_RULES, true),
            Some("https://faggotx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with(
                "https://mobile.twitter.com/user/status/1",
                FAGGOT_RULES,
                true
            ),
            Some("https://mobile.faggotx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://faggotx.com/user/status/1", FAGGOT_RULES, true),
            None
        );
    }

    #[test]
    fn reports_each_rewritten_link() {
        let (out, links) = transform_text_links(
            "a (https://x.com/u/status/1) b <https://bsky.app/p/2> c",
            &FIXUP,
        )
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
        let (_, links) = transform_text_links("  https://x.com/u/status/1\n", &FIXUP).unwrap();
        assert_eq!(links[0].original, "https://x.com/u/status/1");
    }

    #[test]
    fn every_target_has_distinct_rules() {
        use crate::XTarget;
        for (i, a) in XTarget::ALL.iter().enumerate() {
            for b in &XTarget::ALL[i + 1..] {
                assert_ne!(a.rules(), b.rules());
            }
        }
    }

    #[test]
    fn common_rules_apply_to_every_target() {
        for rules in [
            FIXUP_RULES,
            BOY_RULES,
            MPREG_RULES,
            YAOI_RULES,
            FAGGOT_RULES,
        ] {
            assert_eq!(
                transform_clipboard_with("https://www.tiktok.com/@a/video/1", rules, true),
                Some("https://www.tnktok.com/@a/video/1".to_string())
            );
        }
    }

    #[test]
    fn email_addresses_are_not_rewritten() {
        assert_eq!(
            transform_clipboard_with("press@twitter.com", FIXUP_RULES, true),
            None
        );
        assert_eq!(
            transform_clipboard_with("mailto:press@x.com", FIXUP_RULES, true),
            None
        );
    }

    #[test]
    fn trackers_are_stripped_from_embed_links() {
        assert_eq!(
            transform_clipboard_with("https://x.com/u/status/1?s=46&t=abc#r", FIXUP_RULES, true),
            Some("https://fixupx.com/u/status/1#r".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://x.com/u/status/1?s=46", FIXUP_RULES, false),
            Some("https://fixupx.com/u/status/1?s=46".to_string())
        );
    }

    #[test]
    fn other_sites_are_only_cleaned() {
        let url = "https://www.youtube.com/watch?v=abc&si=xyz";
        assert_eq!(
            transform_clipboard_with(url, FIXUP_RULES, true),
            Some("https://www.youtube.com/watch?v=abc".to_string())
        );
        assert_eq!(transform_clipboard_with(url, FIXUP_RULES, false), None);
        // Without a scheme it might not be a URL at all.
        assert_eq!(
            transform_clipboard_with("example.com/?utm_source=a", FIXUP_RULES, true),
            None
        );
        // Already clean: nothing to write back.
        assert_eq!(
            transform_clipboard_with("https://www.youtube.com/watch?v=abc", FIXUP_RULES, true),
            None
        );
    }

    #[test]
    fn cleaned_only_links_are_not_reported() {
        let (out, links) = transform_text_links(
            "a https://example.com/p?utm_source=x b https://x.com/u/status/1?t=abc",
            &FIXUP,
        )
        .unwrap();
        assert_eq!(
            out,
            "a https://example.com/p b https://fixupx.com/u/status/1"
        );
        assert_eq!(
            links,
            vec![Rewrite {
                original: "https://x.com/u/status/1?t=abc".into(),
                embed: "https://fixupx.com/u/status/1".into(),
            }]
        );
        let (_, links) = transform_text_links("https://example.com/?fbclid=1", &FIXUP).unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn converted_x_links_are_cleaned() {
        let url = "https://fixupx.com/ConReforged/status/2115797872029368740?a=CNDV1pkwTLYCXh1ElRo";
        assert_eq!(
            transform_clipboard_with(url, FIXUP_RULES, true),
            Some("https://fixupx.com/ConReforged/status/2115797872029368740".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://x.com/u/status/1?a=CNDV1", MPREG_RULES, true),
            Some("https://mpregx.com/u/status/1".to_string())
        );
        let rules = [("twitter.com", "myfx.com"), ("x.com", "myfx.com")];
        assert_eq!(
            transform_clipboard_with("https://myfx.com/u/status/1?a=1", &rules, true),
            Some("https://myfx.com/u/status/1".to_string())
        );
    }
}
