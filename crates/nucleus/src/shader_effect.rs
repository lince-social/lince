//! Shader effects, exact numeric inputs, and program selection.
// non-lean-yet

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::DecimalValue;

pub mod bindings;
#[cfg(feature = "shader-compiler")]
pub use lince_wgsl::compile;
pub use lince_wgsl::{CALM, HEADER, spatial};

pub const CLEANING: &str = include_str!("shader_effect/cleaning.wgsl");

pub const NUMERIC: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let r = length(uv - 0.5); let a = exp(-r * 8.0); if inputs.present0.x == 0u { return vec4<f32>(vec3<f32>(0.35), a); } let strength = clamp(abs(inputs.numeric0.x) / 100.0, 0.0, 1.0); let pulse = 0.9 + 0.1 * sin(inputs.time * 2.0); return vec4<f32>(mix(vec3<f32>(0.15, 0.25, 0.4), vec3<f32>(0.1, 0.8, 1.0), strength) * pulse, a); }";

pub const NAMESPACE: &str = "lince.wgsl";
pub const ENERGY: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let p = (uv - 0.5) * vec2<f32>(inputs.size.x / max(inputs.size.y, 1.0), 1.0); let r = length(p); let pulse = 0.8 + 0.2 * sin(inputs.time * 3.0 + abs(inputs.quantity)); let core = exp(-r * r * 50.0); let halo = exp(-r * 8.0); let color = vec3<f32>(0.15, 0.4, 1.0) * halo + vec3<f32>(0.7, 0.8, 1.0) * core; return vec4<f32>(color * pulse, max(core, halo)); }";
pub const GRADIENT: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let color = mix(vec3<f32>(0.04, 0.08, 0.16), vec3<f32>(0.12, 0.2, 0.3), uv.y); return vec4<f32>(color, 1.0); }";
pub const RIPPLE: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let wave = 0.5 + 0.5 * sin(length(uv - 0.5) * 30.0 - inputs.time * 2.0 + inputs.quantity); return vec4<f32>(mix(vec3<f32>(0.04, 0.08, 0.16), vec3<f32>(0.25, 0.12, 0.3), wave), 1.0); }";
pub const GRAIN: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let pixel = floor(uv * inputs.size); let grain = fract(sin(dot(pixel, vec2<f32>(12.9898, 78.233))) * 43758.5453); let color = vec3<f32>(0.08, 0.12, 0.18) + vec3<f32>((grain - 0.5) * 0.045); return vec4<f32>(color, 1.0); }";
pub const REWARD: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let p = (uv - 0.5) * vec2<f32>(inputs.size.x / max(inputs.size.y, 1.0), 1.0); let r = length(p); if inputs.task_state == 3u { let pulse = 0.85 + 0.15 * sin(inputs.time * 3.0); let halo = exp(-r * 8.0); return vec4<f32>(vec3<f32>(0.2, 0.7, 1.0) * pulse, halo); } return vec4<f32>(vec3<f32>(0.4), 1.0 - smoothstep(0.22, 0.25, r)); }";
pub const REMINDER: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let p = (uv - 0.5) * vec2<f32>(inputs.size.x / max(inputs.size.y, 1.0), 1.0); let r = length(p); if inputs.task_state == 1u || inputs.task_state == 2u { let pulse = 0.8 + 0.2 * sin(inputs.time * 5.0); return vec4<f32>(vec3<f32>(1.0, 0.3, 0.1) * pulse, exp(-r * 8.0)); } return vec4<f32>(vec3<f32>(0.4), 1.0 - smoothstep(0.22, 0.25, r)); }";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum TaskState {
    #[default]
    Unknown = 0,
    Pending = 1,
    Active = 2,
    Completed = 3,
}

impl TaskState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Completed => "completed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityRange {
    pub start: String,
    pub end: String,
}

impl Default for QuantityRange {
    fn default() -> Self {
        Self {
            start: "0".into(),
            end: "1".into(),
        }
    }
}

