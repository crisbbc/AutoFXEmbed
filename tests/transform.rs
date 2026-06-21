use autofxembed::transform::transform_clipboard;

#[test]
fn transforms_twitter_https() {
    assert_eq!(
        transform_clipboard("https://twitter.com/user/status/123"),
        Some("https://fxtwitter.com/user/status/123".to_string())
    );
}

#[test]
fn transforms_x_https() {
    assert_eq!(
        transform_clipboard("https://x.com/user/status/123"),
        Some("https://fixupx.com/user/status/123".to_string())
    );
}

#[test]
fn transforms_bsky_https() {
    assert_eq!(
        transform_clipboard("https://bsky.app/profile/user.bsky.social"),
        Some("https://fxbsky.app/profile/user.bsky.social".to_string())
    );
}

#[test]
fn transforms_http_scheme() {
    assert_eq!(
        transform_clipboard("http://twitter.com/foo"),
        Some("http://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_no_scheme() {
    assert_eq!(
        transform_clipboard("twitter.com/foo"),
        Some("fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_subdomain() {
    assert_eq!(
        transform_clipboard("https://mobile.twitter.com/foo"),
        Some("https://mobile.fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_x_subdomain() {
    assert_eq!(
        transform_clipboard("https://api.x.com/2/foo"),
        Some("https://api.fixupx.com/2/foo".to_string())
    );
}

#[test]
fn preserves_query_and_fragment() {
    assert_eq!(
        transform_clipboard("https://twitter.com/foo?s=123&t=abc#ref"),
        Some("https://fxtwitter.com/foo?s=123&t=abc#ref".to_string())
    );
}

#[test]
fn transforms_bare_host_no_path() {
    assert_eq!(
        transform_clipboard("https://twitter.com"),
        Some("https://fxtwitter.com".to_string())
    );
}

#[test]
fn trims_surrounding_whitespace() {
    assert_eq!(
        transform_clipboard("  https://twitter.com/foo\n"),
        Some("https://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn skips_already_transformed_twitter() {
    assert_eq!(transform_clipboard("https://fxtwitter.com/foo"), None);
}

#[test]
fn skips_already_transformed_x() {
    assert_eq!(transform_clipboard("https://fixupx.com/foo"), None);
}

#[test]
fn skips_already_transformed_bsky() {
    assert_eq!(transform_clipboard("https://fxbsky.app/foo"), None);
}

#[test]
fn skips_non_matching_url() {
    assert_eq!(transform_clipboard("https://example.com/foo"), None);
}

#[test]
fn skips_plain_text() {
    assert_eq!(transform_clipboard("Hello world"), None);
}

#[test]
fn skips_empty() {
    assert_eq!(transform_clipboard(""), None);
    assert_eq!(transform_clipboard("   "), None);
}

#[test]
fn skips_url_with_internal_space() {
    // Not a clean URL -> leave alone.
    assert_eq!(transform_clipboard("https://twitter.com/foo bar"), None);
}

#[test]
fn skips_prose_containing_url() {
    assert_eq!(transform_clipboard("check out https://twitter.com/foo"), None);
}
