//! Compiles the restricted shader source used by Lince.

use naga::{
    Block, Module, Statement, TypeInner,
    front::wgsl::parse_str,
    valid::{Capabilities, ValidationFlags, Validator},
};

use crate::{HEADER, reserved};

const FOOTER: &str = "\n@fragment fn fragment(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { let color = clamp(shade(uv, shader_inputs), vec4<f32>(0.0), vec4<f32>(1.0)); return vec4<f32>(color.rgb, color.a * clamp(shader_inputs.effect_strength, 0.0, 1.0)); }\n";

fn bounded(block: &Block) -> bool {
    block.iter().all(|statement| match statement {
        Statement::Loop { .. } => false,
        Statement::Block(block) => bounded(block),
        Statement::If { accept, reject, .. } => bounded(accept) && bounded(reject),
        Statement::Switch { cases, .. } => cases.iter().all(|case| bounded(&case.body)),
        _ => true,
    })
}

fn charge(block: &Block, module: &Module, remaining: &mut usize) -> bool {
    block.iter().all(|statement| {
        let Some(next) = remaining.checked_sub(1) else {
            return false;
        };
        *remaining = next;
        match statement {
            Statement::Call { function, .. } => {
                let function = &module.functions[*function];
                let Some(next) = remaining.checked_sub(function.expressions.len()) else {
                    return false;
                };
                *remaining = next;
                charge(&function.body, module, remaining)
            }
            Statement::Block(block) => charge(block, module, remaining),
            Statement::If { accept, reject, .. } => {
                charge(accept, module, remaining) && charge(reject, module, remaining)
            }
            Statement::Switch { cases, .. } => cases
                .iter()
                .all(|case| charge(&case.body, module, remaining)),
            _ => true,
        }
    })
}

/// Wraps a surface shader with Lince's uniform interface and fragment entry.
///
/// # Errors
///
/// Returns an error for invalid WGSL, unsupported features, or exceeded work limits.
pub fn compile(source: &str) -> Result<String, String> {
    compile_with_budget(source, 1024)
}

pub(super) fn compile_with_budget(source: &str, budget: usize) -> Result<String, String> {
    compile_with_callback_budget(source, budget, None)
}

pub(super) fn compile_with_callback_budget(
    source: &str,
    budget: usize,
    callback: Option<&str>,
) -> Result<String, String> {
    if source.len() > 65_536 || source.lines().count() > 2048 {
        return Err("WGSL is limited to 64 KB and 2048 lines.".into());
    }
    let text = format!("{HEADER}{source}{FOOTER}");
    let module = parse_str(&text).map_err(|error| error.emit_to_string(&text))?;
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
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .map_err(|error| error.emit_to_string(&text))?;
    let entry = match module.entry_points.as_slice() {
        [entry] if module.overrides.is_empty() && entry.name == "fragment" => entry,
        _ => return Err("Write fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32>; Lince supplies the fragment entry point. Overrides are unsupported.".into()),
    };
    if module.global_variables.iter().any(|(_, variable)| {
        variable.binding.is_some() && variable.name.as_deref() != Some("shader_inputs")
    }) {
        return Err("ShaderInputs is the only supported resource binding.".into());
    }
    if module.global_variables.len() != 1
        || module.types.iter().any(|(_, ty)| {
            matches!(
                ty.inner,
                TypeInner::Array { .. } | TypeInner::BindingArray { .. }
            ) || matches!(ty.inner, TypeInner::Struct { span, .. } if span > 256)
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
        charge(&entry.function.body, &module, &mut remaining)
    };
    if !within_budget {
        return Err(format!(
            "Expanded shader work exceeds {budget} expressions and statements."
        ));
    }
    Ok(text)
}
