// non-lean-yet
// Scope: Legacy Rust comment scanner and stripper exercised by the scanner tests.
// BeautifulRust (baseline lines):
// - [manual: §6 Idioms] Sites 11–13, 113–119. Rely on Rule's identical extension default
//   and combine the raw-string guard with the end-position lookup.
// - [manual: §4 Types, §10 Documentation] Sites 29–35, 54–132. Keep comment ranges and
//   safety-proof continuation state private or document their UTF-8 boundary invariant.
// - [manual: §5 Errors, §11 Tests] Sites 157–207, 278–291. Preserve the whitespace needed
//   between tokens and raw-string bytes when removing a comment. An escaped newline inside
//   a string currently advances the byte cursor without incrementing its line counter.
//   Cover these cases before using the legacy stripper on source again.

use std::path::Path;

use crate::rules::{Finding, Rule};

pub struct NoComments;

impl Rule for NoComments {
    fn name(&self) -> &'static str {
        "Lince source carries no comments. These do:"
    }

    fn wants(&self, path: &Path) -> bool {
        path.extension().and_then(|value| value.to_str()) == Some("rs")
    }

    fn remedy(&self) -> &'static str {
        "General reasoning belongs in a Record under institute/anicca/. Preserve required attribution and local safety proofs written with `// SAFETY:`."
    }

    fn inspect(&self, source: &str) -> Vec<Finding> {
        findings_in(source)
    }

    fn fix(&self, source: &str) -> Option<String> {
        let mended = strip(source);
        (mended != source).then_some(mended)
    }
}

pub struct Comment {
    pub line: usize,
    pub opener: &'static str,
    pub start: usize,
    pub end: usize,
    pub permitted: bool,
}

const ATTRIBUTIONS: [&str; 10] = [
    "SPDX-License-Identifier",
    "Copyright",
    "\u{a9}",
    "Licensed under",
    "License:",
    "Licence:",
    "Credits:",
    "Vendored from",
    "Upstream:",
    "@generated",
];

pub fn is_attribution(text: &str) -> bool {
    ATTRIBUTIONS.iter().any(|marker| text.contains(marker))
}

pub fn comments_in(source: &str) -> Vec<Comment> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut line = 1usize;
    let mut at = 0usize;
    let mut safety_line = None;

    while at < bytes.len() {
        let byte = bytes[at];

        if byte == b'\n' {
            line += 1;
            at += 1;
            continue;
        }

        if byte == b'/' && bytes.get(at + 1) == Some(&b'/') {
            let start = at;
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            let text = &source[start..at];
            let indent = source[..start].rsplit('\n').next().unwrap_or_default();
            let standalone = indent.bytes().all(|byte| matches!(byte, b' ' | b'\t'));
            let safety = standalone
                && match text.strip_prefix("// SAFETY:") {
                    Some(proof) => !proof.trim().is_empty(),
                    None => {
                        safety_line == Some((line - 1, indent))
                            && text
                                .strip_prefix("// ")
                                .is_some_and(|text| !text.trim().is_empty())
                    }
                };
            safety_line = safety.then_some((line, indent));
            found.push(Comment {
                line,
                opener: "//",
                start,
                end: at,
                permitted: safety || is_attribution(text),
            });
            continue;
        }

        if byte == b'/' && bytes.get(at + 1) == Some(&b'*') {
            let start = at;
            let opened_on = line;
            at = block_end(bytes, at, &mut line);
            found.push(Comment {
                line: opened_on,
                opener: "/*",
                start,
                end: at,
                permitted: is_attribution(&source[start..at]),
            });
            continue;
        }

        let previous = if at == 0 { b' ' } else { bytes[at - 1] };
        if matches!(byte, b'b' | b'c' | b'r')
            && !is_word(previous)
            && let Some(next) = raw_string_end(bytes, at, &mut line)
        {
            at = next;
            continue;
        }

        if byte == b'"' {
            at = string_end(bytes, at, &mut line);
            continue;
        }

        if byte == b'\'' {
            at = quote_end(bytes, at, &mut line);
            continue;
        }

        at += 1;
    }

    found
}

pub fn findings_in(source: &str) -> Vec<Finding> {
    comments_in(source)
        .into_iter()
        .filter(|comment| !comment.permitted)
        .map(|comment| Finding {
            line: comment.line,
            complaint: format!("carries a `{}`", comment.opener),
        })
        .collect()
}

