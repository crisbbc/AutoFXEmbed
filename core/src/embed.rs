//! Reading the embed data (OpenGraph tags) of a fetched page, for the Recent
//! list. Fetching is left to each app.

/// Longest Recent-list label, in characters.
pub const MAX_LABEL_CHARS: usize = 60;

/// The embed data of a page: what Discord and friends show in a link preview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Og {
    pub site: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// Pull the embed data out of an HTML page: `og:*` meta tags, falling back to
/// `twitter:*` tags and then `<title>`.
pub fn parse_og(html: &str) -> Og {
    let lower = html.to_ascii_lowercase();
    let mut og = Og::default();
    let (mut tw_title, mut tw_description) = (None, None);

    let mut from = 0;
    while let Some(found) = lower[from..].find("<meta") {
        let start = from + found + "<meta".len();
        let end = tag_end(html, start);
        from = end;
        let (mut key, mut content) = (None, None);
        for (name, value) in attributes(&html[start..end]) {
            match name.as_str() {
                "property" | "name" => key = Some(value.to_ascii_lowercase()),
                "content" => content = Some(decode_entities(&value)),
                _ => {}
            }
        }
        let (Some(key), Some(content)) = (key, content) else {
            continue;
        };
        let slot = match key.as_str() {
            "og:title" => &mut og.title,
            "og:description" => &mut og.description,
            "og:image" => &mut og.image,
            "og:site_name" => &mut og.site,
            "twitter:title" => &mut tw_title,
            "twitter:description" => &mut tw_description,
            _ => continue,
        };
        if slot.is_none() && !content.is_empty() {
            *slot = Some(content);
        }
    }

    og.title = og.title.or(tw_title).or_else(|| page_title(html, &lower));
    og.description = og.description.or(tw_description);
    og
}

fn page_title(html: &str, lower: &str) -> Option<String> {
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let title = decode_entities(html[start..end].trim());
    (!title.is_empty()).then_some(title)
}

/// Index of the `>` closing the tag whose attributes start at `from` (quotes
/// may contain `>`), or the end of the input.
fn tag_end(html: &str, from: usize) -> usize {
    let mut quote = None;
    for (offset, c) in html[from..].char_indices() {
        match (quote, c) {
            (None, '>') => return from + offset,
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            _ => {}
        }
    }
    html.len()
}

/// Attribute `(lowercase name, raw value)` pairs of a tag body.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return out;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let (value, remaining) = match after_eq.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let body = &after_eq[1..];
                match body.find(q) {
                    Some(close) => (&body[..close], &body[close + 1..]),
                    None => (body, ""),
                }
            }
            _ => {
                let end = after_eq.find(char::is_whitespace).unwrap_or(after_eq.len());
                (&after_eq[..end], &after_eq[end..])
            }
        };
        if !name.is_empty() {
            out.push((name, value.to_string()));
        }
        rest = remaining;
    }
}

/// Decode the named and numeric HTML entities found in meta content.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &rest[1..semi];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name.strip_prefix('#').and_then(|num| {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }),
            }?;
            Some((c, semi + 1))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Lowercase host and path (`/` when empty) of an `http(s)` URL.
fn host_and_path(url: &str) -> Option<(String, &str)> {
    let rest = url.split_once("://")?.1;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = &authority[authority.rfind('@').map_or(0, |i| i + 1)..];
    let host = &host_port[..host_port.find(':').unwrap_or(host_port.len())];
    let after = &rest[authority_end..];
    let path = &after[..after.find(['?', '#']).unwrap_or(after.len())];
    Some((
        host.to_ascii_lowercase(),
        if path.is_empty() { "/" } else { path },
    ))
}

/// Whether a fetched page is a usable embed of `embed`: it stayed on the same
/// host, or was forwarded to another embed host with the same path (some
/// targets redirect to e.g. `vxtwitter.com`), and carries a title or
/// description. A bare host bouncing to an unrelated page changes the path and
/// is rejected.
pub fn is_embed(embed: &str, final_uri: &str, og: &Og) -> bool {
    let (Some((host, path)), Some((final_host, final_path))) =
        (host_and_path(embed), host_and_path(final_uri))
    else {
        return false;
    };
    (host == final_host || path == final_path) && (og.title.is_some() || og.description.is_some())
}

