use super::{SandStyleKind, TOKENS, ThemeSettings, Token, TokenOverrides, TokenValue};

pub fn export(
    settings: &ThemeSettings,
    kind: Option<SandStyleKind>,
    overrides: &TokenOverrides,
) -> Result<String, String> {
    if !settings.validate() || !overrides.validate() {
        return Err("The theme contains invalid token values".into());
    }
    let mut css = String::from(":root {\n");
    for definition in TOKENS {
        let token = definition.token;
        let value = settings.resolve(token, kind, overrides).0;
        let name = serde_json::to_value(token).map_err(|error| error.to_string())?;
        let name = name.as_str().ok_or("Token name must be a string")?;
        match value {
            TokenValue::Color(_) => {
                css.push_str(&format!("  --{name}: {};\n", value.display()));
            }
            TokenValue::Number(value) => {
                let unit = if matches!(
                    token,
                    Token::CanvasPattern | Token::ControlsCornerTransparency
                ) {
                    "%"
                } else {
                    "px"
                };
                css.push_str(&format!(
                    "  --{name}: {value}{unit};\n  --{name}-raw: {value};\n"
                ));
            }
        }
    }
    css.push_str("}\n");
    Ok(css)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{ColorScheme, document::ThemeDocument};

    #[test]
    fn css_uses_the_same_resolved_values_for_every_scheme_and_kind() {
        for scheme in ColorScheme::ALL {
            let theme = ThemeSettings {
                scheme,
                ..Default::default()
            };
            let restored = ThemeDocument::parse(&ThemeDocument::export(&theme).unwrap()).unwrap();
            for kind in std::iter::once(None).chain(SandStyleKind::ALL.into_iter().map(Some)) {
                let mut overrides = TokenOverrides::default();
                overrides.set(Token::Accent, TokenValue::Color([1, 2, 3, 128]));
                let css = export(&restored, kind, &overrides).unwrap();
                for definition in TOKENS {
                    let name = serde_json::to_value(definition.token).unwrap();
                    let name = name.as_str().unwrap();
                    let value = theme.resolve(definition.token, kind, &overrides).0;
                    let expected = match value {
                        TokenValue::Color(_) => format!("--{name}: {};", value.display()),
                        TokenValue::Number(value) => format!("--{name}-raw: {value};"),
                    };
                    assert!(css.contains(&expected), "{scheme:?} {kind:?} {expected}");
                }
                assert!(css.contains("--Accent: #01020380;"));
                assert!(css.contains(&format!(
                        "--CanvasPattern: {}%;",
                        theme
                            .resolve(Token::CanvasPattern, kind, &overrides)
                            .0
                            .number()
                    )));
                assert!(css.contains(&format!(
                    "--FontSize: {}px;",
                    theme.resolve(Token::FontSize, kind, &overrides).0.number()
                )));
            }
        }
    }

    #[test]
    fn invalid_values_are_not_exported_to_css() {
        let mut theme = ThemeSettings::default();
        theme
            .global
            .0
            .insert(Token::FontSize, TokenValue::Number(f32::NAN));
        assert!(export(&theme, None, &TokenOverrides::default()).is_err());
    }
}
