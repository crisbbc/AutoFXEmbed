//! JNI bridge for `com.autofxembed.Native`: the Android app calls straight into
//! `autofxembed-core`, so both apps behave identically.

use autofxembed_core::embed::{self, Og};
use autofxembed_core::{Options, XTarget};
use jni::errors::{Result, ThrowRuntimeExAndDefault};
use jni::objects::{JClass, JObjectArray, JString};
use jni::sys::jboolean;
use jni::{Env, EnvUnowned};

/// Prefix of a custom domain selection id, as on the desktop.
const CUSTOM_PREFIX: &str = "custom:";

fn opt_string(env: &Env, value: &JString) -> Result<Option<String>> {
    if value.is_null() {
        Ok(None)
    } else {
        value.try_to_string(env).map(Some)
    }
}

/// A Java `String[]` of `items`, with `None` as `null`.
fn string_array<'local>(
    env: &mut Env<'local>,
    items: &[Option<&str>],
) -> Result<JObjectArray<'local, JString<'local>>> {
    let array = JObjectArray::<JString>::new(env, items.len(), &JString::null())?;
    for (i, item) in items.iter().enumerate() {
        if let Some(item) = item {
            let value = JString::from_str(env, item)?;
            array.set_element(env, i, &value)?;
        }
    }
    Ok(array)
}

/// `static native String[] transform(String text, String selection, boolean stripTrackers)`
///
/// `selection` is a built-in target id or `custom:<domain>`; unknown ids fall
/// back to FixUpX. Returns `null` when nothing changed, else
/// `[newText, original0, embed0, original1, embed1, ...]` listing every link
/// that was rewritten to an embed host.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_transform<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
    selection: JString<'caller>,
    strip_trackers: jboolean,
) -> JObjectArray<'caller, JString<'caller>> {
    unowned_env
        .with_env(|env| -> Result<_> {
            let text = text.try_to_string(env)?;
            let selection = selection.try_to_string(env)?;
            let custom = selection
                .strip_prefix(CUSTOM_PREFIX)
                .and_then(|domain| autofxembed_core::normalize_domain(domain).ok());
            let custom_rules = custom.as_deref().map(autofxembed_core::custom_rules);
            let x_rules = match &custom_rules {
                Some(rules) => rules.as_slice(),
                None => XTarget::from_id(&selection)
                    .unwrap_or(XTarget::FixUp)
                    .rules(),
            };
            let opts = Options {
                x_rules,
                strip_trackers,
            };
            let Some((out, links)) = autofxembed_core::transform_text_links(&text, &opts) else {
                return Ok(JObjectArray::<JString>::null());
            };
            let mut items = vec![Some(out.as_str())];
            for link in &links {
                items.push(Some(&link.original));
                items.push(Some(&link.embed));
            }
            string_array(env, &items)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String[] targets()`: the built-in X / Twitter targets as
/// `[id0, label0, id1, label1, ...]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_targets<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
) -> JObjectArray<'caller, JString<'caller>> {
    unowned_env
        .with_env(|env| -> Result<_> {
            let items: Vec<Option<&str>> = XTarget::ALL
                .iter()
                .flat_map(|target| [Some(target.id()), Some(target.label())])
                .collect();
            string_array(env, &items)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String[] normalizeDomain(String input)`: `[domain, null]` for
/// a usable custom embed domain, else `[null, reason]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_normalizeDomain<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    input: JString<'caller>,
) -> JObjectArray<'caller, JString<'caller>> {
    unowned_env
        .with_env(|env| -> Result<_> {
            let input = input.try_to_string(env)?;
            match autofxembed_core::normalize_domain(&input) {
                Ok(domain) if autofxembed_core::is_builtin_domain(&domain) => {
                    let reason = format!("{domain} is already built in");
                    string_array(env, &[None, Some(&reason)])
                }
                Ok(domain) => string_array(env, &[Some(&domain), None]),
                Err(reason) => string_array(env, &[None, Some(reason)]),
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String[] parseEmbed(String embed, String finalUrl, String html)`
///
/// The embed data of a fetched page as `[site, title, description, image]`
/// (entries may be `null`), or `null` when the page is not a usable embed of
/// `embed` (see [`embed::is_embed`]).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_parseEmbed<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    embed_url: JString<'caller>,
    final_url: JString<'caller>,
    html: JString<'caller>,
) -> JObjectArray<'caller, JString<'caller>> {
    unowned_env
        .with_env(|env| -> Result<_> {
            let og: Og = embed::parse_og(&html.try_to_string(env)?);
            let embed_url = embed_url.try_to_string(env)?;
            let final_url = final_url.try_to_string(env)?;
            if !embed::is_embed(&embed_url, &final_url, &og) {
                return Ok(JObjectArray::<JString>::null());
            }
            string_array(
                env,
                &[
                    og.site.as_deref(),
                    og.title.as_deref(),
                    og.description.as_deref(),
                    og.image.as_deref(),
                ],
            )
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native String label(String title, String description, String fallback)`:
/// the one-line Recent-list label (`title` / `description` may be `null`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_label<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    title: JString<'caller>,
    description: JString<'caller>,
    fallback: JString<'caller>,
) -> JString<'caller> {
    unowned_env
        .with_env(|env| -> Result<_> {
            let title = opt_string(env, &title)?;
            let description = opt_string(env, &description)?;
            let fallback = fallback.try_to_string(env)?;
            let label = embed::label(title.as_deref(), description.as_deref(), &fallback);
            JString::from_str(env, label)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `static native boolean isNewer(String latest, String current)`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_autofxembed_Native_isNewer<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    latest: JString<'caller>,
    current: JString<'caller>,
) -> jboolean {
    unowned_env
        .with_env(|env| -> Result<_> {
            let latest = latest.try_to_string(env)?;
            let current = current.try_to_string(env)?;
            Ok(autofxembed_core::is_newer(&latest, &current))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
