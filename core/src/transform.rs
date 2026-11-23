/// Maximum clipboard text size scanned (1 MiB) so copying huge logs never stalls.
const MAX_INPUT_BYTES: usize = 1024 * 1024;

/// Settings a rewrite depends on.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    /// X/Twitter `(original host, embed host)` rules, from [`crate::XTarget::rules`]
    /// or built for a custom domain.
    pub x_rules: &'a [(&'a str, &'a str)],
    /// Remove tracking query parameters (see [`crate::trackers`]).
    pub strip_trackers: bool,
    /// Rewrite X / Twitter links.
    pub rewrite_x: bool,
    /// Rewrite Bluesky (`bsky.app` → `fxbsky.app`) links.
    pub rewrite_bsky: bool,
    /// Rewrite TikTok (`tiktok.com` → `tnktok.com`) links.
    pub rewrite_tiktok: bool,
    /// Extra user-defined `(source_domain, target_domain)` rewrite rules.
    pub extra_rules: &'a [(String, String)],
    /// Optional FxEmbed auto-translate language code appended to X status URLs
    /// (e.g. `"en"` turns `/user/status/1` into `/user/status/1/en`).
    pub x_translate_lang: Option<&'a str>,
}

impl<'a> Options<'a> {
    /// Default options with all built-in sites enabled and no extra rules.
    pub const fn new(x_rules: &'a [(&'a str, &'a str)], strip_trackers: bool) -> Self {
        Self {
            x_rules,
            strip_trackers,
            rewrite_x: true,
            rewrite_bsky: true,
            rewrite_tiktok: true,
            extra_rules: &[],
            x_translate_lang: None,
        }
    }
}

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

/// Return the subdomain prefix before `domain` (e.g. `"mobile"` for
/// `mobile.x.com`, `""` for `x.com`), or `None` if `host` doesn't match.
fn host_subdomain<'a>(host: &'a str, domain: &str) -> Option<&'a str> {
    if host.eq_ignore_ascii_case(domain) {
        return Some("");
    }
    let suffix_start = host.len().checked_sub(domain.len())?;
    if suffix_start > 0
        && host.as_bytes().get(suffix_start - 1) == Some(&b'.')
        && host
            .get(suffix_start..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(domain))
    {
        Some(&host[..suffix_start - 1])
    } else {
        None
    }
}

