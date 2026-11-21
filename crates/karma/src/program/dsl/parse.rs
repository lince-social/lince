use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
    vec::IntoIter,
};

use serde::de::{DeserializeOwned, value::BorrowedStrDeserializer};

use crate::{DurationMs, FrequencyAst, KarmaBoundaryError, LocalId, ProgramAst, Slug};

use super::{
    DslErrorKind, KarmaDslError, MAX_DSL_BYTES, MAX_DSL_NESTING, Position,
    lexer::{Token, TokenKind, lex},
};

mod frequency;
mod program;
mod values;

/// Parses a Karma program from versioned DSL text.
///
/// # Errors
///
/// Returns a positioned diagnostic for invalid syntax, atoms, or resource limits.
pub fn parse_program(source: &str) -> Result<ProgramAst, KarmaDslError> {
    Parser::new(source)?.parse_program()
}

/// Parses and validates a Karma Frequency from versioned DSL text.
///
/// # Errors
///
/// Returns a positioned diagnostic for invalid syntax, definitions, or resource limits.
pub fn parse_frequency(source: &str) -> Result<FrequencyAst, KarmaDslError> {
    Parser::new(source)?.parse_frequency()
}

struct Parser<'source> {
    tokens: IntoIter<Token<'source>>,
    last: Position,
}

impl<'source> Parser<'source> {
    fn new(source: &'source str) -> Result<Self, KarmaDslError> {
        if source.len() > MAX_DSL_BYTES {
            return Err(Position::START.error(
                DslErrorKind::SourceTooLarge,
                format!("DSL source exceeds {MAX_DSL_BYTES} bytes"),
            ));
        }
        let (tokens, end) = lex(source)?;
        let last = tokens.first().map_or(end, |token| token.position);
        Ok(Self {
            tokens: tokens.into_iter(),
            last,
        })
    }

    fn finished(&self, message: &str) -> Result<(), KarmaDslError> {
        if self.tokens.as_slice().is_empty() {
            Ok(())
        } else {
            Err(self.error_here(DslErrorKind::TrailingInput, message))
        }
    }

    fn parse_slug_set(&mut self) -> Result<BTreeSet<Slug>, KarmaDslError> {
        self.parse_set("tag", |parser| {
            Slug::new(parser.take_word("tag slug")?).map_err(|error| parser.atom(error))
        })
    }

    fn parse_set<T: Ord>(
        &mut self,
        label: &str,
        mut element: impl FnMut(&mut Self) -> Result<T, KarmaDslError>,
    ) -> Result<BTreeSet<T>, KarmaDslError> {
        self.expect_symbol('[')?;
        let mut values = BTreeSet::new();
        if !self.consume_symbol(']') {
            loop {
                if !values.insert(element(self)?) {
                    return Err(self.duplicate(label));
                }
                if self.consume_symbol(']') {
                    break;
                }
                self.expect_symbol(',')?;
            }
        }
        Ok(values)
    }

    fn parse_id_map<T>(
        &mut self,
        label: &str,
        separator: char,
        mut element: impl FnMut(&mut Self) -> Result<T, KarmaDslError>,
    ) -> Result<BTreeMap<LocalId, T>, KarmaDslError> {
        self.expect_symbol('{')?;
        let mut fields = BTreeMap::new();
        if !self.consume_symbol('}') {
            loop {
                let id = self.parse_local_id(label)?;
                self.expect_symbol(separator)?;
                let value = element(self)?;
                self.insert_unique(&mut fields, id, value)?;
                if self.consume_symbol('}') {
                    break;
                }
                self.expect_symbol(',')?;
            }
        }
        Ok(fields)
    }

    fn parse_local_id(&mut self, label: &str) -> Result<LocalId, KarmaDslError> {
        LocalId::new(self.take_word(label)?).map_err(|error| self.atom(error))
    }

    fn parse_duration_constructor(&mut self, label: &str) -> Result<DurationMs, KarmaDslError> {
        self.expect_word("duration")?;
        self.parse_duration_argument(label)
    }

    fn parse_duration_argument(&mut self, label: &str) -> Result<DurationMs, KarmaDslError> {
        self.expect_symbol('(')?;
        let duration = DurationMs::new(self.parse_atom(label)?);
        self.expect_symbol(')')?;
        Ok(duration)
    }

    fn parse_enum_atom<T: DeserializeOwned>(&mut self, label: &str) -> Result<T, KarmaDslError> {
        let word = self.take_word(label)?;
        T::deserialize(BorrowedStrDeserializer::<serde::de::value::Error>::new(
            word,
        ))
        .map_err(|_| self.invalid_atom(label, word))
    }

