//! Record bodies (Markdown) -> the chapter markup the Instinct shell expects.
//!
//! Deliberately NOT a general Markdown implementation, and deliberately not a
//! dependency. It renders exactly the subset root `anicca/*.lingua` uses, and
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

/// `**bold**`, `_em_` and `` `code` ``, on already-escaped text.
fn inline(text: &str) -> String {
    let mut out = escape(text);
    for (marker, tag) in [("**", "strong"), ("`", "code")] {
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
    emphasis(&out)
}

/// `_em_`, but only where an underscore is a MARKER rather than a letter.
///
/// This corpus is full of `file_sync`, `record.body`, `mantissa TEXT` — an
/// underscore between two word characters is part of an identifier and nothing
/// else. Emphasis therefore has to open on a word boundary and close on one:
/// `_second_` is emphasis, `file_sync` is a name. Counting underscores instead
/// (the old rule) only survived because paragraphs were one line long; joining
/// wrapped lines back together would let `record_editor` on one line pair with
/// `file_sync` three lines down and italicise everything between them.
fn emphasis(text: &str) -> String {
    let bytes = text.as_bytes();
    let word = |index: usize| -> bool {
        bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    };
    // Not an underscore: a run of them is a blank to be signed on a form.
    let letter = |index: usize| word(index) && bytes.get(index) != Some(&b'_');
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(offset) = text[at..].find('_') {
        let open = at + offset;
        out.push_str(&text[at..open]);
        // An opener has a non-word character (or nothing) before it, and the
        // emphasised run has to start with one.
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

/// One Record body as chapter markup.
///
/// **The body on screen is the body in the file, line for line.** Where the
/// writer broke a line, the reader sees a break — the Record is the document,
/// and how it is set is theirs to decide by editing it, not this renderer's to
/// decide by reflowing it.
///
/// A wrapped line is still not a BLOCK, though, and that is the difference from
/// emitting one element per source line: consecutive lines are one paragraph,
/// or one bullet, or one quote, parsed as a whole and then broken with `<br>`.
/// Read line by line instead, a `**` opened before a wrap point could never
/// close after it and stayed on screen as literal asterisks, a wrapped bullet
/// became a new `<ul>` per line, and — because every line was its own `<p>` —
/// the gap between two lines of one paragraph was the same as the gap between
/// two paragraphs, so the file's paragraphs were invisible.
pub fn body_to_html(body: &str) -> String {
    let mut out = String::new();
    let mut lines = body.lines().peekable();
    let mut list: Option<&'static str> = None;
    // The block being accumulated: its lines, and what kind of block it is.
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
        // Read as ONE block, so a `**` opened on one line closes on the next —
        // then broken again exactly where the writer broke it. The newline
        // survives `escape()` untouched, so this order is what lets the two be
        // true at the same time.
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
            // A figure caption: a whole block in emphasis, which is what the
            // mechanical conversion produced for `figure__caption`.
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
            // A quote wraps like everything else, and its continuation lines
            // carry the `>` too — so it accumulates rather than emitting.
            if kind != Block::Quote {
                flush(&mut out, &mut buffer, &mut kind);
                close_list(&mut out, &mut list);
            }
            kind = Block::Quote;
            buffer.push(rest.trim_start());
        } else if let Some(rest) = trimmed.trim_start().strip_prefix("- ") {
            // A `- ` at the start of a line opens a bullet even when the
            // previous one is still open; an indented continuation of a bullet
            // never starts with one, so this is not ambiguous.
            flush(&mut out, &mut buffer, &mut kind);
            if list.is_none() {
                out.push_str("<ul>\n");
                list = Some("ul");
            }
            kind = Block::Item;
            buffer.push(rest);
        } else {
            // Prose. Inside an open bullet this is the rest of that bullet;
            // otherwise it is the rest of the paragraph. Either way it is a
            // continuation, never a new block — only a blank line ends one.
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

    /// The shell finds `pre.mermaid` in the DOM and renders it on chapter
    /// activation. A fence that came out as a paragraph is a diagram that
    /// silently becomes a wall of graph syntax.
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

    /// An unmatched marker must stay literal. Swallowing the rest of a
    /// paragraph into a tag that never closes breaks every element after it.
    #[test]
    fn a_lone_marker_stays_text() {
        let html = body_to_html("2 ** 3 is not emphasis\n");
        assert!(html.contains("2 ** 3"), "{html}");
        assert!(!html.contains("<strong>"), "{html}");
    }

    /// The writer's line breaks are the writer's. What a wrapped line must NOT
    /// do is become its own block: one `<p>` per line spaced the lines of a
    /// paragraph as far apart as the paragraphs themselves, and one `<ul>` per
    /// line turned a three-line bullet into three lists.
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

    /// Emphasis that spans a wrap point used to come out as literal asterisks,
    /// because each line was closed off on its own. It pairs across the break
    /// without swallowing it.
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

    /// An underscore between two word characters is an identifier, not a
    /// marker. Joining lines makes this load-bearing: `record_editor` in one
    /// line would otherwise pair with `file_sync` in the next.
    #[test]
    fn an_underscore_inside_a_name_is_not_emphasis() {
        let html = body_to_html(
            "The `lince.file_sync` config and record_editor\nboth read store_state here.\n",
        );
        assert!(!html.contains("<em>"), "{html}");
        assert!(html.contains("record_editor"), "{html}");
        assert!(html.contains("store_state"), "{html}");
    }

    /// The Institute's founding minutes are a form with blanks to sign in. A
    /// run of underscores is not emphasis, however many of them there are.
    #[test]
    fn a_blank_to_be_filled_in_is_not_emphasis() {
        let html = body_to_html("Aos ___ dias do mês de __________ de ________, na cidade.\n");
        assert!(!html.contains("<em>"), "{html}");
    }

    /// A blockquote is hard-wrapped like everything else, and each of its lines
    /// carries the `>`. Emitted one at a time, the bold that opened on the
    /// first line never closed.
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