/// True if `path` starts with `prefix` followed by end-of-path or `/`.
fn path_starts_with_segment(path: &str, prefix: &str) -> bool {
    let Some(head) = path.get(..prefix.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(prefix) {
        return false;
    }
    matches!(path.as_bytes().get(prefix.len()), None | Some(b'/'))
}

fn path_of_tail(tail: &str) -> &str {
    &tail[..tail.find(['?', '#']).unwrap_or(tail.len())]
}

fn is_bare_root(path: &str) -> bool {
    path.is_empty() || path == "/"
}

fn is_x_non_embed_path(path: &str) -> bool {
    if is_bare_root(path) {
        return true;
    }
    const SKIPPED: &[&str] = &[
        "/home",
        "/explore",
        "/search",
        "/notifications",
        "/messages",
        "/settings",
        "/login",
        "/logout",
        "/signup",
        "/compose",
        "/tos",
        "/privacy",
        "/download",
        "/jobs",
        "/account",
        "/i/flow",
        "/i/grok",
        "/i/trends",
        "/i/bookmarks",
        "/i/lists",
        "/i/connect_people",
        "/i/keyboard_shortcuts",
        "/i/premium",
        "/i/verified-orgs-signup",
    ];
    SKIPPED.iter().any(|p| path_starts_with_segment(path, p))
}

fn is_bsky_non_embed_path(path: &str) -> bool {
    if is_bare_root(path) {
        return true;
    }
    const SKIPPED: &[&str] = &[
        "/settings",
        "/notifications",
        "/search",
        "/feeds",
        "/lists",
        "/moderation",
        "/messages",
    ];
    SKIPPED.iter().any(|p| path_starts_with_segment(path, p))
}

fn is_tiktok_non_embed_path(path: &str, is_short_sub: bool) -> bool {
    if is_bare_root(path) {
        return true;
    }
    if is_short_sub {
        return false;
    }
    const SKIPPED: &[&str] = &[
        "/login",
        "/signup",
        "/upload",
        "/settings",
        "/messages",
        "/foryou",
        "/following",
        "/explore",
        "/friends",
        "/live",
    ];
    SKIPPED.iter().any(|p| path_starts_with_segment(path, p))
}

/// True when `path` is a direct X/Twitter status URL (`/user/status/123` or
/// `/i/status/123` or `/i/web/status/123`) with no extra trailing sub-segments
/// (such as `/photo/1` or an already-present language code).
fn is_bare_x_status_path(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    let Some((_, after_status)) = trimmed.rsplit_once("/status/") else {
        return false;
    };
    !after_status.is_empty() && after_status.bytes().all(|b| b.is_ascii_digit())
}

/// Append `/{lang}` to the path part of `tail` before any `?query` or `#frag`.
fn append_x_translate_lang(tail: &str, lang: &str) -> String {
    let path_end = tail.find(['?', '#']).unwrap_or(tail.len());
    let path = tail[..path_end].trim_end_matches('/');
    let rest = &tail[path_end..];
    format!("{path}/{lang}{rest}")
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

/// Outcome of matching a URL's host + path against rewrite rules.
struct HostRewrite<'a> {
    /// Replacement host (either bare domain or with preserved subdomain).
    replacement: &'a str,
    /// Whether to preserve the original subdomain prefix (e.g. `vm.` for TikTok).
    keep_subdomain: bool,
    /// Original matched root domain length (used when `keep_subdomain` is true).
    matched_domain_len: usize,
    /// Whether this was an X/Twitter rewrite (eligible for `x_translate_lang`).
    is_x: bool,
}

fn match_rewrite<'a>(host: &str, path: &str, opts: &'a Options<'a>) -> Option<HostRewrite<'a>> {
    if opts.rewrite_x {
        for &(domain, replacement) in opts.x_rules {
            if let Some(sub) = host_subdomain(host, domain) {
                if sub.is_empty()
                    || sub.eq_ignore_ascii_case("www")
                    || sub.eq_ignore_ascii_case("mobile")
                {
                    if !is_x_non_embed_path(path) {
                        return Some(HostRewrite {
                            replacement,
                            keep_subdomain: false,
                            matched_domain_len: domain.len(),
                            is_x: true,
                        });
                    }
                    return None;
                }
            }
        }
    }

    if opts.rewrite_bsky {
        if let Some(sub) = host_subdomain(host, "bsky.app") {
            if sub.is_empty() || sub.eq_ignore_ascii_case("www") {
                if !is_bsky_non_embed_path(path) {
                    return Some(HostRewrite {
                        replacement: "fxbsky.app",
                        keep_subdomain: false,
                        matched_domain_len: "bsky.app".len(),
                        is_x: false,
                    });
                }
                return None;
            }
        }
    }

    if opts.rewrite_tiktok {
        if let Some(sub) = host_subdomain(host, "tiktok.com") {
            let short = sub.eq_ignore_ascii_case("vm") || sub.eq_ignore_ascii_case("vt");
            let allowed = short
                || sub.is_empty()
                || sub.eq_ignore_ascii_case("www")
                || sub.eq_ignore_ascii_case("m");
            if allowed {
                if !is_tiktok_non_embed_path(path, short) {
                    return Some(HostRewrite {
                        replacement: "tnktok.com",
                        keep_subdomain: true,
                        matched_domain_len: "tiktok.com".len(),
                        is_x: false,
                    });
                }
                return None;
            }
        }
    }

    for (source, target) in opts.extra_rules {
        if let Some(sub) = host_subdomain(host, source) {
            let allowed = sub.is_empty()
                || sub.eq_ignore_ascii_case("www")
                || sub.eq_ignore_ascii_case("m")
                || sub.eq_ignore_ascii_case("mobile");
            if allowed && !is_bare_root(path) {
                return Some(HostRewrite {
                    replacement: target.as_str(),
                    keep_subdomain: false,
                    matched_domain_len: source.len(),
                    is_x: false,
                });
            }
        }
    }

    None
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
    if text.len() > MAX_INPUT_BYTES {
        return None;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.contains('.') {
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
    let path = path_of_tail(tail);
    let cleaned = opts
        .strip_trackers
        .then(|| crate::trackers::strip(host, is_x_host(host, opts.x_rules), tail))
        .flatten();

    if let Some(rw) = match_rewrite(host, path, opts) {
        let mut final_tail = cleaned.unwrap_or_else(|| tail.to_string());
        if rw.is_x {
            if let Some(lang) = opts.x_translate_lang.filter(|l| !l.is_empty()) {
                if is_bare_x_status_path(path_of_tail(&final_tail)) {
                    final_tail = append_x_translate_lang(&final_tail, lang);
                }
            }
        }
        let prefix_end = if rw.keep_subdomain {
            host_end - rw.matched_domain_len
        } else {
            host_start
        };
        let mut result = String::with_capacity(
            scheme.len()
                + prefix_end
                + rw.replacement.len()
                + (authority.len() - host_end)
                + final_tail.len(),
        );
        result.push_str(scheme);
        result.push_str(&authority[..prefix_end]);
        result.push_str(rw.replacement);
        result.push_str(&authority[host_end..]);
        result.push_str(&final_tail);
        return Some((result, true));
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
    if text.len() > MAX_INPUT_BYTES {
        return None;
    }
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

/// Find the byte index of `http://` or `https://` (ASCII case-insensitive)
/// without allocating a lowercased copy of `token`.
fn find_scheme_start(token: &str) -> Option<usize> {
    let bytes = token.as_bytes();
    if bytes.len() < 7 || !token.contains(':') {
        return None;
    }
    for i in 0..=bytes.len() - 7 {
        if (bytes[i] | 0x20) != b'h' {
            continue;
        }
        if bytes[i..].len() >= 8 && bytes[i..i + 8].eq_ignore_ascii_case(b"https://") {
            return Some(i);
        }
        if bytes[i..i + 7].eq_ignore_ascii_case(b"http://") {
            return Some(i);
        }
    }
    None
}

/// Rewrite one whitespace-delimited token, tolerating wrapping punctuation such
/// as `(url)`, `[text](url)`, `"url"`, `<url>` or a trailing `.` / `,`.
fn transform_token(token: &str, opts: &Options) -> Option<(String, Option<Rewrite>)> {
    // Every supported domain has at least one dot; skip ordinary words cheaply.
    if !token.contains('.') {
        return None;
    }
    if let Some((out, embedded)) = rewrite_url(token, opts) {
        let link = report(token, &out, embedded);
        return Some((out, link));
    }

    // Prefix: everything before an explicit scheme, else leading opener chars.
    let prefix_len = match find_scheme_start(token) {
        Some(pos) => pos,
        None => token
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

/// Scan `text` in a single pass for URLs embedded in prose and rewrite each one
/// in place. Allocates nothing when no token changes.
fn transform_embedded(text: &str, opts: &Options) -> Option<(String, Vec<Rewrite>)> {
    let mut out: Option<String> = None;
    let mut links = Vec::new();
    let mut pos = 0;

    while pos < text.len() {
        let rest = &text[pos..];
        let ws_len = rest.len() - rest.trim_start_matches(char::is_whitespace).len();
        let token_start = pos + ws_len;
        if token_start >= text.len() {
            if let Some(buf) = out.as_mut() {
                buf.push_str(&text[pos..]);
            }
            break;
        }
        let after_ws = &text[token_start..];
        let token_len = after_ws.find(char::is_whitespace).unwrap_or(after_ws.len());
        let token_end = token_start + token_len;
        let token = &text[token_start..token_end];

        match transform_token(token, opts) {
            Some((rewritten, link)) => {
                let buf = match out.as_mut() {
                    Some(buf) => {
                        buf.push_str(&text[pos..token_start]);
                        buf
                    }
                    None => {
                        let mut s = String::with_capacity(text.len() + 16);
                        s.push_str(&text[..token_start]);
                        out.insert(s)
                    }
                };
                buf.push_str(&rewritten);
                links.extend(link);
            }
            None => {
                if let Some(buf) = out.as_mut() {
                    buf.push_str(&text[pos..token_end]);
                }
            }
        }
        pos = token_end;
    }

    out.map(|s| (s, links))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXUP: Options = Options::new(FIXUP_RULES, true);

    fn transform_clipboard_with(
        text: &str,
        x_rules: &[(&str, &str)],
        strip_trackers: bool,
    ) -> Option<String> {
        let opts = Options::new(x_rules, strip_trackers);
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
            Some("https://mpregx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mpregx.com/user/status/1", MPREG_RULES, true),
            None
        );
    }

    #[test]
    fn custom_domain_rules_rewrite_x_and_twitter() {
        let domain = "myfx.com";
        let rules = [("twitter.com", domain), ("x.com", domain)];
        assert_eq!(
            transform_clipboard_with("https://x.com/user/status/1", &rules, true),
            Some("https://myfx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://mobile.twitter.com/user/status/1", &rules, true),
            Some("https://myfx.com/user/status/1".to_string())
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
            Some("https://yaoisex.com/user/status/1".to_string())
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
            Some("https://faggotx.com/user/status/1".to_string())
        );
        assert_eq!(
            transform_clipboard_with("https://faggotx.com/user/status/1", FAGGOT_RULES, true),
            None
        );
    }

    #[test]
    fn skips_non_content_subdomains_and_pages() {
        for url in [
            "https://developer.x.com/en/docs",
            "https://help.twitter.com/en/rules",
            "https://api.x.com/2/tweets",
            "https://x.com",
            "https://x.com/",
            "https://x.com/home",
            "https://x.com/settings/account",
            "https://x.com/search?q=rust",
            "https://x.com/i/grok",
            "https://docs.bsky.app/docs/get-started",
            "https://bsky.app/",
            "https://bsky.app/settings",
            "https://ads.tiktok.com/business",
            "https://www.tiktok.com/foryou",
        ] {
            assert_eq!(
                transform_clipboard(url, &FIXUP),
                None,
                "{url} should not be rewritten"
            );
        }
        // Trackers on skipped X pages are still cleaned when tracker stripping is on.
        assert_eq!(
            transform_clipboard("https://x.com/search?q=rust&s=09", &FIXUP),
            Some("https://x.com/search?q=rust".to_string())
        );
        // Short TikTok subdomains still keep their subdomain.
        assert_eq!(
            transform_clipboard("https://vm.tiktok.com/ZMj1234/", &FIXUP),
            Some("https://vm.tnktok.com/ZMj1234/".to_string())
        );
    }

    #[test]
    fn respects_per_site_toggles_and_extra_rules() {
        let extra = vec![
            ("reddit.com".to_string(), "rxddit.com".to_string()),
            ("instagram.com".to_string(), "kkinstagram.com".to_string()),
        ];
        let mut opts = Options::new(FIXUP_RULES, true);
        opts.rewrite_x = false;
        opts.rewrite_bsky = false;
        opts.rewrite_tiktok = false;
        opts.extra_rules = &extra;

        assert_eq!(transform_clipboard("https://x.com/u/status/1", &opts), None);
        assert_eq!(
            transform_clipboard("https://bsky.app/profile/a", &opts),
            None
        );
        assert_eq!(
            transform_clipboard("https://www.tiktok.com/@a/video/1", &opts),
            None
        );
        assert_eq!(
            transform_clipboard("https://www.reddit.com/r/rust/comments/1/hi/", &opts),
            Some("https://rxddit.com/r/rust/comments/1/hi/".to_string())
        );
        assert_eq!(
            transform_clipboard("https://www.instagram.com/reel/ABC/?igsh=1", &opts),
            Some("https://kkinstagram.com/reel/ABC/".to_string())
        );
        // Bare root of extra rule is not rewritten.
        assert_eq!(transform_clipboard("https://reddit.com/", &opts), None);
    }

    #[test]
    fn appends_x_translate_lang_only_to_status_posts() {
        let mut opts = Options::new(FIXUP_RULES, true);
        opts.x_translate_lang = Some("en");

        assert_eq!(
            transform_clipboard("https://x.com/u/status/12345?s=46", &opts),
            Some("https://fixupx.com/u/status/12345/en".to_string())
        );
        assert_eq!(
            transform_clipboard("https://x.com/u/status/12345/#frag", &opts),
            Some("https://fixupx.com/u/status/12345/en#frag".to_string())
        );
        // Do not corrupt sub-paths like /photo/1 or profile pages.
        assert_eq!(
            transform_clipboard("https://x.com/u/status/12345/photo/1", &opts),
            Some("https://fixupx.com/u/status/12345/photo/1".to_string())
        );
        assert_eq!(
            transform_clipboard("https://x.com/username", &opts),
            Some("https://fixupx.com/username".to_string())
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

    #[test]
    fn huge_inputs_are_skipped_without_stalling() {
        let mut big = "hello ".repeat(200_000);
        big.push_str("https://x.com/u/status/1");
        assert_eq!(transform_text(&big, &FIXUP), None);
    }
}
