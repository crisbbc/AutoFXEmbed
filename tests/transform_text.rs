use autofxembed::transform::transform_text;

#[test]
fn transform_text_embeds_twitter_in_prose() {
    assert_eq!(
        transform_text("check out https://twitter.com/foo"),
        Some("check out https://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transform_text_embeds_x_in_prose() {
    assert_eq!(
        transform_text("see https://x.com/a/status/1 here"),
        Some("see https://fixupx.com/a/status/1 here".to_string())
    );
}

#[test]
fn transform_text_embeds_bsky_in_prose() {
    assert_eq!(
        transform_text("look https://bsky.app/profile/u.bsky.social yeah"),
        Some("look https://fxbsky.app/profile/u.bsky.social yeah".to_string())
    );
}

#[test]
fn transform_text_embeds_schemeless_url() {
    assert_eq!(
        transform_text("see twitter.com/foo here"),
        Some("see fxtwitter.com/foo here".to_string())
    );
}

#[test]
fn transform_text_embeds_multiple_urls() {
    assert_eq!(
        transform_text("a https://twitter.com/1 b https://x.com/2 c"),
        Some("a https://fxtwitter.com/1 b https://fixupx.com/2 c".to_string())
    );
}

#[test]
fn transform_text_single_link_delegates_and_trims() {
    // Fast path: a lone URL is handled by transform_clipboard, which trims.
    assert_eq!(
        transform_text("  https://twitter.com/foo\n"),
        Some("https://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transform_text_preserves_surrounding_whitespace_in_prose() {
    // Embedded path keeps surrounding whitespace verbatim (does not trim).
    assert_eq!(
        transform_text("  hi https://twitter.com/foo  "),
        Some("  hi https://fxtwitter.com/foo  ".to_string())
    );
}

#[test]
fn transform_text_preserves_newlines_between_tokens() {
    assert_eq!(
        transform_text("line1\nhttps://twitter.com/foo\nline3"),
        Some("line1\nhttps://fxtwitter.com/foo\nline3".to_string())
    );
}

#[test]
fn transform_text_skips_already_transformed_embedded() {
    assert_eq!(transform_text("see https://fxtwitter.com/foo"), None);
}

#[test]
fn transform_text_skips_plain_text() {
    assert_eq!(transform_text("Hello world"), None);
}

#[test]
fn transform_text_skips_non_supported_url_in_prose() {
    assert_eq!(transform_text("see https://example.com/foo"), None);
}

#[test]
fn transform_text_skips_empty_and_whitespace_only() {
    assert_eq!(transform_text(""), None);
    assert_eq!(transform_text("   "), None);
    assert_eq!(transform_text("\n\t  \n"), None);
}

#[test]
fn transform_text_transforms_each_newline_separated_url() {
    // Two tweet links, one per line, no spaces: every URL must be rewritten
    // (the fast path must NOT swallow this into a single-host rewrite).
    assert_eq!(
        transform_text("https://twitter.com/a\nhttps://x.com/b"),
        Some("https://fxtwitter.com/a\nhttps://fixupx.com/b".to_string())
    );
}
