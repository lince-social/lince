use crate::{
    host::{DesktopAction, WorkspaceLayout},
    native::{
        actions::Action,
        area::{AreaShape, InfluenceArea},
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
    Scatter,
    Help,
}

#[derive(Component)]
struct Help;

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        Update,
        update
            .after(workspace::PrepareWorkspaces)
            .before(crate::native::layout::ResolveLayout),
    );
}

fn update(world: &mut World) {
    if world.contains_resource::<Launch>() {
        let ready = world
            .query_filtered::<Entity, (
                With<Workspaces>,
                With<CanvasView>,
                Without<super::LaboratoryRoot>,
            )>()
            .iter(world)
            .next()
            .is_some();
        if ready {
            world.remove_resource::<Launch>();
            open(world);
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
        spaces.entries.iter().find(|entry| entry.name == NAME).map(|entry| entry.id)
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
        .0.clone();
    let mut source = tempfile::Builder::new().suffix(".glb").tempfile()?;
    source.write_all(MODEL)?;
    let mut model = assets::copy_import(source.path(), &directory)?;
    model.name = "Showoff sculpture".into();
    model.scale = 100.0;
    workspace::create(world, root);
    workspace::rename(world, root, NAME);
    let workspace = world.get::<Workspaces>(root).unwrap().active;
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let entry = spaces.entries.iter_mut().find(|entry| entry.id == workspace).unwrap();
    entry.desktop.set_mode(WorkspaceLayout::Free);
    entry.center = [0.0, 80.0];
    entry.zoom = 0.65;
    *world.get_mut::<CanvasView>(root).unwrap() = CanvasView {
        center: DVec2::new(0.0, 80.0),
        zoom: 0.65,
    };
    sand(world, root, workspace, SandKind::EditableText,
        "showoff\nA real, saved workspace. Edit this note, move it, tile it alongside Firefox, or add any Sand from the editor.",
        [-650.0, -150.0], [340.0, 260.0]);
    sand(world, root, workspace, SandKind::Text,
        "Canvas\nSpace + drag: pan\nScroll: zoom\nAlt + E: edit\nSuper + Backspace: unfocus\nSuper + H/J/K/L: focus\nSuper + Shift + H/J/K/L: move\nSuper + V: float\nSuper + F: fullscreen\nSuper + Shift + Escape: log out",
        [-650.0, 180.0], [340.0, 330.0]);
    sand(world, root, workspace, SandKind::WorkTimer, "", [640.0, -220.0], [360.0, 360.0]);
    let mut area = InfluenceArea::new(
        AreaShape::Ellipse,
        DVec2::new(620.0, 350.0),
        DVec2::new(500.0, 320.0),
    );
    area.name = "showoff · physics playground".into();
    area.strength = 180.0;
    area.color = [80, 170, 210];
    area.opacity = 0.22;
    crate::native::area::spawn_area(world, root, workspace, area);
    for (index, position) in pebble_positions().into_iter().enumerate() {
        sand(world, root, workspace, SandKind::Text,
            &format!("Physics {}", index + 1), position, [96.0, 72.0]);
    }
    let model = assets::spawn(world, root, workspace, DVec2::new(-230.0, 400.0), model);
    world.entity_mut(model).insert(assets::CenteredAsset);
    EditAction::Close.apply(world, root);
    DesktopAction::Unfocus.apply(world, root);
    controls(world, root, workspace);
    Ok(())
}

fn pebble_positions() -> [[f64; 2]; 3] {
    [[435.0, 270.0], [800.0, 300.0], [590.0, 475.0]]
}

fn sand(world: &mut World, root: Entity, workspace: u64, kind: SandKind, text: &str,
    position: [f64; 2], size: [f32; 2]) -> Entity {
    let entity = spawn_sand(world, root, workspace, kind, text, DVec2::from_array(position));
    world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::from_array(size);
    if let Some(mut layout) = world.get_mut::<crate::native::layout::LayoutBox>(entity) {
        layout.rules = crate::layout::Rules::fixed(size);
    }
    entity
}

fn controls(world: &mut World, root: Entity, workspace: u64) {
    let panel = world.spawn((
        Controls,
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
    )).id();
    let buttons = crate::native::sand_panel::row(world, panel);
    world.get_mut::<Node>(buttons).unwrap().flex_wrap = FlexWrap::Wrap;
    for (title, action) in [
        ("Canvas", DesktopAction::SetLayout(WorkspaceLayout::Free)),
        ("Tile", DesktopAction::SetLayout(WorkspaceLayout::Grid)),
        ("Columns", DesktopAction::SetLayout(WorkspaceLayout::ScrollingColumns)),
        ("Unfocus", DesktopAction::Unfocus),
    ] {
        crate::native::description::button(world, buttons, root, title, action);
    }
    crate::native::description::button(world, buttons, root, "Edit", EditAction::Toggle);
    crate::native::description::button(world, buttons, root, "Areas", EditAction::Areas);
    crate::native::description::button(world, buttons, root, "2D / 3D", TopologyAction::ToggleView);
    crate::native::description::button(world, buttons, root, "Scatter physics", Control::Scatter);
    crate::native::description::button(world, buttons, root, "Help", Control::Help);
    let help = crate::native::edit_mode::label(world, panel,
        "showoff · saved Laboratory workspace\nPan: Space + drag · Zoom: scroll · Edit: Alt+E · Unfocus: Super+Backspace\nFocus: Super+H/J/K/L · Move: Super+Shift+H/J/K/L · Float: Super+V\n3D: WASD + Q/E · Look: arrows · Log out: Super+Shift+Escape", 13.0);
    world.entity_mut(help).insert(Help);
}

impl Action for Control {
    fn apply(&self, world: &mut World, root: Entity) {
        if matches!(self, Self::Help) {
            for mut node in world.query_filtered::<&mut Node, With<Help>>().iter_mut(world) {
                node.display = if node.display == Display::None { Display::Flex } else { Display::None };
            }
            return;
        }
        let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else { return };
        let pebbles: Vec<_> = world.query::<(Entity, &StoredSand, &WorkspaceMember, &ChildOf)>()
            .iter(world)
            .filter(|(_, _, member, parent)| member.0 == workspace && parent.parent() == root)
            .filter_map(|(entity, sand, _, _)| {
                let content = world.get::<Text>(sand.content?)?;
                let index = ["Physics 1", "Physics 2", "Physics 3"].iter().position(|name| content.0 == *name)?;
                Some((entity, index))
            }).collect();
        DesktopAction::SetLayout(WorkspaceLayout::Free).apply(world, root);
        DesktopAction::Unfocus.apply(world, root);
        for (entity, index) in pebbles {
            world.get_mut::<CanvasItem>(entity).unwrap().position = DVec2::from_array(pebble_positions()[index]);
        }
    }
}
