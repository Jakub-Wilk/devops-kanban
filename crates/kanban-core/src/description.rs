//! `System.Description` HTML: sanitised before the dialog's editor or a card renders it (it comes
//! from other people and runs inside the app's webview) and before it is written back, plus the
//! plain-text first line a collapsed card shows.

use std::collections::HashSet;

/// Drops scripts, event handlers and anything else that could run code; keeps formatting,
/// lists, tables, links, images and inline styles (ADO's editor writes those).
pub fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(HashSet::from(["style"]))
        .clean(html)
        .to_string()
}

/// For showing on a card: [`sanitize`], and links open outside the app (in the browser) instead
/// of navigating the app's own window.
pub fn sanitize_for_display(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(HashSet::from(["style"]))
        .set_tag_attribute_value("a", "target", "_blank")
        .clean(html)
        .to_string()
}

/// The first non-empty line of the description as plain text, or `None` when it has no text.
/// Block ends (`<br>`, `</p>`, `</div>`, `</li>`, headings) end a line.
pub fn first_line(html: &str) -> Option<String> {
    let mut text = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect::<String>()
                    .to_ascii_lowercase();
                let ends_line = matches!(
                    name.as_str(),
                    "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                );
                if ends_line {
                    if let Some(line) = clean_line(&text) {
                        return Some(line);
                    }
                    text.clear();
                }
            }
            (true, c) => tag.push(c),
            (false, c) => text.push(c),
        }
    }
    clean_line(&text)
}

fn clean_line(raw: &str) -> Option<String> {
    let decoded = raw
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let line = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_code_keeps_formatting() {
        let html = r#"<div style="color: red"><b>Hi</b> <a href="https://x.y" onclick="alert(1)">x</a><script>alert(2)</script><img src="a.png" onerror="alert(3)"></div>"#;
        let clean = sanitize(html);
        assert!(clean.contains("<b>Hi</b>"));
        assert!(clean.contains(r#"style="color: red""#));
        assert!(clean.contains(r#"href="https://x.y""#));
        assert!(!clean.contains("script"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("onerror"));
        assert!(!clean.contains("javascript:"));
        assert!(!sanitize(r#"<a href="javascript:alert(1)">x</a>"#).contains("javascript:"));
    }

    #[test]
    fn display_links_open_outside() {
        let clean = sanitize_for_display(r#"<a href="https://x.y">x</a>"#);
        assert!(clean.contains(r#"target="_blank""#));
    }

    #[test]
    fn first_line_of_html() {
        assert_eq!(
            first_line("<div><b>Render</b> variants&nbsp;in &amp; out<br>second</div>"),
            Some("Render variants in & out".into())
        );
        assert_eq!(
            first_line("<div><br></div><p>  </p><h4>Acceptance</h4><ul><li>a</li></ul>"),
            Some("Acceptance".into())
        );
        assert_eq!(first_line("plain\ntext"), Some("plain text".into()));
        assert_eq!(first_line("<div>&nbsp;</div>"), None);
        assert_eq!(first_line(""), None);
    }
}
