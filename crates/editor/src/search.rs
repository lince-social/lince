use crate::Result;
use regex::{Regex, RegexBuilder};
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            whole_word: false,
        }
    }
}

pub struct Pattern {
    regex: Regex,
    length: usize,
}

impl Pattern {
    pub fn first(&self, text: &str) -> Option<Range<usize>> {
        self.regex.find(text).map(|found| found.range())
    }

    pub fn new(needle: &str, options: Options) -> Result<Self> {
        if needle.is_empty() || needle.len() > 4096 {
            return Err("Enter between 1 and 4096 bytes of search text".into());
        }
        let escaped = regex::escape(needle);
        let source = if options.whole_word {
            format!(r"\b(?:{escaped})\b")
        } else {
            escaped
        };
        let regex = RegexBuilder::new(&source)
            .case_insensitive(!options.case_sensitive)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            regex,
            length: needle.chars().count(),
        })
    }

    pub fn matches_range(&self, rope: &Rope, range: Range<usize>) -> bool {
        if range.len() != self.length || range.end > rope.len_chars() {
            return false;
        }
        let left = range.start.saturating_sub(1);
        let right = (range.end + 1).min(rope.len_chars());
        let text = rope.slice(left..right).to_string();
        let start = rope.char_to_byte(range.start) - rope.char_to_byte(left);
        let end = rope.char_to_byte(range.end) - rope.char_to_byte(left);
        self.regex
            .find_at(&text, start)
            .is_some_and(|found| found.start() == start && found.end() == end)
    }

    pub fn scan(
        &self,
        rope: &Rope,
        range: Range<usize>,
        cancel: &AtomicBool,
        mut visit: impl FnMut(Range<usize>) -> bool,
    ) -> Result<()> {
        let mut start = range.start.min(rope.len_chars());
        let end = range.end.min(rope.len_chars());
        let mut consumed = start;
        while start < end {
            if cancel.load(Ordering::Relaxed) {
                return Err("Search cancelled".into());
            }
            let core_end = (start + 16_384).min(end);
            let left = start.saturating_sub(1);
            let right = (core_end + self.length + 1).min(rope.len_chars());
            let text = rope.slice(left..right).to_string();
            let offset = rope.char_to_byte(left);
            let core_byte = rope.char_to_byte(core_end);
            let mut begin = rope.char_to_byte(consumed) - offset;
            while let Some(found) = self.regex.find_at(&text, begin) {
                begin = found.end();
                let a = offset + found.start();
                let b = offset + found.end();
                if a >= core_byte {
                    break;
                }
                let a = rope.byte_to_char(a);
                let b = rope.byte_to_char(b);
                if b <= end && !visit(a..b) {
                    return Ok(());
                }
                consumed = b;
            }
            start = core_end;
            consumed = consumed.max(start);
        }
        Ok(())
    }

    pub fn find(
        &self,
        rope: &Rope,
        start: usize,
        backwards: bool,
        cancel: &AtomicBool,
    ) -> Result<Option<Range<usize>>> {
        let start = start.min(rope.len_chars());
        let ranges = if backwards {
            [0..start, start..rope.len_chars()]
        } else {
            [start..rope.len_chars(), 0..start]
        };
        for range in ranges {
            let mut found = None;
            self.scan(rope, range, cancel, |next| {
                found = Some(next);
                backwards
            })?;
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }

    pub fn all(&self, rope: &Rope, cancel: &AtomicBool) -> Result<Vec<Range<usize>>> {
        let mut found = Vec::new();
        self.scan(rope, 0..rope.len_chars(), cancel, |range| {
            found.push(range);
            found.len() <= 100_000
        })?;
        if found.len() > 100_000 {
            return Err("More than 100,000 matches; narrow the search before replacing".into());
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests;