impl QuantityRange {
    fn difference(left: DecimalValue, right: DecimalValue) -> Option<f64> {
        if let Some(value) = left.aligned_sub(right) {
            return Some(value.to_f64());
        }
        let scale = left.scale().min(right.scale());
        let factor = 10_i128.checked_pow(u32::from(left.scale().max(right.scale()) - scale))?;
        let parts = |value: DecimalValue| {
            if value.scale() == scale {
                (value.mantissa(), 0)
            } else {
                (value.mantissa() / factor, value.mantissa() % factor)
            }
        };
        let (left_whole, left_remainder) = parts(left);
        let (right_whole, right_remainder) = parts(right);
        let mut remainder = left_remainder.checked_sub(right_remainder)?;
        let whole = if let Some(mut whole) = left_whole.checked_sub(right_whole) {
            if whole > 0 && remainder < 0 {
                whole -= 1;
                remainder += factor;
            } else if whole < 0 && remainder > 0 {
                whole += 1;
                remainder -= factor;
            }
            whole as f64
        } else {
            left_whole as f64 - right_whole as f64
        };
        Some((whole + remainder as f64 / factor as f64) * 10_f64.powi(-i32::from(scale)))
    }

    fn endpoints(&self) -> Result<(DecimalValue, DecimalValue), String> {
        if self.start.len() > 128 || self.end.len() > 128 {
            return Err("Mapping endpoints are limited to 128 bytes.".into());
        }
        let start = DecimalValue::parse_inferred(&self.start)
            .map_err(|_| "Use decimal mapping endpoints.")?;
        let end = DecimalValue::parse_inferred(&self.end)
            .map_err(|_| "Use decimal mapping endpoints.")?;
        if end.exact_numeric_cmp(start).is_eq() {
            return Err("Mapping start and end must differ.".into());
        }
        Ok((start, end))
    }
}

#[derive(Clone, Copy)]
struct MappedRange {
    start: DecimalValue,
    end: DecimalValue,
}

impl MappedRange {
    fn prepare(range: &QuantityRange) -> Option<Self> {
        crate::performance::add("shader-mapping-ranges-prepared", 1);
        let (start, end) = range.endpoints().ok()?;
        Some(Self { start, end })
    }

    fn state(self, value: DecimalValue) -> TaskState {
        let ascending = self.end.exact_numeric_cmp(self.start).is_gt();
        let from_start = value.exact_numeric_cmp(self.start);
        let from_end = value.exact_numeric_cmp(self.end);
        if ascending && !from_start.is_gt() || !ascending && !from_start.is_lt() {
            TaskState::Pending
        } else if ascending && !from_end.is_lt() || !ascending && !from_end.is_gt() {
            TaskState::Completed
        } else {
            TaskState::Active
        }
    }

