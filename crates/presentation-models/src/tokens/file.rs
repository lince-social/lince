//! Watches a theme file for new content or a new read error.

use std::{fs::read_to_string, path::PathBuf};

use super::{ThemeSettings, document::ThemeDocument};

/// Tracks the last revision read from a theme file.
#[derive(Debug)]
pub struct ThemeFile {
    path: PathBuf,
    last: Option<Result<String, String>>,
}

impl ThemeFile {
    /// Watches a file starting with its first poll.
    pub fn new(path: PathBuf) -> Self {
        Self { path, last: None }
    }

    /// Reads and parses the latest revision when it differs from the previous poll.
    ///
    /// # Errors
    ///
    /// Returns the read or parse error for a changed revision once.
    pub fn poll(&mut self) -> Result<Option<ThemeSettings>, String> {
        let source = read_to_string(&self.path).map_err(|error| error.to_string());
        if self.last.as_ref() == Some(&source) {
            return Ok(None);
        }
        let source = self.last.insert(source).as_ref().map_err(Clone::clone)?;
        ThemeDocument::parse(source).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{Token, TokenOverrides, TokenValue};

    #[test]
    fn theme_reload_recovers_after_invalid_edits_without_replacing_valid_settings() {
        let path =
            std::env::temp_dir().join(format!("lince-theme-reload-{}.json", std::process::id()));
        let mut theme = ThemeSettings::default();
        std::fs::write(&path, ThemeDocument::export(&theme).unwrap()).unwrap();
        let mut file = ThemeFile::new(path.clone());
        let mut applied = file.poll().unwrap().unwrap();
        assert!(file.poll().unwrap().is_none());
        std::fs::write(&path, "{partial").unwrap();
        assert!(file.poll().is_err());
        assert!(file.poll().unwrap().is_none());
        assert_eq!(
            applied
                .resolve(Token::Padding, None, &TokenOverrides::default())
                .0,
            theme
                .resolve(Token::Padding, None, &TokenOverrides::default())
                .0
        );
        theme.global.set(Token::Padding, TokenValue::Number(21.0));
        std::fs::write(&path, ThemeDocument::export(&theme).unwrap()).unwrap();
        applied = file.poll().unwrap().unwrap();
        assert_eq!(applied.global.0[&Token::Padding], TokenValue::Number(21.0));
        std::fs::remove_file(&path).unwrap();
        assert!(file.poll().is_err());
    }
}
