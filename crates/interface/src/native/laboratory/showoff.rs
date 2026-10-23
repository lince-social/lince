use crate::{
    host::{DesktopAction, WorkspaceLayout},
    native::{
        actions::Action,
        area::{AreaShape, InfluenceArea, Property, PropertyRule, RecordProperties},
        canvas::{CanvasItem, CanvasView},
        edit_mode::EditAction,
        sand_store::{SandKind, StoredSand, spawn_sand},
        topology::{assets, ui::TopologyAction},
        workspace::{self, WorkspaceMember, Workspaces},
    },
};
use bevy::{math::DVec2, prelude::*};
use std::io::{self, Write};

pub const NAME: &str = "showoff";
pub const MODEL: &[u8] = include_bytes!("../../../fixtures/showoff.glb");

#[derive(Resource)]
pub struct Launch;

#[derive(Component)]
pub struct Controls;

#[derive(Clone, Copy)]
enum Control {
    Canvas,
    Map,
    Shader,
    Scatter,
    Help,
}

#[derive(Component)]
struct Help;

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
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .filter_map(|(root, spaces)| {
            let workspace = spaces.entries.iter().find(|entry| entry.name == NAME)?.id;
            Some((root, workspace))
        })
        .collect();
    for (root, workspace) in roots {
        let exists = world
            .query_filtered::<(&ChildOf, &WorkspaceMember), With<Controls>>()
            .iter(world)
            .any(|(parent, member)| parent.parent() == root && member.0 == workspace);
        if !exists {
            controls(world, root, workspace);
        }
        let visible = world.get::<Workspaces>(root).is_some_and(|spaces| {
            spaces.active == workspace
                && spaces
                    .entries
                    .iter()
                    .any(|entry| entry.id == workspace && entry.desktop.fullscreen().is_none())
        });
        for (parent, member, mut node) in world
            .query_filtered::<(&ChildOf, &WorkspaceMember, &mut Node), With<Controls>>()
            .iter_mut(world)
        {
            if parent.parent() == root && member.0 == workspace {
                let display = if visible {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display != display {
                    node.display = display;
                }
            }
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
        SandKind::Text,
        "Canvas\nDrag background: pan\nRight-drag: move a Sand\nScroll background: zoom\nAlt + E: edit\nSuper + Backspace: unfocus\nSuper + H/J/K/L: focus\nSuper + Shift + H/J/K/L: move\nSuper + V: float\nSuper + F: fullscreen",
        [-650.0, 180.0],
        [340.0, 330.0],
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
    controls(world, root, workspace);
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

fn controls(world: &mut World, root: Entity, workspace: u64) {
    let panel = world
        .spawn((
            Controls,
            crate::native::inspection::InspectionExcluded,
            Name::new("showoff controls"),
            WorkspaceMember(workspace),
            ChildOf(root),
            GlobalZIndex(24),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            Node {
                position_type: PositionType::Absolute,
                top: px(8),
                left: px(8),
                max_width: percent(95),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(6)),
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    let buttons = crate::native::sand_panel::row(world, panel);
    world.get_mut::<Node>(buttons).unwrap().flex_wrap = FlexWrap::Wrap;
    crate::native::description::button(world, buttons, root, "Canvas", Control::Canvas);
    crate::native::description::button(world, buttons, root, "Map", Control::Map);
    crate::native::description::button(world, buttons, root, "WGSL 3D", Control::Shader);
    for (title, action) in [
        ("Tile", DesktopAction::SetLayout(WorkspaceLayout::Grid)),
        (
            "Columns",
            DesktopAction::SetLayout(WorkspaceLayout::ScrollingColumns),
        ),
        ("Unfocus", DesktopAction::Unfocus),
    ] {
        crate::native::description::button(world, buttons, root, title, action);
    }
    crate::native::description::button(world, buttons, root, "Edit", EditAction::Toggle);
    crate::native::information::action_button(
        world,
        buttons,
        root,
        "Areas",
        crate::native::actions![EditAction::Open, EditAction::Areas],
    );
    crate::native::description::button(world, buttons, root, "2D / 3D", TopologyAction::ToggleView);
    crate::native::description::button(world, buttons, root, "Scatter physics", Control::Scatter);
    crate::native::description::button(world, buttons, root, "Help", Control::Help);
    let help = crate::native::edit_mode::label(
        world,
        panel,
        "showoff · saved Laboratory workspace\nDrag background: pan · Scroll background: zoom · Right-drag: move a Sand\nEdit: Alt+E · Unfocus: Super+Backspace · Focus: Super+H/J/K/L · Move: add Shift\n3D: WASD + Q/E · Look: arrows · Log out: Super+Shift+Escape",
        13.0,
    );
    world.entity_mut(help).insert(Help);
}

impl Action for Control {
    fn apply(&self, world: &mut World, root: Entity) {
        if matches!(self, Self::Canvas | Self::Map | Self::Shader) {
            DesktopAction::SetLayout(WorkspaceLayout::Free).apply(world, root);
            DesktopAction::Unfocus.apply(world, root);
            if world
                .get::<crate::native::topology::view::View>(root)
                .is_some_and(|view| view.spatial)
            {
                TopologyAction::ToggleView.apply(world, root);
            }
            let mut view = overview(world, root);
            if matches!(self, Self::Map) {
                let workspace = world.get::<Workspaces>(root).map(|spaces| spaces.active);
                let item = world.query_filtered::<(&CanvasItem, &WorkspaceMember, &ChildOf), With<crate::native::map_castle::MapSand>>()
                    .iter(world)
                    .find(|(_, member, parent)| Some(member.0) == workspace && parent.parent() == root)
                    .map(|(item, _, _)| *item);
                if let Some(item) = item {
                    let size = viewport_size(world, root);
                    let reserved = world.query_filtered::<(&ComputedNode, &ChildOf, &WorkspaceMember), With<Controls>>()
                        .iter(world)
                        .find(|(_, parent, member)| parent.parent() == root && Some(member.0) == workspace)
                        .map_or(120.0, |(node, _, _)| f64::from(node.size().y * node.inverse_scale_factor()) + 16.0);
                    view.zoom = ((size[0] - 32.0) / f64::from(item.size.x))
                        .min((size[1] - reserved - 16.0) / f64::from(item.size.y))
                        .clamp(0.1, 1.0);
                    view.center = item.position - DVec2::new(0.0, reserved * 0.5 / view.zoom);
                }
            }
            if matches!(self, Self::Shader) {
                let workspace = world.get::<Workspaces>(root).map(|spaces| spaces.active);
                let position = world.query_filtered::<(&CanvasItem, &WorkspaceMember, &ChildOf), With<crate::native::shader_effects::Surface>>()
                    .iter(world)
                    .find(|(_, member, parent)| Some(member.0) == workspace && parent.parent() == root)
                    .map(|(item, _, _)| item.position);
                if let Some(position) = position {
                    view.center = position;
                }
            }
            if let Some(mut current) = world.get_mut::<CanvasView>(root) {
                *current = view;
            }
            if matches!(self, Self::Shader) {
                TopologyAction::ToggleView.apply(world, root);
            }
            return;
        }
        if matches!(self, Self::Help) {
            for mut node in world
                .query_filtered::<&mut Node, With<Help>>()
                .iter_mut(world)
            {
                node.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
            }
            return;
        }
        let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
            return;
        };
        let pebbles: Vec<_> = world
            .query::<(Entity, &StoredSand, &WorkspaceMember, &ChildOf)>()
            .iter(world)
            .filter(|(_, _, member, parent)| member.0 == workspace && parent.parent() == root)
            .filter_map(|(entity, sand, _, _)| {
                let content = world.get::<Text>(sand.content?)?;
                let index = ["Physics 1", "Physics 2", "Physics 3"]
                    .iter()
                    .position(|name| content.0 == *name)?;
                Some((entity, index))
            })
            .collect();
        DesktopAction::SetLayout(WorkspaceLayout::Free).apply(world, root);
        DesktopAction::Unfocus.apply(world, root);
        for (entity, index) in pebbles {
            world.get_mut::<CanvasItem>(entity).unwrap().position =
                DVec2::from_array(pebble_positions()[index]);
        }
    }
}
