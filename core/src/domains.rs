//! Validation of user-added embed domains.

/// True if `host` is `domain` or a subdomain of it.
fn is_or_under(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|p| p.ends_with('.'))
}

/// True for the embed hosts that already have a built-in target.
pub fn is_builtin_domain(domain: &str) -> bool {
    crate::XTarget::ALL
        .into_iter()
        .flat_map(|target| target.rules())
        .any(|&(_, replacement)| replacement == domain)
}

/// Normalize a raw host/URL input (`https://MyFx.com/path`) into a bare
/// lowercase DNS hostname (`myfx.com`) without checking rewrite-loop rules.
pub fn normalize_host(input: &str) -> Result<String, &'static str> {
    let mut text = input.trim().to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(rest) = text.strip_prefix(scheme) {
            text = rest.to_string();
            break;
        }
    }
    text.truncate(text.find(['/', '?', '#', ':']).unwrap_or(text.len()));
    let domain = text.trim_end_matches('.');

    if domain.is_empty() {
        return Err("Enter a domain such as example.com");
    }
    if domain.len() > 253 || !domain.contains('.') {
        return Err("That doesn't look like a domain name");
    }
    let valid_label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if !domain.split('.').all(valid_label) {
        return Err("That doesn't look like a domain name");
    }
    Ok(domain.to_string())
}

/// Turn what a user typed (`https://MyFx.com/path`) into a bare lowercase
/// domain (`myfx.com`), or explain why it can't be used as an X/Twitter embed host.
pub fn normalize_domain(input: &str) -> Result<String, &'static str> {
    let domain = normalize_host(input)?;
    // The embed host must not itself be rewritten, or links would loop.
    if is_or_under(&domain, "x.com") || is_or_under(&domain, "twitter.com") {
        return Err("That domain would be rewritten again");
    }
    Ok(domain)
}

/// Validate a custom `source -> target` site rewrite rule (e.g.
/// `reddit.com -> rxddit.com`). Rejects built-in source hosts and loops.
pub fn normalize_site_rule(source: &str, target: &str) -> Result<(String, String), &'static str> {
    let source = normalize_host(source)?;
    let target = normalize_host(target)?;
    for builtin in ["x.com", "twitter.com", "bsky.app", "tiktok.com"] {
        if is_or_under(&source, builtin) {
            return Err("Built-in sites are configured separately");
        }
        if is_or_under(&target, builtin) {
            return Err("Target domain would be rewritten again");
        }
    }
    if is_or_under(&target, &source) {
        return Err("Target domain cannot be the same as source");
    }
    Ok((source, target))
}

/// Validate an optional FxEmbed translation language code (`en`, `es`, `pt-br`).
/// Empty input returns `Ok(None)` (translation disabled).
pub fn normalize_lang(input: &str) -> Result<Option<String>, &'static str> {
    let code = input.trim().to_ascii_lowercase();
    if code.is_empty() {
        return Ok(None);
    }
    let valid = (2..=8).contains(&code.len())
        && code.split('-').all(|part| {
            (2..=4).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_lowercase())
        })
        && code.split('-').count() <= 2;
    if !valid {
        return Err("Use a language code like en, es, ja, or pt-br");
    }
    Ok(Some(code))
}

/// X/Twitter rewrite rules that send both hosts to a custom `domain`.
pub fn custom_rules(domain: &str) -> [(&str, &str); 2] {
    [("twitter.com", domain), ("x.com", domain)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_what_users_type() {
        for (input, expected) in [
            ("MyFx.COM", "myfx.com"),
            ("  https://myfx.com/path?q=1  ", "myfx.com"),
            ("http://sub.myfx.com:8080/", "sub.myfx.com"),
            ("myfx.com.", "myfx.com"),
            ("my-fx2.example.org", "my-fx2.example.org"),
        ] {
            assert_eq!(normalize_domain(input).as_deref(), Ok(expected), "{input}");
        }
    }

    #[test]
    fn rejects_unusable_domains() {
        for input in [
            "",
            "   ",
            "myfx",
            "x.com",
            "https://twitter.com/foo",
            "fx.twitter.com",
            "bad_domain.com",
            "-a.com",
            "a-.com",
            "a..com",
            "user@myfx.com",
            "myfx .com",
            "ünï.com",
        ] {
            assert!(normalize_domain(input).is_err(), "{input:?} should fail");
        }
        // A look-alike that merely ends in the same letters is fine.
        assert_eq!(normalize_domain("myx.com").as_deref(), Ok("myx.com"));
    }

    #[test]
    fn builtin_hosts_are_not_custom() {
        for domain in ["fxtwitter.com", "fixupx.com", "mpregx.com", "faggotx.com"] {
            assert!(is_builtin_domain(domain), "{domain}");
        }
        assert!(!is_builtin_domain("myfx.com"));
    }

    #[test]
    fn validates_site_rules_and_languages() {
        assert_eq!(
            normalize_site_rule("https://Reddit.com/r/rust", "rxddit.com"),
            Ok(("reddit.com".into(), "rxddit.com".into()))
        );
        assert!(normalize_site_rule("reddit.com", "reddit.com").is_err());
        assert!(normalize_site_rule("reddit.com", "old.reddit.com").is_err());
        assert!(normalize_site_rule("x.com", "myfx.com").is_err());
        assert!(normalize_site_rule("example.com", "x.com").is_err());

        assert_eq!(normalize_lang(""), Ok(None));
        assert_eq!(normalize_lang(" EN "), Ok(Some("en".into())));
        assert_eq!(normalize_lang("pt-BR"), Ok(Some("pt-br".into())));
        assert!(normalize_lang("english").is_err());
        assert!(normalize_lang("en/1").is_err());
    }
}
