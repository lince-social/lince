use super::{DslErrorKind, KarmaDslError, MAX_DSL_STRING_BYTES, MAX_DSL_TOKENS, Position};

#[derive(PartialEq, Eq)]
pub(super) enum TokenKind<'source> {
    Word(&'source str),
    String(String),
    Symbol(char),
}

pub(super) struct Token<'source> {
    pub(super) kind: TokenKind<'source>,
    pub(super) position: Position,
}

pub(super) fn lex(source: &str) -> Result<(Vec<Token<'_>>, Position), KarmaDslError> {
    let mut tokens = Vec::new();
    let mut position = Position::START;
    let bytes = source.as_bytes();
    while let Some(&byte) = bytes.get(position.offset) {
        let start = position;
        let kind = match byte {
            b' ' | b'\t' | b'\r' | b'\n' => {
                position.advance(byte);
                continue;
            }
            b'#' => {
                for &byte in &bytes[position.offset..] {
                    if byte == b'\n' {
                        break;
                    }
                    position.advance(byte);
                }
                continue;
            }
            b'"' => TokenKind::String(read_string(source, &mut position)?),
            byte if is_symbol(byte) => {
                position.advance(byte);
                TokenKind::Symbol(char::from(byte))
            }
            byte if byte.is_ascii() && !byte.is_ascii_control() => {
                for &byte in &bytes[position.offset..] {
                    if byte.is_ascii_whitespace() || byte == b'#' || is_symbol(byte) {
                        break;
                    }
                    if !byte.is_ascii() || byte.is_ascii_control() {
                        return Err(position.error(
                            DslErrorKind::UnexpectedCharacter,
                            "semantic DSL tokens must be ASCII",
                        ));
                    }
                    position.advance(byte);
                }
                // Word scanning advances only over ASCII, preserving UTF-8 boundaries.
                TokenKind::Word(&source[start.offset..position.offset])
            }
            _ => {
                return Err(position.error(
                    DslErrorKind::UnexpectedCharacter,
                    "Unicode is allowed only inside string literals",
                ));
            }
        };
        tokens.push(Token {
            kind,
            position: start,
        });
        if tokens.len() > MAX_DSL_TOKENS {
            return Err(position.error(
                DslErrorKind::TooManyTokens,
                format!("DSL exceeds {MAX_DSL_TOKENS} tokens"),
            ));
        }
    }
    Ok((tokens, position))
}

fn read_string(source: &str, position: &mut Position) -> Result<String, KarmaDslError> {
    let start = *position;
    position.advance(b'"');
    let mut escaped = false;
    for &byte in &source.as_bytes()[position.offset..] {
        position.advance(byte);
        if byte == b'"' && !escaped {
            // The two ASCII quotes delimit complete UTF-8 characters.
            let decoded: String = serde_json::from_str(&source[start.offset..position.offset])
                .map_err(|_| {
                    start.error(
                        DslErrorKind::InvalidAtom,
                        "string must use valid JSON escaping",
                    )
                })?;
            if decoded.len() > MAX_DSL_STRING_BYTES {
                return Err(start.error(
                    DslErrorKind::StringTooLarge,
                    format!("decoded string exceeds {MAX_DSL_STRING_BYTES} bytes"),
                ));
            }
            return Ok(decoded);
        }
        escaped = !escaped && byte == b'\\';
    }
    Err(start.error(DslErrorKind::UnexpectedEnd, "unterminated string literal"))
}

fn is_symbol(byte: u8) -> bool {
    matches!(
        byte,
        b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',' | b';' | b':' | b'='
    )
}
