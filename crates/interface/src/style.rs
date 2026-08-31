use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{Display, Formatter},
    path::{Component, Path},
};

pub const STYLE_CONTRACT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleError {
    detail: String,
}

impl StyleError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl Display for StyleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for StyleError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StyleValueKind {
    Color,
    LengthPx,
    OffsetPx,
    Scalar,
    Integer,
    FontFamily,
    FontWeight,
    LineStyle,
    DurationMs,
    NumericFigures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    Solid,
    Dashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NumericFigures {
    Tabular,
    Proportional,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum StyleValue {
    Color(String),
    LengthPx(f32),
    OffsetPx(f32),
    Scalar(f32),
    Integer(i32),
    FontFamily(Vec<String>),
    FontWeight(u16),
    LineStyle(LineStyle),
    DurationMs(u32),
    NumericFigures(NumericFigures),
}

impl StyleValue {
    pub fn kind(&self) -> StyleValueKind {
        match self {
            Self::Color(_) => StyleValueKind::Color,
            Self::LengthPx(_) => StyleValueKind::LengthPx,
            Self::OffsetPx(_) => StyleValueKind::OffsetPx,
            Self::Scalar(_) => StyleValueKind::Scalar,
            Self::Integer(_) => StyleValueKind::Integer,
            Self::FontFamily(_) => StyleValueKind::FontFamily,
            Self::FontWeight(_) => StyleValueKind::FontWeight,
            Self::LineStyle(_) => StyleValueKind::LineStyle,
            Self::DurationMs(_) => StyleValueKind::DurationMs,
            Self::NumericFigures(_) => StyleValueKind::NumericFigures,
        }
    }

    fn validate(&self) -> Result<(), StyleError> {
        match self {
            Self::Color(value) if parse_color(value).is_none() => {
                Err(StyleError::new(format!("invalid color {value}")))
            }
            Self::LengthPx(value) if !value.is_finite() || *value < 0.0 || *value > 4096.0 => {
                Err(StyleError::new("length must be between 0 and 4096 px"))
            }
            Self::OffsetPx(value) if !value.is_finite() || value.abs() > 4096.0 => {
                Err(StyleError::new("offset must be between -4096 and 4096 px"))
            }
            Self::Scalar(value) if !value.is_finite() || *value < 0.0 || *value > 100.0 => {
                Err(StyleError::new("scalar must be between 0 and 100"))
            }
            Self::FontFamily(values)
                if values.is_empty()
                    || values.len() > 16
                    || values.iter().any(|value| !valid_font_family(value)) =>
            {
                Err(StyleError::new("font family list is invalid"))
            }
            Self::FontWeight(value) if !(1..=1000).contains(value) => {
                Err(StyleError::new("font weight must be between 1 and 1000"))
            }
            _ => Ok(()),
        }
    }

    fn to_css(&self) -> String {
        match self {
            Self::Color(value) => value.clone(),
            Self::LengthPx(value) => format_number(*value, "px"),
            Self::OffsetPx(value) => format_number(*value, "px"),
            Self::Scalar(value) => format_number(*value, ""),
            Self::Integer(value) => value.to_string(),
            Self::FontFamily(values) => values
                .iter()
                .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", "),
            Self::FontWeight(value) => value.to_string(),
            Self::LineStyle(LineStyle::Solid) => "solid".into(),
            Self::LineStyle(LineStyle::Dashed) => "dashed".into(),
            Self::DurationMs(value) => format!("{value}ms"),
            Self::NumericFigures(NumericFigures::Tabular) => "tabular-nums".into(),
            Self::NumericFigures(NumericFigures::Proportional) => "proportional-nums".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleTokenSpec {
    pub name: &'static str,
    pub kind: StyleValueKind,
    pub family: &'static str,
}

pub const STYLE_TOKEN_SPECS: &[StyleTokenSpec] = &[
    token(
        "--lynx-palette-purple-cobalt",
        StyleValueKind::Color,
        "palette",
    ),
    token(
        "--lynx-palette-purple-nocturnal",
        StyleValueKind::Color,
        "palette",
    ),
    token("--lynx-palette-deep-lead", StyleValueKind::Color, "palette"),
    token("--lynx-palette-gray", StyleValueKind::Color, "palette"),
    token("--lynx-palette-ice-white", StyleValueKind::Color, "palette"),
    token("--lynx-surface-canvas", StyleValueKind::Color, "surface"),
    token("--lynx-surface-primary", StyleValueKind::Color, "surface"),
    token("--lynx-surface-secondary", StyleValueKind::Color, "surface"),
    token("--lynx-surface-raised", StyleValueKind::Color, "surface"),
    token("--lynx-surface-input", StyleValueKind::Color, "surface"),
    token("--lynx-surface-hover", StyleValueKind::Color, "surface"),
    token("--lynx-surface-active", StyleValueKind::Color, "surface"),
    token("--lynx-ink-primary", StyleValueKind::Color, "ink"),
    token("--lynx-ink-secondary", StyleValueKind::Color, "ink"),
    token("--lynx-ink-inverse", StyleValueKind::Color, "ink"),
    token("--lynx-border", StyleValueKind::Color, "border"),
    token("--lynx-accent", StyleValueKind::Color, "intent"),
    token("--lynx-accent-supporting", StyleValueKind::Color, "intent"),
    token("--lynx-accent-ink", StyleValueKind::Color, "intent"),
    token("--lynx-focus", StyleValueKind::Color, "state"),
    token("--lynx-selection", StyleValueKind::Color, "state"),
    token("--lynx-need", StyleValueKind::Color, "intent"),
    token("--lynx-contribution", StyleValueKind::Color, "intent"),
    token("--lynx-peace", StyleValueKind::Color, "intent"),
    token("--lynx-info", StyleValueKind::Color, "intent"),
    token("--lynx-success", StyleValueKind::Color, "intent"),
    token("--lynx-warning", StyleValueKind::Color, "intent"),
    token("--lynx-danger", StyleValueKind::Color, "intent"),
    token("--lynx-tooltip-surface", StyleValueKind::Color, "surface"),
    token("--lynx-tooltip-ink", StyleValueKind::Color, "ink"),
    token("--lynx-scrim", StyleValueKind::Color, "opacity"),
    token("--lynx-shadow-color", StyleValueKind::Color, "elevation"),
    token(
        "--lynx-shadow-light-color",
        StyleValueKind::Color,
        "elevation",
    ),
    token("--lynx-state-hover-surface", StyleValueKind::Color, "state"),
    token(
        "--lynx-state-active-surface",
        StyleValueKind::Color,
        "state",
    ),
    token(
        "--lynx-state-selected-surface",
        StyleValueKind::Color,
        "state",
    ),
    token(
        "--lynx-state-selected-border",
        StyleValueKind::Color,
        "state",
    ),
    token("--lynx-state-read-only-ink", StyleValueKind::Color, "state"),
    token(
        "--lynx-state-invalid-border",
        StyleValueKind::Color,
        "state",
    ),
    token("--lynx-state-loading-ink", StyleValueKind::Color, "state"),
    token("--lynx-state-empty-ink", StyleValueKind::Color, "state"),
    token("--lynx-state-focus-border", StyleValueKind::Color, "state"),
    token("--lynx-space-half", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-1", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-2", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-3", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-4", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-5", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-6", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-space-7", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-gap-content", StyleValueKind::LengthPx, "spacing"),
    token("--lynx-padding-record", StyleValueKind::LengthPx, "spacing"),
    token(
        "--lynx-padding-control-x",
        StyleValueKind::LengthPx,
        "spacing",
    ),
    token(
        "--lynx-padding-control-y",
        StyleValueKind::LengthPx,
        "spacing",
    ),
    token("--lynx-control-height", StyleValueKind::LengthPx, "size"),
    token("--lynx-icon-size", StyleValueKind::LengthPx, "icon"),
    token("--lynx-border-hairline", StyleValueKind::LengthPx, "border"),
    token("--lynx-border-focus", StyleValueKind::LengthPx, "border"),
    token("--lynx-radius-control", StyleValueKind::LengthPx, "radius"),
    token("--lynx-radius-panel", StyleValueKind::LengthPx, "radius"),
    token("--lynx-font-body", StyleValueKind::FontFamily, "typography"),
    token(
        "--lynx-font-title",
        StyleValueKind::FontFamily,
        "typography",
    ),
    token("--lynx-font-mono", StyleValueKind::FontFamily, "typography"),
    token(
        "--lynx-font-weight-normal",
        StyleValueKind::FontWeight,
        "typography",
    ),
    token(
        "--lynx-font-weight-medium",
        StyleValueKind::FontWeight,
        "typography",
    ),
    token(
        "--lynx-font-weight-strong",
        StyleValueKind::FontWeight,
        "typography",
    ),
    token(
        "--lynx-text-size-body",
        StyleValueKind::LengthPx,
        "typography",
    ),
    token(
        "--lynx-text-size-compact",
        StyleValueKind::LengthPx,
        "typography",
    ),
    token(
        "--lynx-text-size-metadata",
        StyleValueKind::LengthPx,
        "typography",
    ),
    token(
        "--lynx-line-height-body",
        StyleValueKind::Scalar,
        "typography",
    ),
    token(
        "--lynx-numeric-figures",
        StyleValueKind::NumericFigures,
        "typography",
    ),
    token("--lynx-shadow-x", StyleValueKind::OffsetPx, "elevation"),
    token("--lynx-shadow-y", StyleValueKind::OffsetPx, "elevation"),
    token("--lynx-shadow-blur", StyleValueKind::LengthPx, "elevation"),
    token(
        "--lynx-shadow-light-x",
        StyleValueKind::OffsetPx,
        "elevation",
    ),
    token(
        "--lynx-shadow-light-y",
        StyleValueKind::OffsetPx,
        "elevation",
    ),
    token(
        "--lynx-shadow-light-blur",
        StyleValueKind::LengthPx,
        "elevation",
    ),
    token("--lynx-opacity-disabled", StyleValueKind::Scalar, "opacity"),
    token(
        "--lynx-opacity-ephemeral",
        StyleValueKind::Scalar,
        "opacity",
    ),
    token("--lynx-density-scale", StyleValueKind::Scalar, "density"),
    token("--lynx-stack-base", StyleValueKind::Integer, "stacking"),
    token("--lynx-stack-floating", StyleValueKind::Integer, "stacking"),
    token("--lynx-stack-focus", StyleValueKind::Integer, "stacking"),
    token("--lynx-stack-menu", StyleValueKind::Integer, "stacking"),
    token("--lynx-stack-dialog", StyleValueKind::Integer, "stacking"),
    token("--lynx-stack-security", StyleValueKind::Integer, "stacking"),
    token("--lynx-motion-state", StyleValueKind::DurationMs, "motion"),
    token("--lynx-motion-direct", StyleValueKind::DurationMs, "motion"),
    token(
        "--lynx-motion-reduced",
        StyleValueKind::DurationMs,
        "motion",
    ),
    token("--lynx-truth-settled", StyleValueKind::LineStyle, "truth"),
    token("--lynx-truth-declared", StyleValueKind::LineStyle, "truth"),
];

const fn token(name: &'static str, kind: StyleValueKind, family: &'static str) -> StyleTokenSpec {
    StyleTokenSpec { name, kind, family }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleLayer {
    pub values: BTreeMap<String, StyleValue>,
}

impl StyleLayer {
    pub fn validate_standard(&self) -> Result<(), StyleError> {
        validate_layer(self, &BTreeMap::new())
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StyleScope {
    DefaultContract,
    Projection,
    Definition,
    ActiveTheme,
    Workspace,
    Group,
    Instance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedStyleLayer {
    pub scope: StyleScope,
    pub label: String,
    pub layer: StyleLayer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeAssetKind {
    Raster,
    Svg,
    Font,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeAsset {
    pub path: String,
    pub sha256: String,
    pub kind: ThemeAssetKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeManifest {
    pub contract_version: u32,
    pub uid: String,
    pub display_name: String,
    pub default_mode: String,
    pub extensions: BTreeMap<String, StyleValueKind>,
    pub common: StyleLayer,
    pub modes: BTreeMap<String, StyleLayer>,
    pub assets: Vec<ThemeAsset>,
}

impl ThemeManifest {
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.contract_version != STYLE_CONTRACT_VERSION {
            return Err(StyleError::new(format!(
                "unsupported style contract version {}",
                self.contract_version
            )));
        }
        validate_identifier("theme", &self.uid)?;
        if self.display_name.trim().is_empty() || self.display_name.len() > 256 {
            return Err(StyleError::new("theme display name is invalid"));
        }
        validate_identifier("mode", &self.default_mode)?;
        if !self.modes.contains_key(&self.default_mode) {
            return Err(StyleError::new("theme default mode is missing"));
        }
        for name in self.extensions.keys() {
            validate_token_name(name)?;
            if !name.starts_with("--lynx-local-") {
                return Err(StyleError::new(format!(
                    "theme extension {name} must use --lynx-local-"
                )));
            }
            if standard_kind(name).is_some() {
                return Err(StyleError::new(format!("invalid theme extension {name}")));
            }
        }
        validate_layer(&self.common, &self.extensions)?;
        for (mode, layer) in &self.modes {
            validate_identifier("mode", mode)?;
            validate_layer(layer, &self.extensions)?;
        }
        for asset in &self.assets {
            validate_asset(asset)?;
        }
        Ok(())
    }

    pub fn validate_complete_mode(&self, mode: &str) -> Result<(), StyleError> {
        self.validate()?;
        let mode_layer = self
            .modes
            .get(mode)
            .ok_or_else(|| StyleError::new(format!("theme mode {mode} is missing")))?;
        let mut values = self.common.values.clone();
        values.extend(mode_layer.values.clone());
        for specification in STYLE_TOKEN_SPECS {
            if !values.contains_key(specification.name) {
                return Err(StyleError::new(format!(
                    "theme mode {mode} omits required token {}",
                    specification.name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleOrigin {
    pub scope: StyleScope,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedStyle {
    pub contract_version: u32,
    pub mode: String,
    pub values: BTreeMap<String, StyleValue>,
    pub origins: BTreeMap<String, StyleOrigin>,
}

impl ResolvedStyle {
    pub fn value(&self, name: &str) -> Result<&StyleValue, StyleError> {
        self.values
            .get(name)
            .ok_or_else(|| StyleError::new(format!("resolved style omits {name}")))
    }

    pub fn origin(&self, name: &str) -> Result<&StyleOrigin, StyleError> {
        self.origins
            .get(name)
            .ok_or_else(|| StyleError::new(format!("resolved style has no origin for {name}")))
    }

    pub fn color_srgba8(&self, name: &str) -> Result<[u8; 4], StyleError> {
        match self.value(name)? {
            StyleValue::Color(value) => parse_color(value)
                .ok_or_else(|| StyleError::new(format!("resolved color {name} is invalid"))),
            _ => Err(StyleError::new(format!(
                "resolved token {name} is not a color"
            ))),
        }
    }

    pub fn color_linear(&self, name: &str) -> Result<[f32; 4], StyleError> {
        let color = self.color_srgba8(name)?;
        Ok([
            srgb_to_linear(color[0]),
            srgb_to_linear(color[1]),
            srgb_to_linear(color[2]),
            f32::from(color[3]) / 255.0,
        ])
    }

    pub fn length_px(&self, name: &str) -> Result<f32, StyleError> {
        match self.value(name)? {
            StyleValue::LengthPx(value) => Ok(*value),
            _ => Err(StyleError::new(format!(
                "resolved token {name} is not a length"
            ))),
        }
    }

    pub fn scalar(&self, name: &str) -> Result<f32, StyleError> {
        match self.value(name)? {
            StyleValue::Scalar(value) => Ok(*value),
            _ => Err(StyleError::new(format!(
                "resolved token {name} is not a scalar"
            ))),
        }
    }

    pub fn css_declarations(&self) -> String {
        self.values
            .iter()
            .map(|(name, value)| format!("{name}:{};", value.to_css()))
            .collect::<Vec<_>>()
            .join("")
    }
}

pub fn resolve_style(
    base: &ThemeManifest,
    selected: Option<&ThemeManifest>,
    mode: &str,
    layers: &[ScopedStyleLayer],
) -> Result<ResolvedStyle, StyleError> {
    base.validate_complete_mode(mode)?;
    if let Some(selected) = selected {
        selected.validate()?;
    }
    let mut previous = StyleScope::DefaultContract;
    for layer in layers {
        if matches!(
            layer.scope,
            StyleScope::DefaultContract | StyleScope::ActiveTheme
        ) || layer.scope <= previous
        {
            return Err(StyleError::new("style cascade scopes are out of order"));
        }
        previous = layer.scope;
    }
    let mut extensions = base.extensions.clone();
    if let Some(selected) = selected {
        for (name, kind) in &selected.extensions {
            match extensions.get(name) {
                Some(existing) if existing != kind => {
                    return Err(StyleError::new(format!(
                        "theme extension {name} changes value kind"
                    )));
                }
                _ => {
                    extensions.insert(name.clone(), *kind);
                }
            }
        }
    }
    for layer in layers {
        validate_layer(&layer.layer, &extensions)?;
    }
    let mut values = BTreeMap::new();
    let mut origins = BTreeMap::new();
    apply_layer(
        &mut values,
        &mut origins,
        StyleScope::DefaultContract,
        format!("{}/common", base.uid),
        &base.common,
    );
    apply_layer(
        &mut values,
        &mut origins,
        StyleScope::DefaultContract,
        format!("{}/{mode}", base.uid),
        base.modes
            .get(mode)
            .ok_or_else(|| StyleError::new(format!("base mode {mode} is missing")))?,
    );
    for layer in layers
        .iter()
        .filter(|layer| layer.scope < StyleScope::ActiveTheme)
    {
        apply_layer(
            &mut values,
            &mut origins,
            layer.scope,
            layer.label.clone(),
            &layer.layer,
        );
    }
    if let Some(selected) = selected {
        apply_layer(
            &mut values,
            &mut origins,
            StyleScope::ActiveTheme,
            format!("{}/common", selected.uid),
            &selected.common,
        );
        if let Some(mode_layer) = selected.modes.get(mode) {
            apply_layer(
                &mut values,
                &mut origins,
                StyleScope::ActiveTheme,
                format!("{}/{mode}", selected.uid),
                mode_layer,
            );
        }
    }
    for layer in layers
        .iter()
        .filter(|layer| layer.scope > StyleScope::ActiveTheme)
    {
        apply_layer(
            &mut values,
            &mut origins,
            layer.scope,
            layer.label.clone(),
            &layer.layer,
        );
    }
    for specification in STYLE_TOKEN_SPECS {
        if !values.contains_key(specification.name) {
            return Err(StyleError::new(format!(
                "resolved style omits required token {}",
                specification.name
            )));
        }
    }
    Ok(ResolvedStyle {
        contract_version: STYLE_CONTRACT_VERSION,
        mode: mode.into(),
        values,
        origins,
    })
}

pub fn lynx_theme() -> ThemeManifest {
    let common = StyleLayer {
        values: BTreeMap::from([
            color("--lynx-palette-purple-cobalt", "#3730A3"),
            color("--lynx-palette-purple-nocturnal", "#6366F1"),
            color("--lynx-palette-deep-lead", "#121214"),
            color("--lynx-palette-gray", "#A7B4C2"),
            color("--lynx-palette-ice-white", "#F8FAFC"),
            length("--lynx-space-half", 2.0),
            length("--lynx-space-1", 4.0),
            length("--lynx-space-2", 8.0),
            length("--lynx-space-3", 12.0),
            length("--lynx-space-4", 16.0),
            length("--lynx-space-5", 24.0),
            length("--lynx-space-6", 32.0),
            length("--lynx-space-7", 48.0),
            length("--lynx-gap-content", 4.0),
            length("--lynx-padding-record", 5.0),
            length("--lynx-padding-control-x", 5.0),
            length("--lynx-padding-control-y", 3.0),
            length("--lynx-control-height", 25.0),
            length("--lynx-icon-size", 15.0),
            length("--lynx-border-hairline", 0.5),
            length("--lynx-border-focus", 2.0),
            length("--lynx-radius-control", 2.0),
            length("--lynx-radius-panel", 0.0),
            font("--lynx-font-body", &["Lato", "sans-serif"]),
            font("--lynx-font-title", &["Aleo", "serif"]),
            font("--lynx-font-mono", &["monospace"]),
            weight("--lynx-font-weight-normal", 400),
            weight("--lynx-font-weight-medium", 500),
            weight("--lynx-font-weight-strong", 600),
            length("--lynx-text-size-body", 14.0),
            length("--lynx-text-size-compact", 12.0),
            length("--lynx-text-size-metadata", 11.0),
            scalar("--lynx-line-height-body", 1.3),
            (
                "--lynx-numeric-figures".into(),
                StyleValue::NumericFigures(NumericFigures::Tabular),
            ),
            offset("--lynx-shadow-x", 2.0),
            offset("--lynx-shadow-y", 2.0),
            length("--lynx-shadow-blur", 6.0),
            offset("--lynx-shadow-light-x", -1.0),
            offset("--lynx-shadow-light-y", -1.0),
            length("--lynx-shadow-light-blur", 4.0),
            scalar("--lynx-opacity-disabled", 0.65),
            scalar("--lynx-opacity-ephemeral", 0.72),
            scalar("--lynx-density-scale", 1.0),
            integer("--lynx-stack-base", 0),
            integer("--lynx-stack-floating", 10),
            integer("--lynx-stack-focus", 20),
            integer("--lynx-stack-menu", 40),
            integer("--lynx-stack-dialog", 60),
            integer("--lynx-stack-security", 100),
            duration("--lynx-motion-state", 0),
            duration("--lynx-motion-direct", 120),
            duration("--lynx-motion-reduced", 0),
            (
                "--lynx-truth-settled".into(),
                StyleValue::LineStyle(LineStyle::Solid),
            ),
            (
                "--lynx-truth-declared".into(),
                StyleValue::LineStyle(LineStyle::Dashed),
            ),
        ]),
    };
    let dark = semantic_mode(
        "#121214", "#121214", "#141416", "#18181B", "#141416", "#1B1B20", "#202027", "#F8FAFC",
        "#A7B4C2", "#121214", "#4A4A55", "#6366F1", "#3730A3", "#F8FAFC", "#6366F1", "#3730A3",
        "#6366F1", "#3730A3", "#A7B4C2", "#A7B4C2", "#A7B4C2", "#A7B4C2", "#D65770",
    );
    let light = semantic_mode(
        "#F8FAFC", "#F8FAFC", "#F1F3F6", "#FFFFFF", "#F8FAFC", "#EEF0F4", "#E5E8ED", "#121214",
        "#50505A", "#F8FAFC", "#A7B4C2", "#3730A3", "#6366F1", "#F8FAFC", "#3730A3", "#6366F1",
        "#3730A3", "#6366F1", "#50505A", "#50505A", "#50505A", "#50505A", "#B3263E",
    );
    ThemeManifest {
        contract_version: STYLE_CONTRACT_VERSION,
        uid: "lince.lynx".into(),
        display_name: "Lynx".into(),
        default_mode: "dark".into(),
        extensions: BTreeMap::new(),
        common,
        modes: BTreeMap::from([("dark".into(), dark), ("light".into(), light)]),
        assets: Vec::new(),
    }
}

pub fn partial_theme_fixture() -> ThemeManifest {
    ThemeManifest {
        contract_version: STYLE_CONTRACT_VERSION,
        uid: "lince.fixture.quiet-purple".into(),
        display_name: "Quiet Purple".into(),
        default_mode: "dark".into(),
        extensions: BTreeMap::from([("--lynx-local-fixture-mark".into(), StyleValueKind::Color)]),
        common: StyleLayer {
            values: BTreeMap::from([
                color("--lynx-accent", "#7C75E8"),
                color("--lynx-focus", "#9B96F2"),
                color("--lynx-local-fixture-mark", "#7C75E8"),
            ]),
        },
        modes: BTreeMap::from([("dark".into(), StyleLayer::default())]),
        assets: Vec::new(),
    }
}

#[derive(Clone, Debug)]
pub struct StyleGalleryState {
    base: ThemeManifest,
    selected: ThemeManifest,
    mode: String,
    selected_enabled: bool,
    palette_index: usize,
    density_index: usize,
    radius_index: usize,
}

impl StyleGalleryState {
    pub fn new() -> Result<Self, StyleError> {
        let state = Self {
            base: lynx_theme(),
            selected: partial_theme_fixture(),
            mode: "dark".into(),
            selected_enabled: false,
            palette_index: 0,
            density_index: 1,
            radius_index: 1,
        };
        state.resolved()?;
        Ok(state)
    }

    pub fn resolved(&self) -> Result<ResolvedStyle, StyleError> {
        let projection = ScopedStyleLayer {
            scope: StyleScope::Projection,
            label: "native retained projection".into(),
            layer: StyleLayer {
                values: BTreeMap::from([length("--lynx-icon-size", 15.0)]),
            },
        };
        let definition = ScopedStyleLayer {
            scope: StyleScope::Definition,
            label: "Button definition".into(),
            layer: StyleLayer {
                values: BTreeMap::from([length("--lynx-radius-control", 2.0)]),
            },
        };
        let workspace = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: "Gallery workspace".into(),
            layer: palette_layer(self.palette_index),
        };
        let group = ScopedStyleLayer {
            scope: StyleScope::Group,
            label: "Gallery Castle".into(),
            layer: StyleLayer {
                values: BTreeMap::from([scalar(
                    "--lynx-density-scale",
                    [0.78, 1.0, 1.22][self.density_index],
                )]),
            },
        };
        let instance = ScopedStyleLayer {
            scope: StyleScope::Instance,
            label: "Button instance".into(),
            layer: StyleLayer {
                values: BTreeMap::from([length(
                    "--lynx-radius-control",
                    [0.0, 2.0, 8.0][self.radius_index],
                )]),
            },
        };
        resolve_style(
            &self.base,
            self.selected_enabled.then_some(&self.selected),
            &self.mode,
            &[projection, definition, workspace, group, instance],
        )
    }

    pub fn cycle_palette(&mut self) {
        self.palette_index = (self.palette_index + 1) % 3;
    }

    pub fn cycle_density(&mut self) {
        self.density_index = (self.density_index + 1) % 3;
    }

    pub fn cycle_radius(&mut self) {
        self.radius_index = (self.radius_index + 1) % 3;
    }

    pub fn toggle_selected_theme(&mut self) {
        self.selected_enabled = !self.selected_enabled;
    }

    pub fn toggle_mode(&mut self) {
        self.mode = if self.mode == "dark" { "light" } else { "dark" }.into();
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn theme(&self) -> &str {
        if self.selected_enabled {
            &self.selected.display_name
        } else {
            &self.base.display_name
        }
    }

    pub fn palette_index(&self) -> usize {
        self.palette_index
    }

    pub fn density_index(&self) -> usize {
        self.density_index
    }

    pub fn radius_index(&self) -> usize {
        self.radius_index
    }
}

fn semantic_mode(
    canvas: &str,
    primary: &str,
    secondary: &str,
    raised: &str,
    input: &str,
    hover: &str,
    active: &str,
    ink: &str,
    secondary_ink: &str,
    inverse_ink: &str,
    border: &str,
    accent: &str,
    supporting: &str,
    accent_ink: &str,
    focus: &str,
    selection: &str,
    need: &str,
    contribution: &str,
    peace: &str,
    info: &str,
    success: &str,
    warning: &str,
    danger: &str,
) -> StyleLayer {
    StyleLayer {
        values: BTreeMap::from([
            color("--lynx-surface-canvas", canvas),
            color("--lynx-surface-primary", primary),
            color("--lynx-surface-secondary", secondary),
            color("--lynx-surface-raised", raised),
            color("--lynx-surface-input", input),
            color("--lynx-surface-hover", hover),
            color("--lynx-surface-active", active),
            color("--lynx-ink-primary", ink),
            color("--lynx-ink-secondary", secondary_ink),
            color("--lynx-ink-inverse", inverse_ink),
            color("--lynx-border", border),
            color("--lynx-accent", accent),
            color("--lynx-accent-supporting", supporting),
            color("--lynx-accent-ink", accent_ink),
            color("--lynx-focus", focus),
            color("--lynx-selection", selection),
            color("--lynx-need", need),
            color("--lynx-contribution", contribution),
            color("--lynx-peace", peace),
            color("--lynx-info", info),
            color("--lynx-success", success),
            color("--lynx-warning", warning),
            color("--lynx-danger", danger),
            color("--lynx-tooltip-surface", primary),
            color("--lynx-tooltip-ink", ink),
            color(
                "--lynx-scrim",
                if canvas == "#121214" {
                    "#12121499"
                } else {
                    "#1212148C"
                },
            ),
            color("--lynx-shadow-color", "#00000029"),
            color("--lynx-shadow-light-color", "#FFFFFF14"),
            color("--lynx-state-hover-surface", hover),
            color("--lynx-state-active-surface", active),
            color("--lynx-state-selected-surface", secondary),
            color("--lynx-state-selected-border", selection),
            color("--lynx-state-read-only-ink", secondary_ink),
            color("--lynx-state-invalid-border", danger),
            color("--lynx-state-loading-ink", secondary_ink),
            color("--lynx-state-empty-ink", secondary_ink),
            color("--lynx-state-focus-border", focus),
        ]),
    }
}

fn palette_layer(index: usize) -> StyleLayer {
    let values = match index {
        0 => BTreeMap::new(),
        1 => BTreeMap::from([
            color("--lynx-accent", "#3730A3"),
            color("--lynx-focus", "#6366F1"),
            color("--lynx-selection", "#3730A3"),
        ]),
        _ => BTreeMap::from([
            color("--lynx-accent", "#A7B4C2"),
            color("--lynx-focus", "#A7B4C2"),
            color("--lynx-selection", "#6366F1"),
        ]),
    };
    StyleLayer { values }
}

fn apply_layer(
    values: &mut BTreeMap<String, StyleValue>,
    origins: &mut BTreeMap<String, StyleOrigin>,
    scope: StyleScope,
    label: String,
    layer: &StyleLayer,
) {
    for (name, value) in &layer.values {
        values.insert(name.clone(), value.clone());
        origins.insert(
            name.clone(),
            StyleOrigin {
                scope,
                label: label.clone(),
            },
        );
    }
}

fn validate_layer(
    layer: &StyleLayer,
    extensions: &BTreeMap<String, StyleValueKind>,
) -> Result<(), StyleError> {
    for (name, value) in &layer.values {
        validate_token_name(name)?;
        let expected = standard_kind(name)
            .or_else(|| extensions.get(name).copied())
            .ok_or_else(|| StyleError::new(format!("undeclared style token {name}")))?;
        if value.kind() != expected {
            return Err(StyleError::new(format!(
                "style token {name} expects {expected:?}"
            )));
        }
        value.validate()?;
    }
    Ok(())
}

fn standard_kind(name: &str) -> Option<StyleValueKind> {
    STYLE_TOKEN_SPECS
        .iter()
        .find(|specification| specification.name == name)
        .map(|specification| specification.kind)
}

fn validate_token_name(value: &str) -> Result<(), StyleError> {
    if !value.starts_with("--lynx-")
        || value.len() > 256
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(StyleError::new("invalid canonical style token name"));
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), StyleError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(StyleError::new(format!("invalid {label} identity")));
    }
    Ok(())
}

fn validate_asset(asset: &ThemeAsset) -> Result<(), StyleError> {
    let path = Path::new(&asset.path);
    if asset.path.contains("://")
        || asset.path.len() > 1024
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StyleError::new(
            "theme asset path must be relative and local",
        ));
    }
    let hash = asset.sha256.strip_prefix("sha256:").unwrap_or_default();
    if hash.len() != 64 || !hash.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(StyleError::new("theme asset hash is invalid"));
    }
    Ok(())
}

fn valid_font_family(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_alphanumeric() || character.is_whitespace() || "-_".contains(character)
        })
}

fn parse_color(value: &str) -> Option<[u8; 4]> {
    let digits = value.strip_prefix('#')?;
    if !matches!(digits.len(), 6 | 8)
        || !digits
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return None;
    }
    let red = u8::from_str_radix(&digits[0..2], 16).ok()?;
    let green = u8::from_str_radix(&digits[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&digits[4..6], 16).ok()?;
    let alpha = if digits.len() == 8 {
        u8::from_str_radix(&digits[6..8], 16).ok()?
    } else {
        255
    };
    Some([red, green, blue, alpha])
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn format_number(value: f32, suffix: &str) -> String {
    if value.fract() == 0.0 {
        format!("{}{suffix}", value as i64)
    } else {
        let mut formatted = format!("{value:.4}");
        while formatted.ends_with('0') {
            formatted.pop();
        }
        format!("{formatted}{suffix}")
    }
}

fn color(name: &str, value: &str) -> (String, StyleValue) {
    (name.into(), StyleValue::Color(value.into()))
}

fn length(name: &str, value: f32) -> (String, StyleValue) {
    (name.into(), StyleValue::LengthPx(value))
}

fn offset(name: &str, value: f32) -> (String, StyleValue) {
    (name.into(), StyleValue::OffsetPx(value))
}

fn scalar(name: &str, value: f32) -> (String, StyleValue) {
    (name.into(), StyleValue::Scalar(value))
}

fn integer(name: &str, value: i32) -> (String, StyleValue) {
    (name.into(), StyleValue::Integer(value))
}

fn font(name: &str, values: &[&str]) -> (String, StyleValue) {
    (
        name.into(),
        StyleValue::FontFamily(values.iter().map(|value| (*value).into()).collect()),
    )
}

fn weight(name: &str, value: u16) -> (String, StyleValue) {
    (name.into(), StyleValue::FontWeight(value))
}

fn duration(name: &str, value: u32) -> (String, StyleValue) {
    (name.into(), StyleValue::DurationMs(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lynx_modes_are_complete_and_resolve_every_scope() {
        let mut gallery = StyleGalleryState::new().unwrap();
        gallery.toggle_selected_theme();
        gallery.cycle_palette();
        gallery.cycle_density();
        gallery.cycle_radius();
        let resolved = gallery.resolved().unwrap();
        assert_eq!(resolved.values.len(), STYLE_TOKEN_SPECS.len() + 1);
        assert_eq!(
            resolved.origin("--lynx-radius-control").unwrap().scope,
            StyleScope::Instance
        );
        assert_eq!(
            resolved.origin("--lynx-density-scale").unwrap().scope,
            StyleScope::Group
        );
        assert_eq!(
            resolved.origin("--lynx-accent").unwrap().scope,
            StyleScope::Workspace
        );
    }

    #[test]
    fn partial_theme_inherits_and_css_projection_is_canonical() {
        let base = lynx_theme();
        let partial = partial_theme_fixture();
        let resolved = resolve_style(&base, Some(&partial), "light", &[]).unwrap();
        let css = resolved.css_declarations();
        assert!(css.contains("--lynx-accent:#7C75E8;"));
        assert!(css.contains("--lynx-surface-canvas:#F8FAFC;"));
        assert!(css.contains("--lynx-shadow-light-x:-1px;"));
        assert!(!css.contains("--canvas:"));
    }

    #[test]
    fn invalid_versions_values_assets_and_scopes_fail_closed() {
        let mut invalid = partial_theme_fixture();
        invalid.contract_version = 99;
        assert!(invalid.validate().is_err());
        let mut invalid = partial_theme_fixture();
        invalid.assets.push(ThemeAsset {
            path: "https://example.test/theme.css".into(),
            sha256: format!("sha256:{}", "0".repeat(64)),
            kind: ThemeAssetKind::Svg,
        });
        assert!(invalid.validate().is_err());
        let layer = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: "late".into(),
            layer: StyleLayer::default(),
        };
        let earlier = ScopedStyleLayer {
            scope: StyleScope::Definition,
            label: "earlier".into(),
            layer: StyleLayer::default(),
        };
        assert!(resolve_style(&lynx_theme(), None, "dark", &[layer, earlier]).is_err());
    }

    #[test]
    fn unknown_tokens_and_mismatched_kinds_fail_closed() {
        let unknown = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: "unknown".into(),
            layer: StyleLayer {
                values: BTreeMap::from([color("--lynx-unknown", "#000000")]),
            },
        };
        assert!(resolve_style(&lynx_theme(), None, "dark", &[unknown]).is_err());
        let wrong_kind = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: "wrong kind".into(),
            layer: StyleLayer {
                values: BTreeMap::from([scalar("--lynx-accent", 1.0)]),
            },
        };
        assert!(resolve_style(&lynx_theme(), None, "dark", &[wrong_kind]).is_err());
        let negative_length = ScopedStyleLayer {
            scope: StyleScope::Workspace,
            label: "negative length".into(),
            layer: StyleLayer {
                values: BTreeMap::from([length("--lynx-radius-control", -1.0)]),
            },
        };
        assert!(resolve_style(&lynx_theme(), None, "dark", &[negative_length]).is_err());
    }
}
