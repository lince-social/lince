use std::{ops::Range, path::Path};

pub fn detect(path: &Path) -> Option<&'static str> {
    Some(match path.extension()?.to_str()? {
        "rs" => "rust",
        "nix" => "nix",
        "sh" | "bash" => "shellscript",
        "py" => "python",
        "js" | "jsx" | "mjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "json" => "json",
        "toml" => "toml",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        _ => return None,
    })
}

#[derive(Clone, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tools {
    pub server: Vec<String>,
    pub formatter: Vec<String>,
    pub linter: Vec<String>,
}

impl Tools {
    pub fn valid(&self) -> bool {
        [&self.server, &self.formatter, &self.linter]
            .into_iter()
            .all(|command| {
                command.len() <= 32
                    && command
                        .iter()
                        .all(|arg| arg.len() <= 4096 && !arg.contains('\0'))
            })
    }

    pub fn for_language(language: &str) -> Self {
        let (server, formatter, linter): (&[&str], &[&str], &[&str]) = match language {
            "rust" => (&["rust-analyzer"], &["rustfmt", "--emit", "stdout"], &[]),
            "nix" => (&["nixd"], &["nixfmt"], &["statix", "check", "--stdin"]),
            "shellscript" => (
                &["bash-language-server", "start"],
                &["shfmt"],
                &["shellcheck", "-"],
            ),
            "python" => (
                &["pylsp"],
                &["ruff", "format", "-"],
                &["ruff", "check", "-"],
            ),
            "javascript" | "typescript" => (&["typescript-language-server", "--stdio"], &[], &[]),
            "toml" => (&["taplo", "lsp", "stdio"], &["taplo", "fmt", "-"], &[]),
            "json" => (&["vscode-json-language-server", "--stdio"], &[], &[]),
            "c" | "cpp" => (&["clangd"], &["clang-format"], &[]),
            _ => (&[], &[], &[]),
        };
        Self {
            server: server.iter().map(|s| (*s).into()).collect(),
            formatter: formatter.iter().map(|s| (*s).into()).collect(),
            linter: linter.iter().map(|s| (*s).into()).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Comment,
    String,
    Number,
    Keyword,
}

pub fn highlight(text: &str, language: &str) -> Vec<(Range<usize>, Style)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    let bytes = text.as_bytes();
    let hash_comment = matches!(language, "nix" | "shellscript" | "python" | "toml");
    while cursor < bytes.len() && spans.len() < 8192 {
        let start = cursor;
        let rest = &text[cursor..];
        let style = if (hash_comment && rest.starts_with('#'))
            || (!hash_comment && rest.starts_with("//"))
        {
            cursor += rest.find('\n').unwrap_or(rest.len());
            Some(Style::Comment)
        } else if !matches!(language, "shellscript" | "python" | "toml" | "json")
            && rest.starts_with("/*")
        {
            cursor += rest[2..].find("*/").map_or(rest.len(), |end| end + 4);
            Some(Style::Comment)
        } else if language == "rust"
            && bytes[cursor] == b'\''
            && rest.chars().nth(1) != Some('\\')
            && rest.chars().nth(2) != Some('\'')
        {
            cursor += 1;
            None
        } else if matches!(bytes[cursor], b'"' | b'\'' | b'`') {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' {
                    cursor = (cursor + 2).min(bytes.len());
                } else if bytes[cursor] == quote {
                    cursor += 1;
                    break;
                } else {
                    cursor += 1;
                }
            }
            while !text.is_char_boundary(cursor) {
                cursor += 1;
            }
            Some(Style::String)
        } else if bytes[cursor].is_ascii_digit() {
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'.' | b'_'))
            {
                cursor += 1;
            }
            Some(Style::Number)
        } else if bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_' {
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            matches!(
                &text[start..cursor],
                "fn" | "let"
                    | "mut"
                    | "pub"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "use"
                    | "mod"
                    | "match"
                    | "if"
                    | "else"
                    | "then"
                    | "elif"
                    | "fi"
                    | "for"
                    | "while"
                    | "do"
                    | "done"
                    | "in"
                    | "with"
                    | "inherit"
                    | "rec"
                    | "assert"
                    | "return"
                    | "break"
                    | "continue"
                    | "true"
                    | "false"
                    | "null"
                    | "None"
                    | "Some"
                    | "async"
                    | "await"
                    | "const"
                    | "static"
                    | "class"
                    | "def"
                    | "import"
                    | "from"
                    | "function"
                    | "export"
                    | "var"
                    | "new"
                    | "try"
                    | "catch"
                    | "switch"
                    | "case"
                    | "self"
                    | "Self"
                    | "unsafe"
                    | "where"
                    | "as"
                    | "type"
                    | "int"
                    | "void"
                    | "char"
                    | "bool"
                    | "auto"
                    | "include"
            )
            .then_some(Style::Keyword)
        } else {
            cursor += rest.chars().next().unwrap().len_utf8();
            None
        };
        if let Some(style) = style {
            spans.push((start..cursor, style));
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexical_highlighting_stays_inside_unicode_boundaries_and_is_bounded() {
        let text = "let 猫 = \"hello 🐈\"; // comment\n42";
        let spans = highlight(text, "rust");
        assert_eq!(&text[spans[0].0.clone()], "let");
        assert!(spans.iter().any(
            |(range, style)| *style == Style::String && &text[range.clone()] == "\"hello 🐈\""
        ));
        assert!(
            spans
                .iter()
                .all(|(range, _)| text.get(range.clone()).is_some())
        );
        assert!(highlight(&"let ".repeat(20_000), "rust").len() <= 8192);
        assert!(
            !highlight("fn f<'a>(value: &'a str) {}", "rust")
                .iter()
                .any(|(_, style)| *style == Style::String)
        );
    }
}
