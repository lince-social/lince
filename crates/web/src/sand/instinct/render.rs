fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn inline(text: &str) -> String {
    let mut out = escape(text);
    for (marker, tag) in [("**", "strong"), ("`", "code")] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        let mut open = true;
        while let Some(at) = rest.find(marker) {
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
    emphasis(&out)
}

fn emphasis(text: &str) -> String {
    let bytes = text.as_bytes();
    let word = |index: usize| -> bool {
        bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    };
    let letter = |index: usize| word(index) && bytes.get(index) != Some(&b'_');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(offset) = text[at..].find('_') {
        let open = at + offset;
        out.push_str(&text[at..open]);
        let opens = (open == 0 || !word(open - 1)) && letter(open + 1);
        let close = opens
            .then(|| {
                text[open + 1..]
                    .match_indices('_')
                    .map(|(index, _)| open + 1 + index)
                    .find(|end| letter(end - 1) && !word(end + 1))
            })
            .flatten();
        match close {
            Some(end) => {
                out.push_str("<em>");
                out.push_str(&text[open + 1..end]);
                out.push_str("</em>");
                at = end + 1;
            }
            None => {
                out.push('_');
                at = open + 1;
            }
        }
    }
    out.push_str(&text[at..]);
    out
}

pub fn body_to_html(body: &str) -> String {
    let mut out = String::new();
    let mut lines = body.lines().peekable();
    let mut list: Option<&'static str> = None;
    let mut buffer: Vec<&str> = Vec::new();
    let mut kind = Block::Prose;

    #[derive(PartialEq, Clone, Copy)]
    enum Block {
        Prose,
        Item,
        Quote,
    }

    fn flush(out: &mut String, buffer: &mut Vec<&str>, kind: &mut Block) {
        if buffer.is_empty() {
            return;
        }
        let text = buffer.join("\n");
        buffer.clear();
        let markup = |text: &str| inline(text).replace('\n', "<br>\n");
        match std::mem::replace(kind, Block::Prose) {
            Block::Item => return out.push_str(&format!("<li>{}</li>\n", markup(&text))),
            Block::Quote => {
                return out.push_str(&format!(
                    "<p class=\"chapter__eyebrow\">{}</p>\n",
                    markup(&text)
                ));
            }
            Block::Prose => {}
        }
        if text.starts_with('_') && text.ends_with('_') && text.len() > 2 {
            out.push_str(&format!(
                "<p class=\"figure__caption\">{}</p>\n",
                markup(&text[1..text.len() - 1])
            ));
        } else {
            out.push_str(&format!("<p>{}</p>\n", markup(&text)));
        }
    }

    let close_list = |out: &mut String, list: &mut Option<&'static str>| {
        if let Some(tag) = list.take() {
            out.push_str(&format!("</{tag}>\n"));
        }
    };

    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();

        if trimmed.starts_with("```mermaid") {
            flush(&mut out, &mut buffer, &mut kind);
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
        if trimmed.trim().is_empty() {
            flush(&mut out, &mut buffer, &mut kind);
            close_list(&mut out, &mut list);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            flush(&mut out, &mut buffer, &mut kind);
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h3>{}</h3>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            flush(&mut out, &mut buffer, &mut kind);
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h2>{}</h2>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            flush(&mut out, &mut buffer, &mut kind);
            close_list(&mut out, &mut list);
            out.push_str(&format!("<h1>{}</h1>\n", inline(rest)));
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            if kind != Block::Quote {
                flush(&mut out, &mut buffer, &mut kind);
                close_list(&mut out, &mut list);
            }
            kind = Block::Quote;
            buffer.push(rest.trim_start());
        } else if let Some(rest) = trimmed.trim_start().strip_prefix("- ") {
            flush(&mut out, &mut buffer, &mut kind);
            if list.is_none() {
                out.push_str("<ul>\n");
                list = Some("ul");
            }
            kind = Block::Item;
            buffer.push(rest);
        } else {
            buffer.push(trimmed.trim_start());
        }
    }
    flush(&mut out, &mut buffer, &mut kind);
    close_list(&mut out, &mut list);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mermaid_fence_becomes_a_pre_the_shell_can_find() {
        let html = body_to_html("text\n\n```mermaid\ngraph LR\n  A[\"Need\"] --> B\n```\n");
        assert!(html.contains("<pre class=\"mermaid\">"), "{html}");
        assert!(html.contains("graph LR"), "{html}");
        assert!(
            html.contains("--&gt; B"),
            "the arrow is escaped, not markup: {html}"
        );
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

    #[test]
    fn a_lone_marker_stays_text() {
        let html = body_to_html("2 ** 3 is not emphasis\n");
        assert!(html.contains("2 ** 3"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
    }

    #[test]
    fn wrapped_lines_break_where_the_file_breaks_but_stay_one_block() {
        let html = body_to_html(
            "Open Lince and there is no menu bar, no sidebar, no home\nscreen waiting to be navigated.\n\n- a bullet that runs on\n  past the wrap column\n- a second one\n",
        );
        assert_eq!(html.matches("<p>").count(), 1, "one paragraph: {html}");
        assert!(
            html.contains("no home<br>\nscreen waiting"),
            "the break is kept: {html}"
        );
        assert_eq!(html.matches("<li>").count(), 2, "{html}");
        assert!(
            html.contains("<li>a bullet that runs on<br>\npast the wrap column</li>"),
            "{html}"
        );
        assert_eq!(
            html.matches("<ul>").count(),
            1,
            "one list, not one per line: {html}"
        );
    }

    #[test]
    fn emphasis_spanning_a_wrap_point_still_pairs() {
        let html =
            body_to_html("this is deliberately **not YAML\nfront matter**: each line is Lingua.\n");
        assert!(
            html.contains("<strong>not YAML<br>\nfront matter</strong>"),
            "{html}"
        );
        assert!(!html.contains("**"), "{html}");
    }

    #[test]
    fn an_underscore_inside_a_name_is_not_emphasis() {
        let html = body_to_html(
            "The `lince.file_sync` config and record_editor\nboth read store_state here.\n",
        );
        assert!(!html.contains("<em>"), "{html}");
        assert!(html.contains("record_editor"), "{html}");
        assert!(html.contains("store_state"), "{html}");
    }

    #[test]
    fn a_blank_to_be_filled_in_is_not_emphasis() {
        let html = body_to_html("Aos ___ dias do mês de __________ de ________, na cidade.\n");
        assert!(!html.contains("<em>"), "{html}");
    }

    #[test]
    fn a_wrapped_quote_is_one_quote() {
        let html = body_to_html(
            "> **Records model things. Assertions\n> model what is said about them.**\n",
        );
        assert_eq!(html.matches("chapter__eyebrow").count(), 1, "{html}");
        assert!(html.contains("<strong>Records model things. Assertions<br>\nmodel what is said about them.</strong>"), "{html}");
    }

    #[test]
    fn emphasis_on_word_boundaries_still_renders() {
        let html = body_to_html("A _second_ record, and _Lynx canadensis_ too.\n");
        assert!(html.contains("<em>second</em>"), "{html}");
        assert!(html.contains("<em>Lynx canadensis</em>"), "{html}");
    }

    #[test]
    fn html_in_a_body_is_escaped_rather_than_trusted() {
        let html = body_to_html("<script>alert(1)</script>\n");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }
}
