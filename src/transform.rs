//! Link rewriting with the user's saved settings. The rules themselves live in
//! the `autofxembed-core` crate, which the Android app shares.

use autofxembed_core::Options;

pub use autofxembed_core::Rewrite;

fn with_options<R>(f: impl FnOnce(&Options) -> R) -> R {
    let strip_trackers = crate::config::strip_trackers();
    crate::config::with_x_rules(|x_rules| {
        f(&Options {
            x_rules,
            strip_trackers,
        })
    })
}

/// See [`autofxembed_core::transform_clipboard`].
pub fn transform_clipboard(text: &str) -> Option<String> {
    with_options(|opts| autofxembed_core::transform_clipboard(text, opts))
}

/// See [`autofxembed_core::transform_text`].
pub fn transform_text(text: &str) -> Option<String> {
    with_options(|opts| autofxembed_core::transform_text(text, opts))
}

/// See [`autofxembed_core::transform_text_links`].
pub fn transform_text_links(text: &str) -> Option<(String, Vec<Rewrite>)> {
    with_options(|opts| autofxembed_core::transform_text_links(text, opts))
}
