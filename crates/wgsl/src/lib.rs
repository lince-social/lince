//! Lince's WGSL interface and spatial shader settings.
//!
//! The compiler feature enables WGSL admission and callback validation.

pub mod spatial;

#[cfg(feature = "compiler")]
mod compiler;
#[cfg(feature = "compiler")]
mod reserved;
#[cfg(feature = "compiler")]
pub use compiler::compile;

/// Uniform interface shared with the renderer.
pub const HEADER: &str = "struct ShaderInputs { size: vec2<f32>, time: f32, delta: f32, mouse: vec4<f32>, quantity: f32, quantity_present: u32, task_state: u32, effect_strength: f32, motion_allowed: u32, progress: f32, progress_present: u32, padding: u32, numeric0: vec4<f32>, numeric1: vec4<f32>, present0: vec4<u32>, present1: vec4<u32> };\n@group(1) @binding(0) var<uniform> shader_inputs: ShaderInputs;\n";

/// Built-in radial surface shader.
pub const CALM: &str = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let r = length((uv - 0.5) * vec2<f32>(inputs.size.x / max(inputs.size.y, 1.0), 1.0)); let a = 1.0 - smoothstep(0.22, 0.25, r); return vec4<f32>(vec3<f32>(0.35 + 0.15 * (1.0 - uv.y)), a); }";
