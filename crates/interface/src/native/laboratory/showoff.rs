use crate::{
    host::{DesktopAction, WorkspaceLayout},
    native::{
        actions::Action,
        area::{AreaShape, InfluenceArea, Property, PropertyRule, RecordProperties},
        canvas::{CanvasItem, CanvasView},
        edit_mode::EditAction,
        sand_store::{SandKind, StoredSand, spawn_sand},
        topology::assets,
        workspace::{self, Workspaces},
    },
};
use bevy::{math::DVec2, prelude::*};
use std::io::{self, Write};

pub const NAME: &str = "showoff";
pub const MODEL: &[u8] = include_bytes!("../../../fixtures/showoff.glb");

#[derive(Resource)]
pub struct Launch;

pub(crate) fn install(app: &mut App) {
    app.add_systems(Update, update.after(crate::native::edit_mode::setup));
}

fn update(world: &mut World) {
    if world.contains_resource::<Launch>() {
        let ready = world
            .query_filtered::<Entity, (
                With<Workspaces>,
                With<CanvasView>,
                With<crate::native::edit_mode::EditMode>,
                Without<super::LaboratoryRoot>,
            )>()
            .iter(world)
            .next()
            .is_some();
        if ready {
            world.remove_resource::<Launch>();
            open(world);
        } else if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

pub fn open(world: &mut World) {
    super::close(world);
    let Some(root) = world
        .query_filtered::<Entity, (With<Workspaces>, Without<super::LaboratoryRoot>)>()
        .iter(world)
        .next()
    else {
        return;
    };
    if let Some(workspace) = world.get::<Workspaces>(root).and_then(|spaces| {
        spaces
            .entries
            .iter()
            .find(|entry| entry.name == NAME)
            .map(|entry| entry.id)
    }) {
        workspace::switch(world, root, workspace);
        return;
    }
    if let Err(error) = populate(world, root) {
        crate::native::notifications::report(world, "Laboratory showoff", &error.to_string());
    }
}

fn populate(world: &mut World, root: Entity) -> io::Result<()> {
    let directory = world
        .get_resource::<assets::AssetDirectory>()
        .ok_or_else(|| io::Error::other("The workspace asset directory is unavailable"))?
        .0
        .clone();
    let mut source = tempfile::Builder::new().suffix(".glb").tempfile()?;
    source.write_all(MODEL)?;
    let mut model = assets::copy_import(source.path(), &directory)?;
    model.name = "Showoff sculpture".into();
    model.scale = 100.0;
    workspace::create(world, root);
    workspace::rename(world, root, NAME);
    let workspace = world.get::<Workspaces>(root).unwrap().active;
    let view = overview(world, root);
    let topology = crate::native::topology::view::View {
        selection_depth: 4000.0,
        ..default()
    };
    world.entity_mut(root).insert(topology);
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let entry = spaces
        .entries
        .iter_mut()
        .find(|entry| entry.id == workspace)
        .unwrap();
    entry.desktop.set_mode(WorkspaceLayout::Free);
    entry.center = view.center.to_array();
    entry.zoom = view.zoom;
    entry.topology = topology;
    *world.get_mut::<CanvasView>(root).unwrap() = view;
    sand(
        world,
        root,
        workspace,
        SandKind::EditableText,
        "showoff\nA real, saved workspace. Edit this note, move it, tile it alongside Firefox, or add any Sand from the editor.",
        [-650.0, -150.0],
        [340.0, 260.0],
    );
    sand(
        world,
        root,
        workspace,
        SandKind::WorkTimer,
        "",
        [640.0, -220.0],
        [360.0, 360.0],
    );
    let shader = sand(
        world,
        root,
        workspace,
        SandKind::Square,
        "",
        [-230.0, -230.0],
        [400.0, 340.0],
    );
    world.entity_mut(shader).insert((
        Name::new("Showoff · animated WGSL mesh"),
        crate::native::topology::Spatial {
            elevation: 180.0,
            ..default()
        },
        crate::native::shader_effects::Surface {
            effect: mesh_effect(),
            strength: 1.0,
            glow: 0.0,
            ..default()
        },
    ));
    crate::native::edit_mode::label(
        world,
        shader,
        "WGSL · generated, animated 3D geometry",
        14.0,
    );
    crate::native::map_castle::spawn(
        world,
        root,
        workspace,
        DVec2::new(-1520.0, 100.0),
        lince_map::Camera {
            center: lince_map::Coordinate {
                latitude: -15.0,
                longitude: -47.0,
            },
            zoom: 1.8,
        },
        Vec::new(),
        String::new(),
    );
    let mut area = InfluenceArea::new(
        AreaShape::Circle,
        DVec2::new(620.0, 350.0),
        DVec2::splat(500.0),
    );
    area.name = "showoff · physics playground".into();
    area.strength = 180.0;
    area.rules.push(PropertyRule {
        property: Property::Kind,
        value: "showoff-physics".into(),
    });
    area.color = [80, 170, 210];
    area.opacity = 0.22;
    crate::native::area::spawn_area(world, root, workspace, area)
        .ok_or_else(|| io::Error::other("The showoff influence area is invalid"))?;
    for (index, position) in pebble_positions().into_iter().enumerate() {
        let pebble = sand(
            world,
            root,
            workspace,
            SandKind::Text,
            &format!("Physics {}", index + 1),
            position,
            [144.0, 96.0],
        );
        world.entity_mut(pebble).insert(RecordProperties(
            serde_json::json!({"kind": "showoff-physics"}),
        ));
        let mut tokens = crate::native::tokens::TokenOverrides::default();
        tokens.set(
            crate::native::tokens::Token::SandBackground,
            crate::native::tokens::TokenValue::Color(
                [
                    [80, 120, 200, 255],
                    [135, 80, 190, 255],
                    [185, 125, 50, 255],
                ][index],
            ),
        );
        tokens.set(
            crate::native::tokens::Token::Roundness,
            crate::native::tokens::TokenValue::Number(12.0),
        );
        world.entity_mut(pebble).insert(tokens);
    }
    let model = assets::spawn(world, root, workspace, DVec2::new(-230.0, 400.0), model);
    world.entity_mut(model).insert(assets::CenteredAsset);
    EditAction::Close.apply(world, root);
    DesktopAction::Unfocus.apply(world, root);
    Ok(())
}

fn mesh_effect() -> nucleus::shader_effect::Effect {
    use nucleus::shader_effect::spatial::{Deformation, GeneratedGeometry, MeshSettings, Mode};
    nucleus::shader_effect::Effect {
        mode: Mode::Mesh(MeshSettings {
            geometry: Some(GeneratedGeometry {
                source: nucleus::shader_effect::spatial::GENERATED_SPHERE.into(),
                vertices: 32 * 20 * 6,
            }),
            deformation: Some(Deformation {
                source: nucleus::shader_effect::spatial::DEFORM_SPHERE.into(),
                max_displacement: 0.24,
            }),
            ..default()
        }),
        programs: std::collections::BTreeMap::from([("showoff".into(), MESH_SHADER.into())]),
        rules: Vec::new(),
        fallback: "showoff".into(),
        ..default()
    }
}

const MESH_SHADER: &str = "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let bands = 0.5 + 0.5 * sin(point.local_position.y * 22.0 + inputs.time * f32(inputs.motion_allowed)); let color = mix(vec3<f32>(0.08, 0.55, 0.9), vec3<f32>(0.7, 0.25, 0.95), bands); let light = 0.35 + 0.65 * max(dot(normalize(point.normal), normalize(vec3<f32>(0.5, 0.8, 1.0))), 0.0); return vec4<f32>(color * light, 1.0); }";

fn pebble_positions() -> [[f64; 2]; 3] {
    [[435.0, 270.0], [800.0, 300.0], [590.0, 475.0]]
}

fn overview(world: &World, root: Entity) -> CanvasView {
    let size = viewport_size(world, root);
    CanvasView {
        center: DVec2::new(0.0, 80.0),
        zoom: (size[0] / 1900.0).min(size[1] / 1100.0).clamp(0.15, 0.65),
    }
}

fn viewport_size(world: &World, root: Entity) -> [f64; 2] {
    world
        .get::<crate::native::layout::desktop::HostViewport>(root)
        .map(|viewport| viewport.0.bounds.size())
        .or_else(|| {
            world.get::<ComputedNode>(root).map(|node| {
                (node.size() * node.inverse_scale_factor())
                    .as_dvec2()
                    .to_array()
            })
        })
        .unwrap_or([1280.0, 720.0])
}

fn sand(
    world: &mut World,
    root: Entity,
    workspace: u64,
    kind: SandKind,
    text: &str,
    position: [f64; 2],
    size: [f32; 2],
) -> Entity {
    let entity = spawn_sand(
        world,
        root,
        workspace,
        kind,
        text,
        DVec2::from_array(position),
    );
    world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::from_array(size);
    if let Some(content) = world
        .get::<StoredSand>(entity)
        .and_then(|sand| sand.content)
        && let Some(mut text) = world.get_mut::<crate::native::sand_text::SandText>(content)
    {
        text.size = size.map(|size| (size - 32.0).max(16.0));
    }
    if let Some(mut layout) = world.get_mut::<crate::native::layout::LayoutBox>(entity) {
        layout.rules = crate::layout::Rules::fixed(size);
    }
    entity
}