    fn progress(self, value: DecimalValue, state: TaskState) -> Option<f32> {
        match state {
            TaskState::Pending => Some(0.0),
            TaskState::Completed => Some(1.0),
            TaskState::Active => {
                let ratio = QuantityRange::difference(value, self.start)?
                    / QuantityRange::difference(self.end, self.start)?;
                ratio.is_finite().then(|| ratio.clamp(0.0, 1.0) as f32)
            }
            TaskState::Unknown => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputMappings {
    pub task: Option<QuantityRange>,
    pub progress: Option<QuantityRange>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResolvedInputs {
    pub task: TaskState,
    pub progress: Option<f32>,
}

impl ResolvedInputs {
    pub fn label(self) -> String {
        crate::performance::add("shader-mapping-label-formats", 1);
        match self.progress {
            Some(value) => format!(
                "Task: {} · Progress: {:.1}%",
                self.task.label(),
                value * 100.0
            ),
            None => format!("Task: {} · Progress: unavailable", self.task.label()),
        }
    }
}

impl InputMappings {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(range) = &self.task {
            range.endpoints()?;
        }
        if let Some(range) = &self.progress
            && self.task.as_ref() != Some(range)
        {
            range.endpoints()?;
        }
        Ok(())
    }

    pub fn resolve(&self, quantity: Option<&str>) -> ResolvedInputs {
        let Some(value) = quantity
            .filter(|value| value.len() <= 128 && gpu_quantity(Some(value)).is_some())
            .and_then(|value| DecimalValue::parse_inferred(value).ok())
        else {
            return ResolvedInputs::default();
        };
        let task_range = self.task.as_ref().and_then(MappedRange::prepare);
        let task = task_range
            .map(|range| range.state(value))
            .unwrap_or_default();
        let progress = self.progress.as_ref().and_then(|range| {
            let same = self.task.as_ref() == Some(range);
            let range = if same {
                task_range
            } else {
                MappedRange::prepare(range)
            }?;
            let state = if same { task } else { range.state(value) };
            range.progress(value, state)
        });
        ResolvedInputs { task, progress }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Less,
    AtMost,
    Equal,
    AtLeast,
    Greater,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub operator: Operator,
    pub value: String,
    pub program: String,
    #[serde(default)]
    pub binding: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub version: u32,
    #[serde(default)]
    pub mode: spatial::Mode,
    pub programs: BTreeMap<String, String>,
    pub rules: Vec<Rule>,
    pub fallback: String,
    #[serde(default)]
    pub inputs: InputMappings,
    #[serde(default)]
    pub bindings: Vec<bindings::NumericBinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extension {
    pub revision: u64,
    pub default_effect: String,
    pub effects: BTreeMap<String, NamedEffect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedEffect {
    pub label: String,
    pub effect: Effect,
}

pub fn valid_effect_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

impl Extension {
    pub fn single(revision: u64, effect: Effect) -> Self {
        Self {
            revision,
            default_effect: "main".into(),
            effects: [(
                "main".into(),
                NamedEffect {
                    label: "Main".into(),
                    effect,
                },
            )]
            .into(),
        }
    }

    pub fn selected(&self, id: Option<&str>) -> Option<&Effect> {
        self.effects
            .get(id.unwrap_or(&self.default_effect))
            .map(|named| &named.effect)
    }

    #[cfg(feature = "shader-compiler")]
    pub fn validate(&self) -> Result<(), String> {
        if self.effects.is_empty()
            || self.effects.len() > 8
            || !self.effects.contains_key(&self.default_effect)
        {
            return Err("Use 1–8 effects and choose an existing default effect.".into());
        }
        if crate::json::encoded_len(self).map_err(|error| error.to_string())? > 196_608 {
            return Err(
                "The complete effect catalog is limited to 192 KiB of serialized data.".into(),
            );
        }
        let bytes = self
            .effects
            .values()
            .flat_map(|named| named.effect.sources())
            .map(str::len)
            .sum::<usize>();
        if bytes > 131_072 {
            return Err("The complete effect catalog is limited to 128 KiB of WGSL.".into());
        }
        for (id, named) in &self.effects {
            if !valid_effect_id(id)
                || named.label.trim().is_empty()
                || named.label.len() > 64
                || named.label.chars().any(char::is_control)
            {
                return Err("Use stable effect IDs and 1–64 bytes of readable label text.".into());
            }
            named
                .effect
                .validate()
                .map_err(|error| format!("{}: {error}", named.label))?;
        }
        Ok(())
    }

    pub fn bounded(&self) -> bool {
        !self.effects.is_empty()
            && self.effects.len() <= 8
            && self.effects.contains_key(&self.default_effect)
            && self.effects.iter().all(|(id, named)| {
                valid_effect_id(id)
                    && named.effect.mode.bounded()
                    && !named.label.trim().is_empty()
                    && named.label.len() <= 64
                    && !named.label.chars().any(char::is_control)
                    && !named.effect.programs.is_empty()
                    && named.effect.programs.len() <= 8
                    && named.effect.rules.len() <= 16
                    && named
                        .effect
                        .programs
                        .iter()
                        .all(|(name, source)| name.len() <= 64 && source.len() <= 65_536)
                    && named
                        .effect
                        .rules
                        .iter()
                        .all(|rule| rule.value.len() <= 128 && rule.program.len() <= 64)
                    && bindings::bounded(&named.effect.bindings)
            })
            && self
                .effects
                .values()
                .flat_map(|named| named.effect.sources())
                .map(str::len)
                .sum::<usize>()
                <= 131_072
            && crate::json::encoded_len(self).is_ok_and(|data| data <= 196_608)
    }
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            version: 1,
            mode: spatial::Mode::Surface,
            programs: BTreeMap::from([
                ("negative".into(), ENERGY.into()),
                ("nonnegative".into(), CALM.into()),
                ("neutral".into(), CALM.into()),
            ]),
            rules: vec![
                Rule {
                    operator: Operator::Less,
                    binding: None,
                    value: "0".into(),
                    program: "negative".into(),
                },
                Rule {
                    operator: Operator::AtLeast,
                    binding: None,
                    value: "0".into(),
                    program: "nonnegative".into(),
                },
            ],
            fallback: "neutral".into(),
            inputs: InputMappings::default(),
            bindings: vec![],
        }
    }
}

pub fn quantity_text(value: &str) -> Option<&str> {
    crate::performance::add("shader-quantity-validations", 1);
    (value.len() <= 128 && DecimalValue::parse_inferred(value).is_ok()).then_some(value)
}

pub fn quantity(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => quantity_text(value).map(str::to_owned),
        serde_json::Value::Number(value) => {
            let value = value.to_string();
            quantity_text(&value).is_some().then_some(value)
        }
        _ => None,
    }
}

pub fn gpu_quantity(value: Option<&str>) -> Option<f32> {
    let value = value?.parse::<f32>().ok()?;
    value.is_finite().then_some(value)
}

impl Effect {
    pub fn same_authored(&self, other: &Self) -> bool {
        self == other
            && match (&self.mode, &other.mode) {
                (spatial::Mode::Mesh(first), spatial::Mode::Mesh(second)) => {
                    first
                        .deformation
                        .as_ref()
                        .map(|stage| stage.max_displacement.to_bits())
                        == second
                            .deformation
                            .as_ref()
                            .map(|stage| stage.max_displacement.to_bits())
                }
                (spatial::Mode::Volume(first), spatial::Mode::Volume(second)) => {
                    first.density.to_bits() == second.density.to_bits()
                }
                _ => true,
            }
    }

    pub fn sources(&self) -> impl Iterator<Item = &str> {
        self.programs
            .values()
            .map(String::as_str)
            .chain(self.mode.sources())
    }

    #[cfg(feature = "shader-compiler")]
    pub fn validate(&self) -> Result<(), String> {
        self.inputs.validate()?;
        bindings::validate(&self.bindings)?;
        self.mode.validate()?;
        if self.version != 1
            || self.programs.is_empty()
            || self.programs.len() > 8
            || self.rules.len() > 16
        {
            return Err("Use contract version 1, 1–8 programs and at most 16 rules.".into());
        }
        if !self.programs.contains_key(&self.fallback) {
            return Err("Choose an existing fallback program.".into());
        }
        let mut bytes = self.mode.sources().map(str::len).sum::<usize>();
        for (name, source) in &self.programs {
            if !valid_effect_id(name) {
                return Err(
                    "Program names use 1–64 letters, digits, underscores or hyphens.".into(),
                );
            }
            bytes += source.len();
            if bytes > 131_072 {
                return Err("The effect bundle is limited to 128 KB.".into());
            }
            self.mode
                .validate_program(source)
                .map_err(|error| format!("{name}: {error}"))?;
        }
        for rule in &self.rules {
            if rule.value.len() > 128
                || DecimalValue::parse_inferred(&rule.value).is_err()
                || !self.programs.contains_key(&rule.program)
                || rule
                    .binding
                    .is_some_and(|slot| !self.bindings.iter().any(|binding| binding.slot == slot))
            {
                return Err("Every rule needs a decimal threshold and an existing program.".into());
            }
        }
        Ok(())
    }

    pub fn selected(&self, value: Option<&str>) -> &str {
        self.selected_with_inputs(value, &bindings::NumericValues::default())
    }

    pub fn selected_with_inputs(
        &self,
        quantity: Option<&str>,
        inputs: &bindings::NumericValues,
    ) -> &str {
        for rule in &self.rules {
            let value = rule.binding.map_or(quantity, |slot| inputs.get(slot));
            let Some(value) = value
                .filter(|value| value.len() <= 128)
                .and_then(|value| DecimalValue::parse_inferred(value).ok())
            else {
                continue;
            };
            let Ok(threshold) = DecimalValue::parse_inferred(&rule.value) else {
                continue;
            };
            let order = value.exact_numeric_cmp(threshold);
            let matched = match rule.operator {
                Operator::Less => order.is_lt(),
                Operator::AtMost => !order.is_gt(),
                Operator::Equal => order.is_eq(),
                Operator::AtLeast => !order.is_lt(),
                Operator::Greater => order.is_gt(),
            };
            if matched {
                return &rule.program;
            }
        }
        &self.fallback
    }
}

#[cfg(test)]
mod tests;