    fn parse_bool(&mut self) -> Result<bool, KarmaDslError> {
        self.parse_atom("Boolean literal")
    }

    fn parse_atom<T: FromStr>(&mut self, label: &str) -> Result<T, KarmaDslError> {
        let word = self.take_word(label)?;
        word.parse().map_err(|_| self.invalid_atom(label, word))
    }

    fn invalid_atom(&self, label: &str, word: &str) -> KarmaDslError {
        self.error_here(
            DslErrorKind::InvalidAtom,
            format!("invalid {label} {word:?}"),
        )
    }

    fn check_depth(&self, depth: usize) -> Result<(), KarmaDslError> {
        if depth > MAX_DSL_NESTING {
            Err(self.error_here(
                DslErrorKind::NestingTooDeep,
                format!("DSL nesting exceeds {MAX_DSL_NESTING}"),
            ))
        } else {
            Ok(())
        }
    }

    fn set_once<T>(
        &self,
        target: &mut Option<T>,
        value: T,
        label: &str,
    ) -> Result<(), KarmaDslError> {
        if target.replace(value).is_some() {
            Err(self.duplicate(label))
        } else {
            Ok(())
        }
    }

    fn expect_word(&mut self, expected: &str) -> Result<(), KarmaDslError> {
        let actual = self.take_word(expected)?;
        if actual == expected {
            Ok(())
        } else {
            Err(self.unexpected(format!("expected {expected:?}, found {actual:?}")))
        }
    }

    fn take_word(&mut self, label: &str) -> Result<&'source str, KarmaDslError> {
        match self.next_token()?.kind {
            TokenKind::Word(word) => Ok(word),
            _ => Err(self.unexpected(format!("expected {label}"))),
        }
    }

    fn take_string(&mut self, label: &str) -> Result<String, KarmaDslError> {
        match self.next_token()?.kind {
            TokenKind::String(string) => Ok(string),
            _ => Err(self.unexpected(format!("expected {label} string"))),
        }
    }

    fn expect_symbol(&mut self, expected: char) -> Result<(), KarmaDslError> {
        if self.next_token()?.kind == TokenKind::Symbol(expected) {
            Ok(())
        } else {
            Err(self.unexpected(format!("expected symbol {expected:?}")))
        }
    }

    fn comma(&mut self) -> Result<(), KarmaDslError> {
        self.expect_symbol(',')
    }

    fn consume_symbol(&mut self, expected: char) -> bool {
        self.consume(TokenKind::Symbol(expected))
    }

    fn consume_word(&mut self, expected: &str) -> bool {
        self.consume(TokenKind::Word(expected))
    }

    fn consume(&mut self, expected: TokenKind<'_>) -> bool {
        if let Some(token) = self.tokens.as_slice().first()
            && token.kind == expected
        {
            self.last = token.position;
            self.tokens.next();
            true
        } else {
            false
        }
    }

    fn next_token(&mut self) -> Result<Token<'source>, KarmaDslError> {
        let token = self
            .tokens
            .next()
            .ok_or_else(|| self.error_here(DslErrorKind::UnexpectedEnd, "unexpected end of DSL"))?;
        self.last = token.position;
        Ok(token)
    }

    fn atom(&self, error: KarmaBoundaryError) -> KarmaDslError {
        self.error_here(DslErrorKind::InvalidAtom, error.to_string())
    }

    fn duplicate(&self, label: &str) -> KarmaDslError {
        self.error_here(
            DslErrorKind::DuplicateDeclaration,
            format!("duplicate {label}"),
        )
    }

    fn missing(&self, label: &str) -> KarmaDslError {
        self.error_here(
            DslErrorKind::MissingDeclaration,
            format!("missing required {label}"),
        )
    }

    fn unexpected(&self, message: impl Into<String>) -> KarmaDslError {
        self.error_here(DslErrorKind::UnexpectedToken, message)
    }

    fn error_here(&self, kind: DslErrorKind, message: impl Into<String>) -> KarmaDslError {
        self.last.error(kind, message)
    }

    fn insert_unique<V>(
        &self,
        values: &mut BTreeMap<LocalId, V>,
        key: LocalId,
        value: V,
    ) -> Result<(), KarmaDslError> {
        if values.insert(key, value).is_some() {
            Err(self.duplicate("map id"))
        } else {
            Ok(())
        }
    }
}
