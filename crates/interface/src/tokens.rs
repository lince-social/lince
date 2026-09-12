use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorScheme {
    #[default]
    Dark,
    Light,
}

impl ColorScheme {
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "Lince Dark",
            Self::Light => "Lince Light",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TokenValue {
    Color([u8; 4]),
    Number(f32),
}

impl TokenValue {
    pub fn color(self) -> Color {
        match self {
            Self::Color([r, g, b, a]) => Color::srgba_u8(r, g, b, a),
            Self::Number(_) => unreachable!(),
        }
    }

    pub fn number(self) -> f32 {
        match self {
            Self::Number(value) => value,
            Self::Color(_) => unreachable!(),
        }
    }

    pub fn display(self) -> String {
        match self {
            Self::Color([r, g, b, 255]) => format!("#{r:02X}{g:02X}{b:02X}"),
            Self::Color([r, g, b, a]) => format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
            Self::Number(value) => value.to_string(),
        }
    }
}

pub struct TokenDefinition {
    pub token: Token,
    pub name: &'static str,
    pub dark: TokenValue,
    pub light: TokenValue,
    pub range: Option<(f32, f32)>,
}

macro_rules! tokens {
    ($($id:ident, $name:literal, $dark:expr, $light:expr, $range:expr;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[repr(usize)]
        pub enum Token { $($id,)* }
        pub static TOKENS: &[TokenDefinition] = &[
            $(TokenDefinition { token: Token::$id, name: $name, dark: $dark, light: $light, range: $range },)*
        ];
    };
}

use TokenValue::{Color as Rgba, Number};
tokens! {
    Surface, "Panel background", Rgba([18,18,20,255]), Rgba([248,250,252,255]), None;
    Ink, "Interface text and icons", Rgba([248,250,252,255]), Rgba([18,18,20,255]), None;
    Accent, "Accent and focus", Rgba([99,102,241,255]), Rgba([55,48,163,255]), None;
    CanvasBackground, "Canvas background", Rgba([18,18,20,255]), Rgba([248,250,252,255]), None;
    CanvasGrid, "Canvas grid", Rgba([44,44,49,255]), Rgba([210,214,222,255]), None;
    CanvasPattern, "Pattern", Number(100.0), Number(100.0), Some((0.0,100.0));
    SandBackground, "Sand background", Rgba([18,18,20,255]), Rgba([248,250,252,255]), None;
    SandBorder, "Sand border", Rgba([99,102,241,255]), Rgba([55,48,163,255]), None;
    SandInk, "Sand text", Rgba([248,250,252,255]), Rgba([18,18,20,255]), None;
    Connections, "Edit mode connections", Rgba([77,217,230,255]), Rgba([0,105,120,255]), None;
    ConnectionFill, "Edit mode connection fill", Rgba([77,217,230,31]), Rgba([0,105,120,31]), None;
    Width, "Sand width", Number(248.0), Number(248.0), Some((48.0,100_000.0));
    Height, "Sand height", Number(184.0), Number(184.0), Some((48.0,100_000.0));
    Roundness, "Sand roundness", Number(0.0), Number(0.0), Some((0.0,1000.0));
    BorderWidth, "Sand border thickness", Number(0.0), Number(0.0), Some((0.0,32.0));
    Spacing, "Spacing", Number(8.0), Number(8.0), Some((0.0,32.0));
    Padding, "Padding", Number(8.0), Number(8.0), Some((0.0,32.0));
    FontSize, "Text size", Number(16.0), Number(16.0), Some((8.0,32.0));
    IconSize, "Icon size", Number(24.0), Number(24.0), Some((12.0,48.0));
    IconPadding, "Icon padding", Number(7.0), Number(7.0), Some((0.0,24.0));
    IconRoundness, "Icon roundness", Number(4.0), Number(4.0), Some((0.0,32.0));
    ControlBorder, "Control border thickness", Number(1.0), Number(1.0), Some((0.0,8.0));
    PanelWidth, "Panel width", Number(376.0), Number(376.0), Some((280.0,1200.0));
    CustomizationWidth, "Customization width", Number(720.0), Number(720.0), Some((360.0,1600.0));
    TooltipWidth, "Tooltip width", Number(280.0), Number(280.0), Some((120.0,800.0));
    TooltipRoundness, "Tooltip roundness", Number(4.0), Number(4.0), Some((0.0,32.0));
    GridSpacing, "Grid spacing", Number(32.0), Number(32.0), Some((8.0,128.0));
    GridThickness, "Grid thickness", Number(1.0), Number(1.0), Some((0.5,4.0));
}

impl Token {
    pub fn definition(self) -> &'static TokenDefinition {
        &TOKENS[self as usize]
    }

    pub fn default_value(self, scheme: ColorScheme) -> TokenValue {
        let definition = self.definition();
        match scheme {
            ColorScheme::Dark => definition.dark,
            ColorScheme::Light => definition.light,
        }
    }

    pub fn accepts(self, value: TokenValue) -> bool {
        match (self.definition().range, value) {
            (Some((min, max)), Number(value)) => value.is_finite() && (min..=max).contains(&value),
            (None, Rgba(rgba)) => !self.is_canvas() || rgba[3] == 255,
            _ => false,
        }
    }

    pub fn parse(self, text: &str) -> Option<TokenValue> {
        let value = if self.definition().range.is_some() {
            Number(text.trim().parse().ok()?)
        } else {
            let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
            if !matches!(text.len(), 3 | 6 | 8)
                || !text.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            let mut rgba = [255; 4];
            let count = if text.len() == 8 { 4 } else { 3 };
            for (i, channel) in rgba.iter_mut().enumerate().take(count) {
                *channel = if text.len() == 3 {
                    u8::from_str_radix(&text[i..i + 1], 16).ok()? * 17
                } else {
                    u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?
                };
            }
            Rgba(rgba)
        };
        self.accepts(value).then_some(value)
    }

    pub fn is_canvas(self) -> bool {
        matches!(
            self,
            Self::CanvasBackground | Self::CanvasGrid | Self::CanvasPattern
        )
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenOverrides(pub BTreeMap<Token, TokenValue>);

impl TokenOverrides {
    pub fn validate(&self) -> bool {
        self.0.iter().all(|(token, value)| token.accepts(*value))
    }

    pub fn set(&mut self, token: Token, value: TokenValue) -> bool {
        if !token.accepts(value) {
            return false;
        }
        self.0.insert(token, value);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SandStyleKind {
    Square,
    Text,
    EditableText,
    Record,
}

impl SandStyleKind {
    pub const ALL: [Self; 4] = [Self::Square, Self::Text, Self::EditableText, Self::Record];

    pub fn name(self) -> &'static str {
        match self {
            Self::Square => "Squares",
            Self::Text => "Plain text",
            Self::EditableText => "Editable text",
            Self::Record => "Records",
        }
    }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub scheme: ColorScheme,
    pub global: TokenOverrides,
    pub kinds: BTreeMap<SandStyleKind, TokenOverrides>,
}

impl ThemeSettings {
    pub fn validate(&self) -> bool {
        self.global.validate() && self.kinds.values().all(TokenOverrides::validate)
    }

    pub fn resolve(
        &self,
        token: Token,
        kind: Option<SandStyleKind>,
        overrides: &TokenOverrides,
    ) -> (TokenValue, &'static str) {
        if let Some(value) = overrides.0.get(&token) {
            return (*value, "This Sand");
        }
        if let Some(value) = kind
            .and_then(|kind| self.kinds.get(&kind))
            .and_then(|values| values.0.get(&token))
        {
            return (*value, "Sand type");
        }
        if let Some(value) = self.global.0.get(&token) {
            return (*value, "All Sands");
        }
        if token == Token::SandBackground
            && matches!(
                kind,
                Some(SandStyleKind::Text | SandStyleKind::EditableText)
            )
        {
            return (Rgba([0, 0, 0, 0]), "Default");
        }
        if kind == Some(SandStyleKind::Record) {
            match token {
                Token::Width => return (Number(232.0), "Default"),
                Token::Height => return (Number(176.0), "Default"),
                _ => {}
            }
        }
        (token.default_value(self.scheme), "Colorscheme")
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn every_compiled_default_has_a_unique_name_and_round_trips() {
        let mut names = std::collections::HashSet::new();
        for definition in TOKENS {
            assert!(names.insert(definition.name));
            assert!(std::ptr::eq(definition, definition.token.definition()));
            for value in [definition.dark, definition.light] {
                assert!(definition.token.accepts(value));
                assert_eq!(definition.token.parse(&value.display()), Some(value));
            }
        }
    }

    #[cfg_attr(test, test)]
    fn schemes_preserve_overrides_at_every_level_and_reset_reveals_inheritance() {
        let token = Token::SandBackground;
        let global = Rgba([10, 20, 30, 255]);
        let kind_value = Rgba([40, 50, 60, 255]);
        let individual = Rgba([70, 80, 90, 255]);
        let mut settings = ThemeSettings::default();
        let kind = Some(SandStyleKind::Square);
        let mut own = TokenOverrides::default();
        settings.global.set(token, global);
        settings
            .kinds
            .entry(kind.unwrap())
            .or_default()
            .set(token, kind_value);
        own.set(token, individual);
        settings.scheme = ColorScheme::Light;
        assert_eq!(
            settings.resolve(token, kind, &own),
            (individual, "This Sand")
        );
        own.0.clear();
        assert_eq!(
            settings.resolve(token, kind, &own),
            (kind_value, "Sand type")
        );
        settings.kinds.clear();
        assert_eq!(settings.resolve(token, kind, &own), (global, "All Sands"));
        settings.global.0.clear();
        assert_eq!(
            settings.resolve(token, kind, &own).0,
            token.default_value(ColorScheme::Light)
        );
    }

    #[cfg_attr(test, test)]
    fn invalid_colors_numbers_and_saved_types_are_rejected() {
        for text in ["#é1234", "#12", "#GGG", "#１２３", "NaN", "infinity"] {
            assert_eq!(Token::SandBackground.parse(text), None);
        }
        for text in ["NaN", "inf", "-1", "0", "100001"] {
            assert_eq!(Token::Width.parse(text), None);
        }
        for text in ["NaN", "inf", "-1", "100.01"] {
            assert_eq!(Token::CanvasPattern.parse(text), None);
        }
        for amount in [0.0, 1.0, 2.0, 100.0] {
            assert!(Token::CanvasPattern.accepts(Number(amount)));
        }
        assert_eq!(
            Token::SandBackground.parse("#AbC"),
            Some(Rgba([170, 187, 204, 255]))
        );
        assert_eq!(
            Token::SandBackground.parse("#12345600"),
            Some(Rgba([18, 52, 86, 0]))
        );
        let mut values = TokenOverrides::default();
        assert!(!values.set(Token::Width, Rgba([0; 4])));
        values.0.insert(Token::Width, Number(f32::NAN));
        assert!(!values.validate());
    }

    crate::laboratory_cases! {
        every_compiled_default_has_a_unique_name_and_round_trips,
        schemes_preserve_overrides_at_every_level_and_reset_reveals_inheritance,
        invalid_colors_numbers_and_saved_types_are_rejected,
    }
}
