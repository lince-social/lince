// non-lean-yet
// Scope: mod bindings, mod reserved, mod spatial, const CLEANING.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 605, 651, 715, 739, 869, 886.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 87, 89, 91, 142.
// - [Clippy: clippy::cast_possible_truncation] Prove the bounds or use a checked conversion. Sites
//   142.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 27, 216.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   99, 101.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 177, 294, 447, 568.
// - [Clippy: clippy::redundant_clone] Move or borrow at the marked sites and check later uses.
//   Sites 868.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 7,
//   9, 11, 13, 14, 16, 17, 18, 19, 20, 21, 22, 27, 29, 30, 31, 32, 36, 48, 49, 50, 151, 152, 153,
//   157, 158, 159, 163, 177, 189, 216, 217, 218, 219, 220, 221, 226, 227, 228, 229, 231, 236, 237,
//   239, 240, 241, 242, 244, 246, 251, 252, 253, 254, 259, 260, 261, 264, 273, 288, 294, 331, 398,
//   403, 414, 420, 440, 447, 495, 499, 568.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites compile: String
//   (568), validate: String (177, 294, 447).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   549.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::DecimalValue;

pub mod bindings;
mod reserved;
pub mod spatial;

pub const CLEANING: &str = include_str!("shader_effect/cleaning.wgsl");

pub const NUMERIC: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let r = length(uv - 0.5); let a = exp(-r * 8.0); if inputs.present0.x == 0u { return vec4<f32>(vec3<f32>(0.35), a); } let strength = clamp(abs(inputs.numeric0.x) / 100.0, 0.0, 1.0); let pulse = 0.9 + 0.1 * sin(inputs.time * 2.0); return vec4<f32>(mix(vec3<f32>(0.15, 0.25, 0.4), vec3<f32>(0.1, 0.8, 1.0), strength) * pulse, a); }";

