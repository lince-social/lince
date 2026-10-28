#![forbid(unsafe_code)]

use std::{fmt, sync::Arc};
use tnil::{
    prelude::{IntoScript, IntoScriptFlags, ScriptRepr, TryAsSpecific, Word},
    script::{buf::CharacterBuf, repr::IthkuilBasicEncoding},
    word::NormalReferential,
};

pub static FONT_BYTES: &[u8] = include_bytes!("../assets/IthkuilBasic.ttf");
pub const FONT_FAMILY: &str = "IthkuilBasic";
pub const FONT_LICENSE: &str = include_str!("../assets/OFL.txt");
pub const FONT_CREDITS: &str = include_str!("../assets/CREDITS.txt");
pub const FONT_NOTICE: &str = concat!(
    include_str!("../assets/CREDITS.txt"),
    "\n",
    include_str!("../assets/OFL.txt"),
);
pub const FONT_SHA256: &str = "a87b9193981f000ae48e5c5b52d3b67c8decf9a4b7f5b127f2fd01ff97ed5897";
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_WORD_BYTES: usize = 512;
pub const MAX_WORDS: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    romanization: Arc<str>,
    encoding: Arc<str>,
    characters: usize,
}

impl Script {
    pub fn romanization(&self) -> &str {
        &self.romanization
    }

    pub fn encoding(&self) -> &str {
        &self.encoding
    }

    pub fn character_count(&self) -> usize {
        self.characters
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooLong,
    InvalidWord(usize),
    UnsupportedWord(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Enter New Ithkuil text to render"),
            Self::TooLong => formatter.write_str("The Ithkuil text exceeds the rendering limit"),
            Self::InvalidWord(index) => {
                write!(formatter, "Invalid Ithkuil word at position {}", index + 1)
            }
            Self::UnsupportedWord(index) => write!(
                formatter,
                "The script converter does not support the word type at position {}",
                index + 1
            ),
        }
    }
}

impl std::error::Error for Error {}

pub fn render(romanization: &str) -> Result<Script, Error> {
    if romanization.len() > MAX_INPUT_BYTES {
        return Err(Error::TooLong);
    }
    if romanization.trim().is_empty() {
        return Err(Error::Empty);
    }
    let mut encoding = String::new();
    let mut characters = 0;
    for (index, source) in romanization.split_whitespace().enumerate() {
        if index >= MAX_WORDS || source.len() > MAX_WORD_BYTES {
            return Err(Error::TooLong);
        }
        let word: Word = source.parse().map_err(|_| Error::InvalidWord(index))?;
        let flags = IntoScriptFlags::NONE;
        let buffer: CharacterBuf = match word {
            Word::Formative(value) => value.into_script(flags),
            Word::Referential(value) => {
                let value: NormalReferential = value
                    .try_as_specific()
                    .ok_or(Error::UnsupportedWord(index))?;
                value.into_script(flags)
            }
            Word::Bias(value) => value.bias.into_script(flags),
            Word::Numeric(value) => value.form.into_script(flags),
            _ => return Err(Error::UnsupportedWord(index)),
        };
        if index > 0 {
            encoding.push(' ');
        }
        characters += buffer.vec.len();
        encoding.push_str(&IthkuilBasicEncoding::from_chars(&buffer.vec).0);
    }
    Ok(Script {
        romanization: Arc::from(romanization),
        encoding: Arc::from(encoding),
        characters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn grammar_drives_script_instead_of_substituting_latin_letters() {
        let script = render("ırburučpaızya").unwrap();
        assert_eq!(script.encoding(), r#"\^p_xr_bč'_p_ä|^t^a_aò"#);
        assert_ne!(script.encoding(), script.romanization());
        assert!(script.character_count() >= 4);
        assert_eq!(render("lo").unwrap().encoding(), "|_kl^ä");
    }

    #[test]
    fn bundled_font_shapes_every_translated_label_without_missing_glyphs() {
        let face = rustybuzz::Face::from_slice(FONT_BYTES, 0).unwrap();
        for source in ["adnila", "ardnila", "afçäla", "amäla", "aţčäla", "aňala"] {
            let script = render(source).unwrap();
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(script.encoding());
            buffer.guess_segment_properties();
            let shaped = rustybuzz::shape(&face, &[], buffer);
            assert!(!shaped.glyph_infos().is_empty());
            assert!(
                shaped.glyph_infos().iter().all(|glyph| glyph.glyph_id != 0),
                "{source}"
            );
            let nominal = script
                .encoding()
                .chars()
                .map(|character| u32::from(face.glyph_index(character).unwrap().0));
            if script.encoding().contains(['^', '_', '\'']) {
                assert!(
                    shaped
                        .glyph_infos()
                        .iter()
                        .map(|glyph| glyph.glyph_id)
                        .ne(nominal),
                    "Native glyph composition was not applied to {source}"
                );
            }
            assert!(
                shaped
                    .glyph_positions()
                    .iter()
                    .any(|position| position.x_advance > 0)
            );
        }
    }

    #[test]
    fn input_limits_and_unsupported_grammar_fail_without_guessing() {
        assert_eq!(render("  ").unwrap_err(), Error::Empty);
        assert_eq!(
            render(&"a".repeat(MAX_INPUT_BYTES + 1)).unwrap_err(),
            Error::TooLong
        );
        assert_eq!(
            render(&"a".repeat(MAX_WORD_BYTES + 1)).unwrap_err(),
            Error::TooLong
        );
        assert_eq!(render("adnila ???").unwrap_err(), Error::InvalidWord(1));
        assert_eq!(render("er").unwrap_err(), Error::UnsupportedWord(0));
    }

    #[test]
    fn whitespace_separates_words_and_script_clones_share_storage() {
        let script = render("adnila\n\tamäla").unwrap();
        assert_eq!(
            script.encoding(),
            format!(
                "{} {}",
                render("adnila").unwrap().encoding(),
                render("amäla").unwrap().encoding()
            )
        );
        let cloned = script.clone();
        assert!(std::ptr::eq(script.encoding(), cloned.encoding()));
        assert!(std::ptr::eq(script.romanization(), cloned.romanization()));
    }

    #[test]
    fn font_is_pinned_and_contains_its_original_open_font_license() {
        let checksum: String = Sha256::digest(FONT_BYTES)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(checksum, FONT_SHA256);
        assert!(FONT_LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(FONT_CREDITS.contains("Shankar Sivarajan"));
        assert!(FONT_CREDITS.contains("Kevin McCurdy"));
        assert!(FONT_CREDITS.contains("Zachary Sakowitz"));
        let face = rustybuzz::Face::from_slice(FONT_BYTES, 0).unwrap();
        assert!(face.names().into_iter().any(|name| {
            name.name_id == 13
                && name
                    .to_string()
                    .is_some_and(|value| value.contains("SIL OPEN FONT LICENSE Version 1.1"))
        }));
    }
}