pub fn strip(source: &str) -> String {
    let comments: Vec<Comment> = comments_in(source)
        .into_iter()
        .filter(|comment| !comment.permitted)
        .collect();
    if comments.is_empty() {
        return source.to_string();
    }

    let mut removed = vec![false; source.len()];
    for comment in &comments {
        for flag in removed.iter_mut().take(comment.end).skip(comment.start) {
            *flag = true;
        }
    }

    let bytes = source.as_bytes();
    let ends_with_newline = source.ends_with('\n');
    let mut kept: Vec<String> = Vec::new();
    let mut dropped_before = false;
    let mut at = 0usize;

    while at <= bytes.len() {
        let end = match bytes[at..].iter().position(|byte| *byte == b'\n') {
            Some(offset) => at + offset,
            None => bytes.len(),
        };
        let had_comment = (at..end).any(|index| removed[index]);
        let residue: Vec<u8> = (at..end)
            .filter(|index| !removed[*index])
            .map(|index| bytes[index])
            .collect();
        let text = String::from_utf8_lossy(&residue).trim_end().to_string();

        if had_comment && text.is_empty() {
            dropped_before = true;
        } else if text.is_empty()
            && dropped_before
            && kept
                .last()
                .map(std::string::String::is_empty)
                .unwrap_or(true)
        {
            dropped_before = false;
        } else {
            dropped_before = false;
            kept.push(text);
        }

        if end >= bytes.len() {
            break;
        }
        at = end + 1;
    }

    while kept
        .last()
        .map(std::string::String::is_empty)
        .unwrap_or(false)
    {
        kept.pop();
    }

    let mut out = kept.join("\n");
    if ends_with_newline && !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Removes ordinary comments while preserving documentation, safety, and attribution.
pub fn strip_ordinary(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for comment in comments_in(source) {
        let text = &source[comment.start..comment.end];
        if comment.permitted
            || text.contains("SAFETY:")
            || text.starts_with("//!")
            || text.starts_with("/*!")
            || text.starts_with("///") && !text.starts_with("////")
            || text.starts_with("/**") && !text.starts_with("/***") && text != "/**/"
            || text.trim_end() == "// non-lean-yet"
            || text.contains("rustfmt::skip")
        {
            continue;
        }
        output.push_str(&source[cursor..comment.start]);
        // A block comment separates tokens even when it touches both of them.
        if comment.opener == "/*" {
            output.push(' ');
        }
        output.extend(text.chars().filter(|&c| matches!(c, '\n' | '\r')));
        cursor = comment.end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn block_end(bytes: &[u8], start: usize, line: &mut usize) -> usize {
    let mut at = start + 2;
    let mut depth = 1usize;
    while at < bytes.len() && depth > 0 {
        if bytes[at] == b'\n' {
            *line += 1;
            at += 1;
        } else if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'*') {
            depth += 1;
            at += 2;
        } else if bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/') {
            depth -= 1;
            at += 2;
        } else {
            at += 1;
        }
    }
    at
}

fn raw_string_end(bytes: &[u8], start: usize, line: &mut usize) -> Option<usize> {
    let mut at = start;
    if bytes.get(at) == Some(&b'b') || bytes.get(at) == Some(&b'c') {
        at += 1;
    }
    if bytes.get(at) != Some(&b'r') {
        return None;
    }
    at += 1;
    let hashes = {
        let from = at;
        while bytes.get(at) == Some(&b'#') {
            at += 1;
        }
        at - from
    };
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    at += 1;

    while at < bytes.len() {
        if bytes[at] == b'\n' {
            *line += 1;
            at += 1;
            continue;
        }
        if bytes[at] == b'"' {
            let closed = bytes
                .iter()
                .skip(at + 1)
                .take(hashes)
                .take_while(|byte| **byte == b'#')
                .count();
            if closed == hashes {
                return Some(at + 1 + hashes);
            }
        }
        at += 1;
    }
    Some(at)
}

fn string_end(bytes: &[u8], start: usize, line: &mut usize) -> usize {
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => {
                *line += usize::from(bytes.get(at + 1) == Some(&b'\n'));
                at += 2;
            }
            b'\n' => {
                *line += 1;
                at += 1;
            }
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    at
}

fn quote_end(bytes: &[u8], start: usize, line: &mut usize) -> usize {
    if bytes.get(start + 1) == Some(&b'\\') {
        let mut at = start + 1;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 2,
                b'\n' => {
                    *line += 1;
                    at += 1;
                }
                b'\'' => return at + 1,
                _ => at += 1,
            }
        }
        return at;
    }

    let width = utf8_width(bytes.get(start + 1).copied().unwrap_or(0));
    if bytes.get(start + 1 + width) == Some(&b'\'') {
        return start + 2 + width;
    }

    start + 1
}

fn utf8_width(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}
