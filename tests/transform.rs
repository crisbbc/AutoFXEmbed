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