pub const NAMESPACE: &str = "lince.wgsl";
pub const HEADER: &str = "struct ShaderInputs { size: vec2<f32>, time: f32, delta: f32, mouse: vec4<f32>, quantity: f32, quantity_present: u32, task_state: u32, effect_strength: f32, motion_allowed: u32, progress: f32, progress_present: u32, padding: u32, numeric0: vec4<f32>, numeric1: vec4<f32>, present0: vec4<u32>, present1: vec4<u32> };\n@group(1) @binding(0) var<uniform> shader_inputs: ShaderInputs;\n";
const FOOTER: &str = "\n@fragment fn fragment(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { let color = clamp(shade(uv, shader_inputs), vec4<f32>(0.0), vec4<f32>(1.0)); return vec4<f32>(color.rgb, color.a * clamp(shader_inputs.effect_strength, 0.0, 1.0)); }\n";
pub const CALM: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let r = length((uv - 0.5) * vec2<f32>(inputs.size.x / max(inputs.size.y, 1.0), 1.0)); let a = 1.0 - smoothstep(0.22, 0.25, r); return vec4<f32>(vec3<f32>(0.35 + 0.15 * (1.0 - uv.y)), a); }";
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
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            {
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

fn bounded(block: &naga::Block) -> bool {
    block.iter().all(|statement| match statement {
        naga::Statement::Loop { .. } => false,
        naga::Statement::Block(block) => bounded(block),
        naga::Statement::If { accept, reject, .. } => bounded(accept) && bounded(reject),
        naga::Statement::Switch { cases, .. } => cases.iter().all(|case| bounded(&case.body)),
        _ => true,
    })
}

fn charge(block: &naga::Block, module: &naga::Module, remaining: &mut usize) -> bool {
    block.iter().all(|statement| {
        let Some(next) = remaining.checked_sub(1) else {
            return false;
        };
        *remaining = next;
        match statement {
            naga::Statement::Call { function, .. } => {
                let function = &module.functions[*function];
                let Some(next) = remaining.checked_sub(function.expressions.len()) else {
                    return false;
                };
                *remaining = next;
                charge(&function.body, module, remaining)
            }
            naga::Statement::Block(block) => charge(block, module, remaining),
            naga::Statement::If { accept, reject, .. } => {
                charge(accept, module, remaining) && charge(reject, module, remaining)
            }
            naga::Statement::Switch { cases, .. } => cases
                .iter()
                .all(|case| charge(&case.body, module, remaining)),
            _ => true,
        }
    })
}

pub fn compile(source: &str) -> Result<String, String> {
    compile_with_budget(source, 1024)
}

fn compile_with_budget(source: &str, budget: usize) -> Result<String, String> {
    compile_with_callback_budget(source, budget, None)
}

fn compile_with_callback_budget(
    source: &str,
    budget: usize,
    callback: Option<&str>,
) -> Result<String, String> {
    if source.len() > 65_536 || source.lines().count() > 2048 {
        return Err("WGSL is limited to 64 KB and 2048 lines.".into());
    }
    let text = format!("{HEADER}{source}{FOOTER}");
    let module =
        naga::front::wgsl::parse_str(&text).map_err(|error| error.emit_to_string(&text))?;
    if let Some(name) = module
        .functions
        .iter()
        .filter_map(|(_, function)| function.name.as_deref())
        .find(|name| reserved::renderer_function(name))
    {
        return Err(format!(
            "The function name {name} is reserved for renderer bounds, transforms and lighting. Rename your helper; WGSL built-ins remain available."
        ));
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|error| error.emit_to_string(&text))?;
    if !module.overrides.is_empty()
        || module.entry_points.len() != 1
        || module.entry_points[0].name != "fragment"
    {
        return Err("Write fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32>; Lince supplies the fragment entry point. Overrides are unsupported.".into());
    }
    if module.global_variables.iter().any(|(_, variable)| {
        variable.binding.is_some() && variable.name.as_deref() != Some("shader_inputs")
    }) {
        return Err("ShaderInputs is the only supported resource binding.".into());
    }
    if module.global_variables.len() != 1
        || module.types.iter().any(|(_, ty)| {
            matches!(
                ty.inner,
                naga::TypeInner::Array { .. } | naga::TypeInner::BindingArray { .. }
            ) || matches!(ty.inner, naga::TypeInner::Struct { span, .. } if span > 256)
        })
    {
        return Err("Use constants and function-local values; arrays, extra global variables and structures larger than 256 bytes are unsupported.".into());
    }
    if module.functions.len() > 32
        || module
            .functions
            .iter()
            .any(|(_, function)| function.expressions.len() > 1024 || !bounded(&function.body))
    {
        return Err(
            "Use at most 32 functions and 1024 expressions per function; loops are unsupported."
                .into(),
        );
    }
    let mut remaining = budget;
    let within_budget = if let Some(callback) = callback {
        let function = module
            .functions
            .iter()
            .find(|(_, function)| function.name.as_deref() == Some(callback))
            .map(|(_, function)| function)
            .ok_or("The shader callback is missing.")?;
        remaining
            .checked_sub(function.expressions.len())
            .is_some_and(|next| {
                remaining = next;
                charge(&function.body, &module, &mut remaining)
            })
    } else {
        charge(
            &module.entry_points[0].function.body,
            &module,
            &mut remaining,
        )
    };
    if !within_budget {
        return Err(format!(
            "Expanded shader work exceeds {budget} expressions and statements."
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_lighting_calls_cannot_be_overridden_by_unused_helpers() {
        for name in [
            "mix",
            "inverseSqrt",
            "sqrt",
            "reflect",
            "textureDimensions",
            "lince_lit_color",
            "helperX_naga_oil_mod_XAAAX",
        ] {
            let source = format!("fn {name}(x: f32) -> f32 {{ return x; }}\n{CALM}");
            assert!(compile(&source).unwrap_err().contains("reserved"), "{name}");
        }
        assert!(
            compile(&format!(
                "fn user_tint(x: f32) -> f32 {{ return x; }}\n{CALM}"
            ))
            .is_ok()
        );
    }

    #[test]
    fn catalog_identity_defaults_and_limits_are_explicit() {
        let mut catalog = Extension::single(1, Effect::default());
        for index in 1..8 {
            catalog.effects.insert(
                format!("effect-{index}"),
                NamedEffect {
                    label: format!("Effect {index}"),
                    effect: Effect::default(),
                },
            );
        }
        assert!(catalog.validate().is_ok());
        assert!(catalog.bounded());
        assert_eq!(catalog.selected(None), catalog.selected(Some("main")));
        assert!(catalog.selected(Some("missing")).is_none());
        catalog.default_effect = "effect-1".into();
        assert_eq!(catalog.selected(None), catalog.selected(Some("effect-1")));
        let stored = serde_json::to_value(&catalog).unwrap();
        assert_eq!(
            serde_json::from_value::<Extension>(stored).unwrap(),
            catalog
        );
        catalog
            .effects
            .insert("ninth".into(), catalog.effects["main"].clone());
        assert!(catalog.validate().is_err());
        assert!(!catalog.bounded());
        catalog.effects.remove("ninth");
        catalog.default_effect = "missing".into();
        assert!(catalog.validate().is_err());
        catalog.default_effect = "main".into();
        for label in ["", "   ", "bad\nlabel"] {
            catalog.effects.get_mut("main").unwrap().label = label.into();
            assert!(catalog.validate().is_err());
        }
        catalog.effects.get_mut("main").unwrap().label = "Main".into();
        let effect = catalog.effects.remove("effect-1").unwrap();
        catalog.effects.insert("invalid id".into(), effect);
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn catalog_bounds_combined_source_and_encoded_data() {
        let mut effect = Effect::default();
        effect.rules.clear();
        effect.fallback = "only".into();
        effect.programs = [("only".into(), format!("{CALM}{}", " ".repeat(60_000)))].into();
        let mut catalog = Extension::single(1, effect);
        let main = catalog.effects["main"].clone();
        catalog.effects.insert("second".into(), main.clone());
        assert!(catalog.validate().is_ok());
        catalog.effects.insert("third".into(), main);
        assert!(catalog.validate().unwrap_err().contains("128 KiB"));
        assert!(!catalog.bounded());
        catalog.effects.remove("third");
        for named in catalog.effects.values_mut() {
            let source = named.effect.programs.get_mut("only").unwrap();
            *source = format!("{CALM}{}", "\n".repeat(60_000));
        }
        assert!(catalog.validate().unwrap_err().contains("192 KiB"));
        assert!(!catalog.bounded());
    }

    #[test]
    fn mappings_are_opt_in_exact_directional_and_clamped() {
        assert_eq!(
            InputMappings::default().resolve(Some("-3")),
            ResolvedInputs::default()
        );
        let mut mapping = InputMappings {
            task: Some(QuantityRange {
                start: "-3".into(),
                end: "0".into(),
            }),
            progress: Some(QuantityRange {
                start: "-3".into(),
                end: "0".into(),
            }),
        };
        assert!(mapping.validate().is_ok());
        for (value, state, progress) in [
            ("-4", TaskState::Pending, 0.0),
            ("-3", TaskState::Pending, 0.0),
            ("-1.5", TaskState::Active, 0.5),
            ("0", TaskState::Completed, 1.0),
            ("7", TaskState::Completed, 1.0),
        ] {
            assert_eq!(
                mapping.resolve(Some(value)),
                ResolvedInputs {
                    task: state,
                    progress: Some(progress)
                }
            );
        }
        assert_eq!(
            mapping.resolve(Some("-0.000000000000000001")).task,
            TaskState::Active
        );
        mapping.task = Some(QuantityRange {
            start: "10".into(),
            end: "0".into(),
        });
        mapping.progress = mapping.task.clone();
        assert_eq!(
            mapping.resolve(Some("5")),
            ResolvedInputs {
                task: TaskState::Active,
                progress: Some(0.5)
            }
        );
        assert_eq!(mapping.resolve(Some("11")).task, TaskState::Pending);
        assert_eq!(mapping.resolve(Some("-1")).task, TaskState::Completed);
        for missing in [None, Some("NaN"), Some("1e100"), Some("invalid")] {
            assert_eq!(mapping.resolve(missing), ResolvedInputs::default());
        }
        let start = "10000000000000000";
        let end = "10000000000000002";
        mapping.progress = Some(QuantityRange {
            start: start.into(),
            end: end.into(),
        });
        assert_eq!(
            mapping.resolve(Some("10000000000000001")).progress,
            Some(0.5)
        );
        mapping.progress = Some(QuantityRange {
            start: "-17014118346046923173168730371588410573".into(),
            end: "-17014118346046923173168730371588410572".into(),
        });
        assert_eq!(
            mapping
                .resolve(Some("-17014118346046923173168730371588410572.6"))
                .progress,
            Some(0.4)
        );
        mapping.progress = Some(QuantityRange {
            start: "-170141183460469231731687303715884105727".into(),
            end: "0".into(),
        });
        assert_eq!(mapping.resolve(Some("-0.1")).progress, Some(1.0));
        mapping.progress.as_mut().unwrap().end = "170141183460469231731687303715884105727".into();
        assert!(mapping.validate().is_ok());
        assert_eq!(mapping.resolve(Some("0")).progress, Some(0.5));
        let stored = serde_json::to_value(&mapping).unwrap();
        assert_eq!(
            serde_json::from_value::<InputMappings>(stored).unwrap(),
            mapping
        );
    }

    #[test]
    fn mappings_reject_invalid_ranges_and_task_presets_compile() {
        let mut effect = Effect::default();
        for (start, end) in [("0", "0.0"), ("NaN", "1"), ("0", "1e100")] {
            effect.inputs.task = Some(QuantityRange {
                start: start.into(),
                end: end.into(),
            });
            assert!(effect.validate().is_err());
        }
        assert!(compile(REWARD).is_ok());
        assert!(compile(REMINDER).is_ok());
        assert!(compile(GRAIN).is_ok());
        assert!(compile(NUMERIC).is_ok());
        assert!(compile(CLEANING).is_ok());
    }

    #[test]
    fn rules_use_decimal_boundaries_and_fallbacks() {
        let effect = Effect::default();
        assert!(effect.validate().is_ok());
        assert_eq!(effect.selected(Some("-0.000000000000000001")), "negative");
        assert_eq!(effect.selected(Some("0")), "nonnegative");
        assert_eq!(effect.selected(Some("1")), "nonnegative");
        assert_eq!(effect.selected(None), "neutral");
        assert_eq!(effect.selected(Some("NaN")), "neutral");
        let mut overlap = effect.clone();
        overlap.rules[0].operator = Operator::AtLeast;
        assert_eq!(overlap.selected(Some("0")), "negative");
    }

    #[test]
    fn supported_programs_reject_unbounded_work_and_extra_bindings() {
        for source in [CALM, ENERGY, GRADIENT, RIPPLE] {
            assert!(compile(source).is_ok());
        }
        assert!(compile("fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { loop {} return vec4<f32>(1.0); }").is_err());
        assert!(
            compile(&format!(
                "@group(2) @binding(0) var image: texture_2d<f32>;{CALM}"
            ))
            .is_err()
        );
        let mut effect = Effect::default();
        effect.rules[0].program = "absent".into();
        assert!(effect.validate().is_err());
        assert!(gpu_quantity(Some("inf")).is_none());
        assert!(compile(&format!("var<private> huge: array<f32, 1000000>;{CALM}")).is_err());
    }
}
