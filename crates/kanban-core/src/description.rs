//! `System.Description` HTML: sanitised before the dialog's editor renders it (it comes from
//! other people and runs inside the app's webview) and before it is written back.

use std::collections::HashSet;

/// Drops scripts, event handlers and anything else that could run code; keeps formatting,
/// lists, tables, links, images and inline styles (ADO's editor writes those).
pub fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(HashSet::from(["style"]))
        .clean(html)
        .to_string()
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
}