/// One-line Recent-list label: `title — description`, or `fallback` (the embed
/// URL) until the metadata is known. At most [`MAX_LABEL_CHARS`] characters.
pub fn label(title: Option<&str>, description: Option<&str>, fallback: &str) -> String {
    let title = title.filter(|text| !text.is_empty());
    let description = description.filter(|text| !text.is_empty());
    let text = match (title, description) {
        (Some(title), Some(description)) => format!("{title} — {description}"),
        (Some(only), None) | (None, Some(only)) => only.to_string(),
        (None, None) => fallback.to_string(),
    };
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_LABEL_CHARS {
        return one_line;
    }
    let mut label: String = one_line.chars().take(MAX_LABEL_CHARS - 1).collect();
    label.push('…');
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    fn og_with_title() -> Og {
        Og {
            title: Some("Jane (@jane)".into()),
            ..Og::default()
        }
    }

    #[test]
    fn embeds_must_stay_on_host_or_path_and_have_data() {
        assert!(is_embed(
            "https://mpregx.com/u/status/1",
            "https://vxtwitter.com/u/status/1",
            &og_with_title()
        ));
        assert!(!is_embed(
            "https://mpregx.com/u/status/1",
            "https://vxtwitter.com/other",
            &og_with_title()
        ));
        let embed = "https://fixupx.com/";
        assert!(is_embed(
            embed,
            "https://fixupx.com/u/status/1",
            &og_with_title()
        ));
        assert!(is_embed(embed, "https://FixupX.com/", &og_with_title()));
        assert!(!is_embed(
            embed,
            "https://github.com/FxEmbed/FxEmbed",
            &og_with_title()
        ));
        assert!(!is_embed(embed, embed, &Og::default()));
    }

    #[test]
    fn parses_a_fxtwitter_style_page() {
        let html = r#"<html><head>
            <meta property="og:site_name" content="FixupX">
            <meta property="og:title" content="Jane (@jane)" />
            <meta property="og:description" content="Fish &amp; chips &#39;n&#x27; peas &gt; all">
            <meta property="og:image" content="https://pbs.example/img.jpg">
            <title>ignored</title></head></html>"#;
        assert_eq!(
            parse_og(html),
            Og {
                site: Some("FixupX".into()),
                title: Some("Jane (@jane)".into()),
                description: Some("Fish & chips 'n' peas > all".into()),
                image: Some("https://pbs.example/img.jpg".into()),
            }
        );
    }

    #[test]
    fn parses_reversed_attributes_and_single_quotes() {
        let html = "<META CONTENT='Hi > there' NAME='twitter:title'><meta content=x name=twitter:description>";
        let og = parse_og(html);
        assert_eq!(og.title.as_deref(), Some("Hi > there"));
        assert_eq!(og.description.as_deref(), Some("x"));
    }

    #[test]
    fn falls_back_to_the_page_title_and_handles_empty_pages() {
        assert_eq!(
            parse_og("<title> A &amp; B </title>").title.as_deref(),
            Some("A & B")
        );
        assert_eq!(parse_og("<html></html>"), Og::default());
        assert_eq!(parse_og(""), Og::default());
    }

    #[test]
    fn entity_decoding_leaves_unknown_entities_alone() {
        assert_eq!(
            decode_entities("a &nope; b & c &#xZZ;"),
            "a &nope; b & c &#xZZ;"
        );
    }

    #[test]
    fn labels_are_one_line_and_bounded() {
        let url = "https://fixupx.com/a";
        assert_eq!(label(None, Some(""), url), url);
        assert_eq!(
            label(Some("Jane (@jane)"), Some("line one\nline two"), url),
            "Jane (@jane) — line one line two"
        );
        let long = label(Some("Jane"), Some(&"x".repeat(200)), url);
        assert_eq!(long.chars().count(), MAX_LABEL_CHARS);
        assert!(long.ends_with('…'));
    }
}
