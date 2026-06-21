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
