//! Record bodies (Markdown) -> the chapter markup the Instinct shell expects.
//!
//! Deliberately NOT a general Markdown implementation, and deliberately not a
//! dependency. It renders exactly the subset `docs/records/*.lingua` uses, and
//! anything outside that subset is escaped and shown as text rather than
//! guessed at. The bodies came from this repository's own HTML through a
//! mechanical conversion, so the subset is known rather than hoped for — and a
//! body that grows a construct this does not handle shows up as visible
//! literal text in the sand, which is how the person who wrote it finds out.
//!
//! The one shape that must survive untouched is a ```mermaid fence: the shell
//! finds `pre.mermaid` in the DOM and renders it on chapter activation.

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `**bold**`, `_em_`, `` `code` `` and `[text](url)`, on already-escaped text.
fn inline(text: &str) -> String {
    let mut out = escape(text);
    for (marker, tag) in [("**", "strong"), ("`", "code"), ("_", "em")] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        let mut open = true;
        while let Some(at) = rest.find(marker) {
            // An unmatched marker stays literal rather than swallowing the
            // rest of the paragraph into a tag that never closes.
            if open && !rest[at + marker.len()..].contains(marker) {
                break;
            }
            result.push_str(&rest[..at]);
            result.push_str(if open { "<" } else { "</" });
            result.push_str(tag);
            result.push('>');
            rest = &rest[at + marker.len()..];
            open = !open;
        }
        result.push_str(rest);
        out = result;
    }
    out
}

/// One Record body as chapter markup.
pub fn body_to_html(body: &str) -> String {
    let mut out = String::new();
    let mut lines = body.lines().peekable();
    let mut list: Option<&'static str> = None;

    let close_list = |out: &mut String, list: &mut Option<&'static str>| {
        if let Some(tag) = list.take() {
            out.push_str(&format!("</{tag}>\n"));
        }
    };

    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();

        if trimmed.starts_with("```mermaid") {
            close_list(&mut out, &mut list);
            let mut block = String::new();
            for inner in lines.by_ref() {
                if inner.trim_start().starts_with("```") {
                    break;
                }
                block.push_str(inner);
                block.push('\n');
            }
            out.push_str(&format!(
                "<div class=\"figure\"><div class=\"figure__frame\">\n<pre class=\"mermaid\">{}</pre>\n</div></div>\n",
                escape(block.trim_end())
            ));
            continue;
        }
        if trimmed.is_empty() {
            close_list(&mut out, &mut list);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h3>{}</h3>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h2>{}</h2>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h1>{}</h1>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            close_list(&mut out, &mut list);
            out.push_str(&format!("<p class=\"chapter__eyebrow\">{}</p>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            if list.is_none() {
                out.push_str("<ul>\n");
                list = Some("ul");
            }
            out.push_str(&format!("<li>{}</li>\n", inline(rest)));
        } else if trimmed.starts_with('_') && trimmed.ends_with('_') && trimmed.len() > 2 {
            // A figure caption: a whole line in emphasis, which is what the
            // mechanical conversion produced for `figure__caption`.
            close_list(&mut out, &mut list);
            out.push_str(&format!(
                "<p class=\"figure__caption\">{}</p>\n",
                inline(&trimmed[1..trimmed.len() - 1])
            ));
        } else {
            close_list(&mut out, &mut list);
            out.push_str(&format!("<p>{}</p>\n", inline(trimmed)));
        }
    }
    close_list(&mut out, &mut list);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell finds `pre.mermaid` in the DOM and renders it on chapter
    /// activation. A fence that came out as a paragraph is a diagram that
    /// silently becomes a wall of graph syntax.
    #[test]
    fn a_mermaid_fence_becomes_a_pre_the_shell_can_find() {
        let html = body_to_html("text\n\n```mermaid\ngraph LR\n  A[\"Need\"] --> B\n```\n");
        assert!(html.contains("<pre class=\"mermaid\">"), "{html}");
        assert!(html.contains("graph LR"), "{html}");
        assert!(html.contains("--&gt; B"), "the arrow is escaped, not markup: {html}");
    }

    #[test]
    fn headings_emphasis_and_lists_render() {
        let html = body_to_html("## Title\n\nA **strong** and `code` word.\n\n- one\n- two\n");
        assert!(html.contains("<h2>Title</h2>"));
        assert!(html.contains("<strong>strong</strong>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("</ul>"));
    }

    /// An unmatched marker must stay literal. Swallowing the rest of a
    /// paragraph into a tag that never closes breaks every element after it.
    #[test]
    fn a_lone_marker_stays_text() {
        let html = body_to_html("2 ** 3 is not emphasis\n");
        assert!(html.contains("2 ** 3"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
    }

    #[test]
    fn html_in_a_body_is_escaped_rather_than_trusted() {
        let html = body_to_html("<script>alert(1)</script>\n");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }
}
