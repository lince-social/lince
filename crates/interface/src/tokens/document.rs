use super::{ColorScheme, SandStyleKind, TOKENS, ThemeSettings, Token, TokenOverrides, TokenValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeDocument {
    name: String,
    scheme: ColorScheme,
    tokens: BTreeMap<Token, Entry>,
    kinds: BTreeMap<SandStyleKind, BTreeMap<Token, Entry>>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Entry {
    Color(String),
    Number(f32),
}

impl From<TokenValue> for Entry {
    fn from(value: TokenValue) -> Self {
        match value {
            TokenValue::Color(_) => Self::Color(value.display()),
            TokenValue::Number(number) => Self::Number(number),
        }
    }
}

impl ThemeDocument {
    pub fn export(settings: &ThemeSettings) -> Result<String, String> {
        if !settings.validate() {
            return Err("The theme contains invalid token values".into());
        }
        let empty = TokenOverrides::default();
        let tokens = TOKENS
            .iter()
            .map(|definition| {
                let token = definition.token;
                (token, settings.resolve(token, None, &empty).0.into())
            })
            .collect();
        let kinds = SandStyleKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let overrides: BTreeMap<_, _> = TOKENS
                    .iter()
                    .filter_map(|definition| {
                        let token = definition.token;
                        let value = settings.resolve(token, Some(kind), &empty).0;
                        (value != settings.resolve(token, None, &empty).0)
                            .then_some((token, value.into()))
                    })
                    .collect();
                (!overrides.is_empty()).then_some((kind, overrides))
            })
            .collect();
        let document = Self {
            name: settings.scheme.name().into(),
            scheme: settings.scheme,
            tokens,
            kinds,
        };
        serde_json::to_string_pretty(&document)
            .map(|json| format!("{json}\n"))
            .map_err(|error| format!("Could not export theme: {error}"))
    }

    pub fn parse(source: &str) -> Result<ThemeSettings, String> {
        let document: Self = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if document.name.trim().is_empty() || document.name.chars().count() > 160 {
            return Err("Use a theme name of 1–160 characters".into());
        }
        if document.tokens.len() != TOKENS.len() {
            return Err("A theme must include every token".into());
        }
        fn values(entries: BTreeMap<Token, Entry>) -> Result<TokenOverrides, String> {
            let mut values = TokenOverrides::default();
            for (token, entry) in entries {
                let value = match entry {
                    Entry::Color(color) if token.definition().range.is_none() => {
                        token.parse(&color)
                    }
                    Entry::Number(number) => Some(TokenValue::Number(number)),
                    Entry::Color(_) => None,
                };
                if !value.is_some_and(|value| values.set(token, value)) {
                    return Err(format!("Invalid value for {}", token.definition().name));
                }
            }
            Ok(values)
        }
        Ok(ThemeSettings {
            scheme: document.scheme,
            global: values(document.tokens)?,
            kinds: document
                .kinds
                .into_iter()
                .map(|(kind, entries)| values(entries).map(|values| (kind, values)))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_round_trips_every_effective_token_and_kind_for_all_themes() {
        for scheme in ColorScheme::ALL {
            let mut settings = ThemeSettings {
                scheme,
                ..Default::default()
            };
            for customized in [false, true] {
                if customized {
                    settings
                        .global
                        .set(Token::Padding, TokenValue::Number(13.5));
                    settings.global.set(Token::Width, TokenValue::Number(340.0));
                    settings
                        .global
                        .set(Token::Accent, TokenValue::Color([123, 34, 56, 255]));
                    settings
                        .kinds
                        .entry(SandStyleKind::Record)
                        .or_default()
                        .set(Token::Roundness, TokenValue::Number(19.0));
                    settings
                        .kinds
                        .entry(SandStyleKind::Text)
                        .or_default()
                        .set(Token::SandInk, TokenValue::Color([12, 34, 56, 128]));
                }
                let json = ThemeDocument::export(&settings).unwrap();
                let restored = ThemeDocument::parse(&json).unwrap();
                assert!(restored.validate());
                assert_eq!(restored.scheme, scheme);
                assert_eq!(restored.global.0.len(), TOKENS.len());
                for kind in std::iter::once(None).chain(SandStyleKind::ALL.into_iter().map(Some)) {
                    for definition in TOKENS {
                        assert_eq!(
                            settings
                                .resolve(definition.token, kind, &TokenOverrides::default())
                                .0,
                            restored
                                .resolve(definition.token, kind, &TokenOverrides::default())
                                .0,
                            "{scheme:?} {kind:?} {:?}",
                            definition.token
                        );
                    }
                }
                assert_eq!(ThemeDocument::export(&restored).unwrap(), json);
                let document: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(document["name"], scheme.name());
                assert!(document["tokens"]["Roundness"].is_number());
                assert!(
                    document["tokens"]["Accent"]
                        .as_str()
                        .unwrap()
                        .starts_with('#')
                );
            }
        }
    }

    #[test]
    fn bundled_themes_have_distinct_geometry_and_readable_text() {
        fn luminance(value: TokenValue) -> f32 {
            let TokenValue::Color([r, g, b, _]) = value else {
                panic!("expected color")
            };
            [r, g, b]
                .into_iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(channel, weight)| {
                    let channel = f32::from(channel) / 255.0;
                    weight
                        * if channel <= 0.04045 {
                            channel / 12.92
                        } else {
                            ((channel + 0.055) / 1.055).powf(2.4)
                        }
                })
                .sum()
        }
        for scheme in [ColorScheme::ComfyPink, ColorScheme::Moss] {
            for token in [
                Token::Roundness,
                Token::BorderWidth,
                Token::Width,
                Token::Height,
                Token::Padding,
                Token::ControlRoundness,
            ] {
                assert_ne!(
                    token.default_value(scheme),
                    token.default_value(ColorScheme::Dark)
                );
            }
            for (ink, background) in [
                (Token::Ink, Token::Surface),
                (Token::SandInk, Token::SandBackground),
                (Token::Ink, Token::CanvasBackground),
                (Token::Accent, Token::Surface),
            ] {
                let a = luminance(ink.default_value(scheme));
                let b = luminance(background.default_value(scheme));
                assert!(
                    (a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5,
                    "{scheme:?} {ink:?} on {background:?}"
                );
            }
        }
        assert!(
            Token::Roundness
                .default_value(ColorScheme::ComfyPink)
                .number()
                > Token::Roundness.default_value(ColorScheme::Moss).number()
        );
    }

    #[test]
    fn incomplete_mistyped_and_out_of_range_documents_are_rejected() {
        let source = ThemeDocument::export(&ThemeSettings::default()).unwrap();
        let base: serde_json::Value = serde_json::from_str(&source).unwrap();
        for (token, value) in [
            ("Width", serde_json::json!(-1)),
            ("Roundness", serde_json::json!(1001)),
            ("Padding", serde_json::json!("12")),
            ("Ink", serde_json::json!(12)),
            ("Ink", serde_json::json!("#bad-color")),
            ("CanvasBackground", serde_json::json!("#12345600")),
        ] {
            let mut document = base.clone();
            document["tokens"][token] = value;
            assert!(ThemeDocument::parse(&document.to_string()).is_err());
        }
        let mut document = base.clone();
        document["tokens"].as_object_mut().unwrap().remove("Width");
        assert!(ThemeDocument::parse(&document.to_string()).is_err());
        let mut document = base.clone();
        document["tokens"]["Unknown"] = 1.into();
        assert!(ThemeDocument::parse(&document.to_string()).is_err());
        let mut document = base;
        document["kinds"]["Record"]["Width"] = (-1).into();
        assert!(ThemeDocument::parse(&document.to_string()).is_err());
        let mut settings = ThemeSettings::default();
        settings
            .global
            .0
            .insert(Token::Width, TokenValue::Number(f32::NAN));
        assert!(ThemeDocument::export(&settings).is_err());
    }
}
