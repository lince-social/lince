use bevy::{
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType},
    shader::{Shader, ValidateShader},
    ui::RelativeCursorPosition,
    ui_render::ui_material::UiMaterialKey,
};
use std::time::{Duration, Instant};

pub(crate) const BALL: &str = include_str!("shader_ball.wgsl");

const HEADER: &str = "struct ShaderInputs { size: vec2<f32>, time: f32, delta: f32, mouse: vec4<f32> };\n@group(1) @binding(0) var<uniform> shader_inputs: ShaderInputs;\n";
const FOOTER: &str = "\n@fragment fn fragment(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> { return shade(uv, shader_inputs); }\n";

#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct ShaderInputs {
    size: Vec2,
    time: f32,
    delta: f32,
    mouse: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(ShaderKey)]
struct ShaderMaterial {
    #[uniform(0)]
    inputs: ShaderInputs,
    shader: Handle<Shader>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ShaderKey(Handle<Shader>);

impl From<&ShaderMaterial> for ShaderKey {
    fn from(material: &ShaderMaterial) -> Self {
        Self(material.shader.clone())
    }
}

impl UiMaterial for ShaderMaterial {
    fn specialize(descriptor: &mut RenderPipelineDescriptor, key: UiMaterialKey<Self>) {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = key.bind_group_data.0;
        }
    }
}

#[derive(Component)]
pub(super) struct Preview {
    source: String,
    pending: Option<Instant>,
    started: Instant,
    surface: Entity,
    label: Entity,
    material: Option<Handle<ShaderMaterial>>,
}

pub(super) fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        app.add_plugins(UiMaterialPlugin::<ShaderMaterial>::default());
    }
    app.add_systems(Update, update.after(super::sync_editors));
}

pub(super) fn spawn(world: &mut World, parent: Entity, source: &str, old: Option<Entity>) {
    if let Some(old) = old {
        let mut preview = world.get_mut::<Preview>(old).unwrap();
        if preview.source != source {
            preview.source = source.into();
            preview.pending = Some(Instant::now());
        }
        world.entity_mut(old).insert(ChildOf(parent));
        return;
    }
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let surface = world
        .spawn((
            Node {
                width: percent(100),
                height: px(280),
                flex_shrink: 0.0,
                ..default()
            },
            RelativeCursorPosition::default(),
            ChildOf(owner),
        ))
        .id();
    let label = crate::edit_mode::label(world, owner, "Compiling WGSL…", 12.0);
    world.entity_mut(owner).insert(Preview {
        source: source.into(),
        pending: Some(Instant::now()),
        started: Instant::now(),
        surface,
        label,
        material: None,
    });
}

fn compile(source: &str) -> Result<String, String> {
    if source.len() > 65_536 || source.lines().count() > 2048 {
        return Err("WGSL is limited to 64 KB and 2048 lines.".into());
    }
    let text = format!("{HEADER}{source}{FOOTER}");
    let module =
        naga::front::wgsl::parse_str(&text).map_err(|error| error.emit_to_string(&text))?;
    if !module.overrides.is_empty() {
        return Err("Use const for shader constants; pipeline overrides are not supported.".into());
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|error| error.emit_to_string(&text))?;
    if module.entry_points.len() != 1 || module.entry_points[0].name != "fragment" {
        return Err("Write fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32>; the preview supplies the entry point.".into());
    }
    if module.global_variables.iter().any(|(_, variable)| {
        variable.binding.is_some() && variable.name.as_deref() != Some("shader_inputs")
    }) {
        return Err(
            "The preview provides ShaderInputs only; extra resource bindings are not supported."
                .into(),
        );
    }
    Ok(text)
}

