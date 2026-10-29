//! Tracking query parameters stripped from copied links.
//!
//! Only parameters that exist purely to attribute shares or clicks are listed,
//! so removing them never changes what the link points to. X post links are the
//! exception: they never need a query, so theirs is dropped whole (X keeps
//! adding new share ids such as `?a=`).

use crate::transform::host_matches;

/// Removed from every URL. A trailing `*` matches any parameter with that prefix.
const GLOBAL: &[&str] = &[
    "utm_*", "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid", "msclkid", "twclid",
    "ttclid", "yclid", "mc_cid", "mc_eid", "_hsenc", "_hsmi", "mkt_tok", "igshid", "igsh",
];

/// Removed from X / Twitter (and embed-host) links other than posts.
const X: &[&str] = &["s", "t", "a", "ref_src", "ref_url"];

/// Removed only on the given site (and its subdomains).
const PER_SITE: &[(&str, &[&str])] = &[
    (
        "tiktok.com",
        &[
            "is_from_webapp",
            "sender_device",
            "sender_web_id",
            "is_copy_url",
            "share_app_id",
            "share_item_id",
            "share_link_id",
            "social_share_type",
            "u_code",
            "tt_from",
            "_r",
            "_t",
            "_d",
        ],
    ),
    ("youtube.com", &["si", "pp", "feature"]),
    ("youtu.be", &["si", "feature"]),
    ("spotify.com", &["si", "nd"]),
    ("instagram.com", &["img_index"]),
    ("reddit.com", &["share_id", "rdt", "ref_source"]),
];

fn name_matches(name: &str, pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix)),
        None => name.eq_ignore_ascii_case(pattern),
    }
}

fn is_tracker(host: &str, x_host: bool, name: &str) -> bool {
    GLOBAL.iter().any(|p| name_matches(name, p))
        || (x_host && X.iter().any(|p| name_matches(name, p)))
        || PER_SITE
            .iter()
            .filter(|(domain, _)| host_matches(host, domain))
            .any(|(_, names)| names.iter().any(|p| name_matches(name, p)))
}

/// Remove tracking parameters from `tail`, the part of a URL after its
/// authority (`/path?query#fragment`). `x_host` marks X / Twitter and the
/// hosts its links are rewritten to. Returns `None` when nothing was removed.
pub(crate) fn strip(host: &str, x_host: bool, tail: &str) -> Option<String> {
    let query_start = tail.find('?')?;
    let x_post = x_host && tail[..query_start].contains("/status/");
    let query_end = tail[query_start..]
        .find('#')
        .map_or(tail.len(), |i| query_start + i);
    let query = &tail[query_start + 1..query_end];

    let params: Vec<&str> = query.split('&').collect();
    let kept: Vec<&str> = params
        .iter()
        .copied()
        .filter(|param| {
            let name = param.split_once('=').map_or(*param, |(name, _)| name);
            !x_post && !is_tracker(host, x_host, name)
        })
        .collect();
    if kept.len() == params.len() {
        return None;
    }

    let mut out = String::with_capacity(tail.len());
    out.push_str(&tail[..query_start]);
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    out.push_str(&tail[query_end..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_site_and_global_trackers() {
        assert_eq!(
            strip("x.com", true, "/u/status/1?s=46&t=abc").as_deref(),
            Some("/u/status/1")
        );
        assert_eq!(
            strip(
                "www.youtube.com",
                false,
                "/watch?v=abc&si=xyz&UTM_source=a#t=10"
            )
            .as_deref(),
            Some("/watch?v=abc#t=10")
        );
    }

    #[test]
    fn x_posts_lose_their_whole_query() {
        assert_eq!(
            strip("fixupx.com", true, "/u/status/1?a=CNDV1pkw&foo=1#r").as_deref(),
            Some("/u/status/1#r")
        );
        // Elsewhere on X only known trackers go: search needs `q`.
        assert_eq!(
            strip("x.com", true, "/search?q=rust&a=1").as_deref(),
            Some("/search?q=rust")
        );
        assert_eq!(strip("example.com", false, "/u/status/1?a=1"), None);
    }

    #[test]
    fn site_params_stay_on_other_sites() {
        // `t` is a timestamp on YouTube and `s` a search term on many sites.
        assert_eq!(strip("youtube.com", false, "/watch?v=a&t=42"), None);
        assert_eq!(strip("example.com", false, "/find?s=rust&si=1"), None);
    }

    #[test]
    fn untouched_without_trackers() {
        assert_eq!(strip("x.com", true, "/u/status/1"), None);
        assert_eq!(strip("x.com", true, "/u/status/1#s=1"), None);
        assert_eq!(strip("x.com", true, "/search?q=a&src=typed"), None);
        // `utm` alone is not a `utm_` parameter.
        assert_eq!(strip("example.com", false, "/?utm=1"), None);
    }
}