fn visible(world: &World, entity: Entity) -> bool {
    if world
        .get::<ComputedNode>(entity)
        .is_none_or(ComputedNode::is_empty)
        || world
            .get::<InheritedVisibility>(entity)
            .is_some_and(|visible| !visible.get())
    {
        return false;
    }
    if let (Some(node), Some(transform), Some(target)) = (
        world.get::<ComputedNode>(entity),
        world.get::<UiGlobalTransform>(entity),
        world.get::<ComputedUiRenderTargetInfo>(entity),
    ) {
        let bounds = Rect::from_center_size(transform.affine().translation, node.size());
        let mut clip = Rect::from_corners(Vec2::ZERO, target.physical_size().as_vec2());
        if let Some(calculated) = world.get::<CalculatedClip>(entity) {
            clip = clip.intersect(calculated.clip);
        }
        if bounds.intersect(clip).is_empty() {
            return false;
        }
    }
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
        {
            return false;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    true
}

fn update(world: &mut World) {
    let previews: Vec<_> = world
        .query_filtered::<Entity, With<Preview>>()
        .iter(world)
        .collect();
    let mut active = false;
    for entity in previews {
        let preview = world.get::<Preview>(entity).unwrap();
        let (surface, label) = (preview.surface, preview.label);
        if !visible(world, surface) {
            continue;
        }
        active |= preview.pending.is_some() || preview.material.is_some();
        if preview
            .pending
            .is_some_and(|at| at.elapsed() >= Duration::from_millis(200))
        {
            let result = compile(&preview.source);
            let current = preview.material.clone();
            world.get_mut::<Preview>(entity).unwrap().pending = None;
            let result = result.and_then(|text| {
                if !world.contains_resource::<Assets<Shader>>()
                    || !world.contains_resource::<Assets<ShaderMaterial>>()
                {
                    return Err("Shader rendering needs the GPU renderer.".into());
                }
                let mut shader =
                    Shader::from_wgsl(text, format!("description-{}.wgsl", entity.to_bits()));
                shader.validate_shader = ValidateShader::Enabled;
                if let Some(material) = current {
                    let handle = world
                        .resource::<Assets<ShaderMaterial>>()
                        .get(&material)
                        .unwrap()
                        .shader
                        .clone();
                    world
                        .resource_mut::<Assets<Shader>>()
                        .insert(handle.id(), shader)
                        .map_err(|error| error.to_string())?;
                } else {
                    let shader = world.resource_mut::<Assets<Shader>>().add(shader);
                    let material =
                        world
                            .resource_mut::<Assets<ShaderMaterial>>()
                            .add(ShaderMaterial {
                                inputs: default(),
                                shader,
                            });
                    world
                        .entity_mut(surface)
                        .insert(MaterialNode(material.clone()));
                    world.get_mut::<Preview>(entity).unwrap().material = Some(material);
                }
                Ok(())
            });
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = result
                    .err()
                    .map_or_else(String::new, |error| format!("WGSL: {error}"));
            }
        }
        let preview = world.get::<Preview>(entity).unwrap();
        let Some(material) = preview.material.clone() else {
            continue;
        };
        let size = world.get::<ComputedNode>(surface).unwrap().size();
        let cursor = world.get::<RelativeCursorPosition>(surface);
        let inside = cursor.is_some_and(RelativeCursorPosition::cursor_over);
        let pointer = cursor
            .and_then(|cursor| cursor.normalized)
            .map_or(Vec2::ZERO, |position| position + Vec2::splat(0.5))
            * size;
        let pressed = inside
            && world
                .get_resource::<ButtonInput<MouseButton>>()
                .is_some_and(|buttons| buttons.pressed(MouseButton::Left));
        let inputs = ShaderInputs {
            size,
            time: preview.started.elapsed().as_secs_f32(),
            delta: world.get_resource::<Time>().map_or(0.0, Time::delta_secs),
            mouse: Vec4::new(
                pointer.x,
                pointer.y,
                u8::from(pressed) as f32,
                u8::from(inside) as f32,
            ),
        };
        if let Some(mut material) = world
            .resource_mut::<Assets<ShaderMaterial>>()
            .get_mut(&material)
        {
            material.inputs = inputs;
        }
    }
    if active && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ball_and_rejects_invalid_or_extra_resources() {
        assert!(compile(BALL).is_ok());
        assert!(compile("fn shade(").is_err());
        assert!(
            compile(&format!(
                "@group(2) @binding(0) var image: texture_2d<f32>;\n{BALL}"
            ))
            .is_err()
        );
        assert!(
            compile(&format!(
                "@compute @workgroup_size(1) fn other() {{}}\n{BALL}"
            ))
            .is_err()
        );
        assert!(compile(&"x".repeat(65_537)).is_err());
        assert!(compile(&format!("override missing: f32;\n{BALL}")).is_err());
        assert!(
            compile("fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> f32 { return 1.0; }")
                .is_err()
        );
    }

    #[test]
    fn edits_reuse_gpu_assets_and_keep_last_valid_shader_on_errors() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Assets<Shader>>();
        world.init_resource::<Assets<ShaderMaterial>>();
        world.init_resource::<crate::theme::Typography>();
        let parent = world.spawn(Node::default()).id();
        spawn(&mut world, parent, BALL, None);
        let entity = world.get::<Children>(parent).unwrap()[0];
        let surface = world.get::<Preview>(entity).unwrap().surface;
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>()
            .insert(InheritedVisibility::VISIBLE);
        world.get_mut::<ComputedNode>(surface).unwrap().size = Vec2::new(400.0, 280.0);
        world
            .get_mut::<RelativeCursorPosition>(surface)
            .unwrap()
            .normalized = Some(Vec2::ZERO);
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        update(&mut world);
        world
            .entity_mut(surface)
            .remove::<ComputedUiRenderTargetInfo>();
        let material = world
            .get::<Preview>(entity)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let shader = world
            .resource::<Assets<ShaderMaterial>>()
            .get(&material)
            .unwrap()
            .shader
            .clone();
        assert_eq!(
            world
                .resource::<Assets<ShaderMaterial>>()
                .get(&material)
                .unwrap()
                .inputs
                .mouse
                .x,
            200.0
        );
        assert_eq!(ShaderInputs::min_size().get(), 32);
        assert!(matches!(
            world
                .resource::<Assets<Shader>>()
                .get(&shader)
                .unwrap()
                .validate_shader,
            ValidateShader::Enabled
        ));
        spawn(&mut world, parent, "fn shade(", Some(entity));
        let label = world.get::<Preview>(entity).unwrap().label;
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        update(&mut world);
        assert!(world.get::<Text>(label).unwrap().0.contains("WGSL:"));
        let bevy::shader::Source::Wgsl(source) = &world
            .resource::<Assets<Shader>>()
            .get(&shader)
            .unwrap()
            .source
        else {
            panic!("WGSL expected")
        };
        assert!(source.contains(BALL));
        let replacement = "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(uv, 0.0, 1.0); }";
        spawn(&mut world, parent, replacement, Some(entity));
        update(&mut world);
        assert!(world.get::<Preview>(entity).unwrap().pending.is_some());
        world.get_mut::<Preview>(entity).unwrap().pending =
            Some(Instant::now() - Duration::from_secs(1));
        world.get_mut::<Node>(parent).unwrap().display = Display::None;
        update(&mut world);
        assert!(world.get::<Preview>(entity).unwrap().pending.is_some());
        world.get_mut::<Node>(parent).unwrap().display = Display::Flex;
        update(&mut world);
        assert!(world.get::<Text>(label).unwrap().0.is_empty());
        assert_eq!(
            world.get::<Preview>(entity).unwrap().material.as_ref(),
            Some(&material)
        );
        assert_eq!(world.resource::<Assets<Shader>>().len(), 1);
        assert_eq!(world.resource::<Assets<ShaderMaterial>>().len(), 1);
    }
}
