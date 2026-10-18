use super::*;
use bevy::{
    asset::embedded_asset,
    camera::{RenderTarget, visibility::RenderLayers},
    render::render_resource::{AsBindGroup, TextureFormat},
    shader::ShaderRef,
};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct ProjectionMaterial {
    #[uniform(0)]
    inverse_x: Vec4,
    #[uniform(1)]
    inverse_y: Vec4,
    #[uniform(2)]
    inverse_w: Vec4,
    #[texture(3)]
    #[sampler(4)]
    image: Handle<Image>,
}

impl UiMaterial for ProjectionMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://lince_interface/native/physical_workspace/projection.wgsl".into()
    }
}

#[derive(Component)]
#[component(on_remove = release)]
struct Output {
    root: Entity,
    workspace: u64,
    surface: String,
    calibration: u64,
    body: Entity,
    image: Handle<Image>,
    material: Handle<ProjectionMaterial>,
    signature: String,
    pattern: bool,
}

#[derive(Component)]
struct PointerMarker(String);

fn pointers(world: &mut World) {
    let outputs: Vec<_> = world
        .query::<&Output>()
        .iter(world)
        .map(|o| (o.root, o.workspace, o.surface.clone(), o.body))
        .collect();
    for (root, workspace, surface, body) in outputs {
        let points: BTreeMap<_, _> = world
            .get::<Installation>(root)
            .and_then(|i| i.0.get(&workspace))
            .map(|s| {
                s.runtime
                    .pointers
                    .iter()
                    .filter(|(_, p)| p.surface == surface)
                    .map(|(id, p)| (id.clone(), p.local))
                    .collect()
            })
            .unwrap_or_default();
        let markers: Vec<_> = world
            .query::<(Entity, &PointerMarker, &ChildOf)>()
            .iter(world)
            .filter(|(_, _, parent)| parent.parent() == body)
            .map(|(entity, marker, _)| (entity, marker.0.clone()))
            .collect();
        for (entity, id) in &markers {
            if !points.contains_key(id) {
                world.entity_mut(*entity).despawn();
            }
        }
        for (id, point) in points {
            let entity = markers
                .iter()
                .find(|(_, key)| *key == id)
                .map(|(entity, _)| *entity)
                .unwrap_or_else(|| {
                    world
                        .spawn((
                            ChildOf(body),
                            PointerMarker(id),
                            Node::default(),
                            BackgroundColor(Color::srgb(0.1, 1.0, 0.3)),
                            ZIndex(100),
                            bevy::picking::Pickable::IGNORE,
                        ))
                        .id()
                });
            world.entity_mut(entity).insert(Node {
                position_type: PositionType::Absolute,
                left: percent((point[0] * 100.0) as f32),
                top: percent((point[1] * 100.0) as f32),
                width: px(14),
                height: px(14),
                ..default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_output_owns_valid_ui_roots_and_blanks_before_releasing_assets() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        let world = app.world_mut();
        world
            .get_mut::<crate::native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<ProjectionMaterial>>();
        configure(world, root, replay_configuration(1)).unwrap();
        connect(world, root, 1, "preview-projector", "output-session").unwrap();
        let image_count = world.resource::<Assets<Image>>().len();
        open(world, root, 1, "desk").unwrap();
        let (window, output) = world.query::<(Entity, &Output)>().single(world).unwrap();
        let body = output.body;
        assert!(
            world
                .get::<bevy::ui::experimental::GhostNode>(window)
                .is_some()
        );
        let mut roots =
            bevy::ecs::system::SystemState::<bevy::ui::experimental::UiRootNodes>::new(world);
        assert!(
            roots
                .get(world)
                .unwrap()
                .iter()
                .any(|entity| entity == body)
        );
        super::super::blank(world, root);
        assert!(world.get::<Window>(window).is_some());
        assert_eq!(world.get::<Output>(window).unwrap().signature, "blank");
        super::super::close_outputs(world, root);
        assert!(world.get_entity(window).is_err());
        assert_eq!(world.resource::<Assets<Image>>().len(), image_count);
        assert!(world.resource::<Assets<ProjectionMaterial>>().is_empty());
    }

    #[test]
    fn physical_board_projection_uses_live_columns_scopes_rows_and_bounds_cards() {
        let mut app = crate::native::sand_panel::tests::app();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((
            MinimalPlugins,
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::area_mutation::AreaMutationPlugin,
            crate::native::layout::LayoutPlugin,
            crate::native::kanban::KanbanPlugin,
        ));
        let root = app
            .world_mut()
            .spawn((crate::native::container::BoxRoot, Workspaces::default()))
            .id();
        let owner = crate::native::kanban::spawn(app.world_mut(), root, 1, bevy::math::DVec2::ZERO)
            .unwrap();
        let world = app.world_mut();
        let source = board_source(world, owner).unwrap();
        let column = crate::native::kanban::column_entity(world, owner, 0).unwrap();
        let mut first = None;
        for index in 0..10 {
            let uid = nucleus::new_uid("r");
            let entity = world.spawn((ChildOf(root), WorkspaceMember(1), crate::native::protein_area::RecordBinding { area: source, uid: uid.clone(), source: crate::native::protein_area::Source::Local }, crate::native::area::RecordProperties(serde_json::json!({"uid":uid,"head":format!("Task {index}"),"quantity":0,"body":"PRIVATE DESCRIPTION"})), crate::native::layout::LayoutRuntime { parent: Some(column), ..default() })).id();
            first.get_or_insert(entity);
        }
        let foreign = world.spawn_empty().id();
        world.spawn((
            ChildOf(foreign),
            WorkspaceMember(1),
            crate::native::protein_area::RecordBinding {
                area: source,
                uid: nucleus::new_uid("r"),
                source: crate::native::protein_area::Source::Local,
            },
            crate::native::area::RecordProperties(serde_json::json!({"head":"FOREIGN ROOT"})),
            crate::native::layout::LayoutRuntime {
                parent: Some(column),
                ..default()
            },
        ));
        let mut budget = 2;
        let columns = board(world, owner, &mut budget).unwrap();
        assert_eq!(columns[0].total, 10);
        assert_eq!(columns[0].cards.len(), 2);
        assert_eq!(budget, 0);
        assert!(
            columns[0]
                .cards
                .iter()
                .all(|card| !card.caption.contains("PRIVATE DESCRIPTION")
                    && !card.caption.contains("FOREIGN ROOT"))
        );
        let entity = first.unwrap();
        world
            .entity_mut(owner)
            .insert(canvas_host::Identity("projected-board".into()));
        let properties = &world
            .get::<crate::native::area::RecordProperties>(entity)
            .unwrap()
            .0;
        let selection = TaskSelection {
            board: "projected-board".into(),
            record: properties["uid"].as_str().unwrap().into(),
            revision: task_revision(properties).unwrap(),
        };
        assert!(validate_task(world, root, 1, &selection).is_ok());
        world
            .get_mut::<crate::native::area::RecordProperties>(entity)
            .unwrap()
            .0["quantity"] = 1.into();
        assert!(validate_task(world, root, 1, &selection).is_err());
    }
}

fn release(mut world: bevy::ecs::world::DeferredWorld, context: bevy::ecs::lifecycle::HookContext) {
    let Some(output) = world.get::<Output>(context.entity) else {
        return;
    };
    let image = output.image.id();
    let material = output.material.id();
    if let Some(mut assets) = world.get_resource_mut::<Assets<Image>>() {
        assets.remove(image);
    }
    if let Some(mut assets) = world.get_resource_mut::<Assets<ProjectionMaterial>>() {
        assets.remove(material);
    }
}

pub(super) fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        embedded_asset!(app, "projection.wgsl");
        app.add_plugins(UiMaterialPlugin::<ProjectionMaterial>::default());
    }
}

pub fn open(world: &mut World, root: Entity, workspace: u64, id: &str) -> Result<(), String> {
    permitted(world, root, workspace)?;
    let session = world
        .get::<Installation>(root)
        .and_then(|i| i.0.get(&workspace))
        .ok_or("Configure this workspace first.")?;
    let surface = session.runtime.surface(id, workspace)?.clone();
    let projector = session
        .runtime
        .configuration
        .devices
        .iter()
        .find(|d| d.id == surface.projector)
        .unwrap()
        .clone();
    if !session.runtime.connections.contains_key(&projector.id) {
        return Err("Connect the chosen output device first.".into());
    }
    if !world.contains_resource::<Assets<ProjectionMaterial>>()
        || !world.contains_resource::<Assets<Image>>()
    {
        return Err("Surface output requires the native renderer.".into());
    }
    let existing = world
        .query::<(Entity, &Output)>()
        .iter(world)
        .find(|(_, o)| o.root == root && o.workspace == workspace && o.surface == id)
        .map(|(entity, _)| entity);
    if let Some(entity) = existing {
        if world.get::<Output>(entity).unwrap().calibration == surface.calibration_revision {
            let mut output = world.get_mut::<Output>(entity).unwrap();
            output.pattern = false;
            output.signature.clear();
            world
                .get_mut::<Installation>(root)
                .unwrap()
                .0
                .get_mut(&workspace)
                .unwrap()
                .runtime
                .blank(false);
            return Ok(());
        }
        remove(world, entity);
    }
    if world.query::<&Output>().iter(world).count() >= 4 {
        return Err("At most four surface output windows can be active.".into());
    }
    let pixels: u64 = world
        .query::<(&Output, &Window)>()
        .iter(world)
        .map(|(_, window)| u64::from(window.physical_width()) * u64::from(window.physical_height()))
        .sum();
    if pixels + u64::from(projector.resolution[0]) * u64::from(projector.resolution[1]) > 16_777_216
    {
        return Err("Active surface outputs share a sixteen-megapixel window budget.".into());
    }
    let inverse = surface
        .projector_quad
        .matrix()?
        .inverse()
        .to_cols_array_2d();
    let row = |i| {
        Vec4::new(
            inverse[0][i] as f32,
            inverse[1][i] as f32,
            inverse[2][i] as f32,
            0.0,
        )
    };
    let longest = surface.size_meters[0].max(surface.size_meters[1]);
    let dimension = |meters: f64| (1024.0 * meters / longest).round().clamp(64.0, 1024.0) as u32;
    let width = dimension(surface.size_meters[0]);
    let height = dimension(surface.size_meters[1]);
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            width,
            height,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let material = world
        .resource_mut::<Assets<ProjectionMaterial>>()
        .add(ProjectionMaterial {
            inverse_x: row(0),
            inverse_y: row(1),
            inverse_w: row(2),
            image: image.clone(),
        });
    let window = world
        .spawn((
            Window {
                title: format!("Lince surface — {}", surface.name),
                resolution: (projector.resolution[0], projector.resolution[1]).into(),
                resizable: false,
                ..default()
            },
            bevy::ui::experimental::GhostNode,
        ))
        .id();
    let content_camera = world
        .spawn((
            ChildOf(window),
            Camera2d,
            Camera {
                order: -10,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            RenderLayers::layer(29),
            Msaa::Off,
        ))
        .id();
    let body = world
        .spawn((
            ChildOf(window),
            UiTargetCamera(content_camera),
            Node {
                width: px(width),
                height: px(height),
                ..default()
            },
            BackgroundColor(Color::BLACK),
        ))
        .id();
    let camera = world
        .spawn((
            ChildOf(window),
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
            RenderLayers::layer(28),
            Msaa::Off,
        ))
        .id();
    world.spawn((
        ChildOf(window),
        UiTargetCamera(camera),
        MaterialNode(material.clone()),
        Node {
            width: percent(100),
            height: percent(100),
            ..default()
        },
        bevy::picking::Pickable::IGNORE,
    ));
    world.entity_mut(window).insert(Output {
        root,
        workspace,
        surface: id.into(),
        calibration: surface.calibration_revision,
        body,
        image,
        material,
        signature: String::new(),
        pattern: false,
    });
    world
        .get_mut::<Installation>(root)
        .unwrap()
        .0
        .get_mut(&workspace)
        .unwrap()
        .runtime
        .blank(false);
    Ok(())
}

pub(super) fn calibration(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
) -> Result<(), String> {
    open(world, root, workspace, surface)?;
    let entity = world
        .query::<(Entity, &Output)>()
        .iter(world)
        .find(|(_, o)| o.root == root && o.workspace == workspace && o.surface == surface)
        .map(|(entity, _)| entity)
        .ok_or("Output is unavailable.")?;
    let mut output = world.get_mut::<Output>(entity).unwrap();
    output.pattern = true;
    output.signature.clear();
    drop(output);
    let session = world
        .get_mut::<Installation>(root)
        .unwrap()
        .into_inner()
        .0
        .get_mut(&workspace)
        .unwrap();
    session.runtime.pointers.clear();
    session.drags.clear();
    Ok(())
}

pub(super) fn calibrating(world: &mut World, root: Entity, workspace: u64, surface: &str) -> bool {
    world
        .query::<&Output>()
        .iter(world)
        .any(|o| o.root == root && o.workspace == workspace && o.surface == surface && o.pattern)
}

fn remove(world: &mut World, entity: Entity) {
    if let Ok(entity) = world.get_entity_mut(entity) {
        entity.despawn();
    }
}

pub(super) fn close(world: &mut World, root: Entity) {
    let outputs: Vec<_> = world
        .query::<(Entity, &Output)>()
        .iter(world)
        .filter(|(_, o)| o.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for output in outputs {
        remove(world, output);
    }
}

fn black_output(world: &mut World, entity: Entity) {
    let output = world.get::<Output>(entity).unwrap();
    if output.signature == "blank" {
        return;
    }
    let body = output.body;
    sand_panel::clear(world, body);
    world.get_mut::<Output>(entity).unwrap().signature = "blank".into();
}

pub(super) fn black(world: &mut World, root: Entity) {
    let outputs: Vec<_> = world
        .query::<(Entity, &Output)>()
        .iter(world)
        .filter(|(_, output)| output.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for entity in outputs {
        black_output(world, entity);
    }
}

fn label(world: &World, entity: Entity, id: &str) -> String {
    if let Some(properties) = world.get::<crate::native::area::RecordProperties>(entity) {
        let title = properties.0["head"].as_str().unwrap_or("Record");
        let quantity = match &properties.0["quantity"] {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Number(value) => value.to_string(),
            _ => "unavailable".into(),
        };
        format!(
            "{}\nQuantity: {}",
            title.chars().take(256).collect::<String>(),
            quantity.chars().take(128).collect::<String>()
        )
    } else {
        let texts: Vec<_> = crate::native::sand_text::snapshot(world, entity)
            .into_iter()
            .take(3)
            .map(|text| text.text.chars().take(1024).collect::<String>())
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
        world
            .get::<Name>(entity)
            .map(|n| n.as_str())
            .unwrap_or(id)
            .chars()
            .take(256)
            .collect()
    }
}

#[derive(Debug)]
struct BoardCard {
    caption: String,
    image: Option<Handle<Image>>,
    record: String,
    revision: String,
    entity: Entity,
}

#[derive(Debug)]
struct BoardColumn {
    name: String,
    total: usize,
    cards: Vec<BoardCard>,
}

#[derive(Clone)]
pub(super) struct TaskSelection {
    pub board: String,
    pub record: String,
    revision: String,
}

#[derive(Component)]
struct TaskHit {
    root: Entity,
    workspace: u64,
    surface: String,
    selection: TaskSelection,
}

#[derive(Component)]
struct ColumnHit {
    root: Entity,
    workspace: u64,
    surface: String,
    board: String,
    column: usize,
}

fn task_revision(value: &serde_json::Value) -> Result<String, String> {
    model::revision(
        &serde_json::json!({"uid":value["uid"], "head":value["head"], "quantity":value["quantity"], "revision":value["revision"]}),
    )
}

fn contains(world: &World, entity: Entity, local: [f64; 2]) -> bool {
    let (Some(node), Some(transform), Some(target)) = (
        world.get::<ComputedNode>(entity),
        world.get::<UiGlobalTransform>(entity),
        world.get::<ComputedUiRenderTargetInfo>(entity),
    ) else {
        return false;
    };
    let pixel = Vec2::new(local[0] as f32, local[1] as f32) * target.physical_size().as_vec2();
    let point = transform.inverse().transform_point2(pixel);
    point.is_finite()
        && node.size().min_element() > 0.0
        && (point.abs().cmple(node.size() * 0.5)).all()
        && world
            .get::<CalculatedClip>(entity)
            .is_none_or(|clip| clip.clip.contains(pixel))
}

pub(super) fn pick_task(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
    local: [f64; 2],
) -> Option<TaskSelection> {
    world
        .query::<(Entity, &TaskHit)>()
        .iter(world)
        .find(|(entity, hit)| {
            hit.root == root
                && hit.workspace == workspace
                && hit.surface == surface
                && contains(world, *entity, local)
        })
        .map(|(_, hit)| hit.selection.clone())
}

pub(super) fn pick_column(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
    board: &str,
    local: [f64; 2],
) -> Option<usize> {
    world
        .query::<(Entity, &ColumnHit)>()
        .iter(world)
        .find(|(entity, hit)| {
            hit.root == root
                && hit.workspace == workspace
                && hit.surface == surface
                && hit.board == board
                && contains(world, *entity, local)
        })
        .map(|(_, hit)| hit.column)
}

fn board_source(world: &mut World, owner: Entity) -> Option<Entity> {
    let source = world
        .get::<crate::native::kanban::Kanban>(owner)?
        .source
        .clone();
    let root = world.get::<ChildOf>(owner)?.parent();
    let workspace = world.get::<WorkspaceMember>(owner)?.0;
    world
        .query::<(
            Entity,
            &crate::native::area::InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
        )>()
        .iter(world)
        .find(|(_, area, parent, member)| {
            area.id == source && parent.parent() == root && member.0 == workspace
        })
        .map(|(entity, _, _, _)| entity)
}

pub(super) fn validate_task(
    world: &mut World,
    root: Entity,
    workspace: u64,
    selection: &TaskSelection,
) -> Result<(), String> {
    let owner =
        resolve(world, root, workspace, &selection.board).ok_or("The board disappeared.")?;
    let source = board_source(world, owner).ok_or("The board source is unavailable.")?;
    let current = world
        .query::<(
            &crate::native::protein_area::RecordBinding,
            &crate::native::area::RecordProperties,
            &ChildOf,
            &WorkspaceMember,
        )>()
        .iter(world)
        .find(|(binding, _, parent, member)| {
            binding.area == source
                && binding.uid == selection.record
                && parent.parent() == root
                && member.0 == workspace
                && binding.source == crate::native::protein_area::Source::Local
        })
        .and_then(|(_, properties, _, _)| task_revision(&properties.0).ok());
    if current.as_deref() != Some(&selection.revision) {
        return Err("The task changed during selection; select again.".into());
    }
    Ok(())
}

fn board(world: &mut World, owner: Entity, budget: &mut usize) -> Option<Vec<BoardColumn>> {
    let count = world
        .get::<crate::native::kanban::Kanban>(owner)?
        .columns
        .len();
    let parent = world.get::<ChildOf>(owner)?.parent();
    let workspace = world.get::<WorkspaceMember>(owner)?.0;
    let source = board_source(world, owner)?;
    let mut columns = Vec::new();
    for index in 0..count.min(7) {
        let column = crate::native::kanban::column_entity(world, owner, index)?;
        let mut rows: Vec<_> = world
            .query::<(
                Entity,
                &crate::native::area::RecordProperties,
                &crate::native::layout::LayoutRuntime,
                &ChildOf,
                &WorkspaceMember,
                &crate::native::protein_area::RecordBinding,
            )>()
            .iter(world)
            .filter(|(_, _, layout, row_parent, member, binding)| {
                layout.parent == Some(column)
                    && row_parent.parent() == parent
                    && member.0 == workspace
                    && binding.area == source
            })
            .filter_map(|(entity, properties, _, _, _, binding)| {
                task_revision(&properties.0)
                    .ok()
                    .map(|revision| (entity, binding.uid.clone(), revision))
            })
            .collect();
        rows.sort_by(|a, b| a.1.cmp(&b.1));
        let total = rows.len();
        let cards: Vec<_> = rows
            .into_iter()
            .take((*budget).min(8))
            .map(|(entity, uid, revision)| BoardCard {
                caption: label(world, entity, &uid),
                image: crate::native::shader_effects::projection_image(world, entity),
                record: uid,
                revision,
                entity,
            })
            .collect();
        *budget -= cards.len();
        columns.push(BoardColumn {
            name: lince_interface::records::KANBAN_COLUMNS[index].0.into(),
            total,
            cards,
        });
    }
    Some(columns)
}

fn render_board(
    world: &mut World,
    parent: Entity,
    root: Entity,
    workspace: u64,
    surface: &str,
    board: &str,
    columns: Vec<BoardColumn>,
) {
    if let Some(mut node) = world.get_mut::<Node>(parent) {
        node.column_gap = px(4);
    }
    let count = columns.len().max(1);
    for (index, column) in columns.into_iter().enumerate() {
        let owner = world
            .spawn((
                ChildOf(parent),
                Node {
                    width: percent(100.0 / count as f32),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.04, 0.06, 0.1)),
            ))
            .id();
        world.entity_mut(owner).insert(ColumnHit {
            root,
            workspace,
            surface: surface.into(),
            board: board.into(),
            column: index,
        });
        world.spawn((
            ChildOf(owner),
            Text::new(format!("{} ({})", column.name, column.total)),
            world
                .resource::<crate::native::theme::Typography>()
                .text(18.0),
            TextColor(Color::WHITE),
        ));
        let visible = column.cards.len();
        for card_content in column.cards {
            let card = world
                .spawn((
                    ChildOf(owner),
                    Node {
                        width: percent(100),
                        min_height: px(64),
                        max_height: px(96),
                        flex_shrink: 0.0,
                        overflow: Overflow::clip(),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.08, 0.1, 0.16)),
                ))
                .id();
            world.entity_mut(card).insert(TaskHit {
                root,
                workspace,
                surface: surface.into(),
                selection: TaskSelection {
                    board: board.into(),
                    record: card_content.record,
                    revision: card_content.revision,
                },
            });
            if let Some(image) = card_content.image {
                world.spawn((
                    ChildOf(card),
                    ImageNode::new(image),
                    Node {
                        width: px(32),
                        height: px(32),
                        ..default()
                    },
                ));
            }
            world.spawn((
                ChildOf(card),
                Text::new(card_content.caption),
                world
                    .resource::<crate::native::theme::Typography>()
                    .text(16.0),
                TextColor(Color::WHITE),
            ));
        }
        if column.total > visible {
            world.spawn((
                ChildOf(owner),
                Text::new(format!("{} more — open Lince", column.total - visible)),
                world
                    .resource::<crate::native::theme::Typography>()
                    .text(16.0),
                TextColor(Color::WHITE),
            ));
        }
    }
}

pub(super) fn tick(world: &mut World) {
    let mut demanded = std::collections::BTreeSet::new();
    let outputs: Vec<_> = world
        .query::<(Entity, &Output)>()
        .iter(world)
        .map(|(entity, o)| {
            (
                entity,
                o.root,
                o.workspace,
                o.surface.clone(),
                o.calibration,
                o.body,
                o.signature.clone(),
            )
        })
        .collect();
    for (entity, root, workspace, surface, revision, body, previous) in outputs {
        let session = world
            .get::<Installation>(root)
            .and_then(|i| i.0.get(&workspace));
        let allowed = session.is_some_and(|s| {
            !s.runtime.blanked
                && s.runtime.surface(&surface, workspace).is_ok_and(|p| {
                    p.calibration_revision == revision
                        && s.runtime.connections.contains_key(&p.projector)
                })
        }) && active_scope(world, root, workspace)
            && world
                .get::<Workspaces>(root)
                .is_some_and(|s| s.active == workspace)
            && !crate::native::laboratory::suspended(world, root);
        if !allowed {
            black_output(world, entity);
            continue;
        }
        if world.get::<Output>(entity).unwrap().pattern {
            if previous != "calibration" {
                sand_panel::clear(world, body);
                for (x, y, caption) in [
                    (0.02, 0.02, "TOP LEFT"),
                    (0.72, 0.02, "TOP RIGHT"),
                    (0.72, 0.88, "BOTTOM RIGHT"),
                    (0.02, 0.88, "BOTTOM LEFT"),
                    (0.4, 0.45, "+ CENTER"),
                ] {
                    world.spawn((
                        ChildOf(body),
                        Node {
                            position_type: PositionType::Absolute,
                            left: percent(x * 100.0),
                            top: percent(y * 100.0),
                            ..default()
                        },
                        Text::new(caption),
                        world
                            .resource::<crate::native::theme::Typography>()
                            .text(24.0),
                        TextColor(Color::WHITE),
                    ));
                }
                world.get_mut::<Output>(entity).unwrap().signature = "calibration".into();
            }
            continue;
        }
        let bindings: Vec<_> = session
            .unwrap()
            .bindings
            .iter()
            .filter(|b| b.surface == surface)
            .cloned()
            .collect();
        let mut entries = Vec::new();
        let mut card_budget = 64;
        for binding in bindings {
            if let Some(owner) = resolve(world, root, workspace, &binding.placement) {
                let Some(point) = coordinates(world, root, workspace, &binding.placement)
                    .filter(|point| point.room == binding.room)
                else {
                    continue;
                };
                let mut binding = binding;
                demanded.insert(owner);
                binding.rectangle =
                    binding.rectangle_at([point.local.position[0], point.local.position[2]]);
                let mut caption = label(world, owner, &binding.placement);
                if let Ok((target, _)) =
                    inference::target(world, root, workspace, &binding.placement)
                {
                    let answer = world
                        .get::<Installation>(root)
                        .and_then(|i| i.0.get(&workspace))
                        .and_then(|session| {
                            session
                                .runtime
                                .requests
                                .values()
                                .filter(|r| r.surface == surface && r.target == target)
                                .max_by_key(|r| r.updated_ms)
                        })
                        .map(|r| match r.status {
                            model::Status::Completed => r.result.clone().unwrap_or_default(),
                            model::Status::Queued => "Request queued".into(),
                            model::Status::Running => "Fiote is working".into(),
                            model::Status::Waiting => "Waiting for input in Lince".into(),
                            model::Status::Canceled => "Request canceled".into(),
                            model::Status::Failed => "Request needs attention in Lince".into(),
                        });
                    if let Some(answer) = answer {
                        caption.push_str("\n\n");
                        caption.push_str(&answer);
                    }
                }
                let image = crate::native::shader_effects::projection_image(world, owner);
                let board = board(world, owner, &mut card_budget);
                if let Some(columns) = &board {
                    for card in columns.iter().flat_map(|column| &column.cards) {
                        demanded.insert(card.entity);
                    }
                }
                entries.push((binding, caption, image, board));
            }
        }
        let signature = format!("{entries:?}");
        if previous == signature {
            continue;
        }
        sand_panel::clear(world, body);
        if entries.is_empty() {
            world.spawn((
                ChildOf(body),
                Text::new("Assign placements from the computer to display them here."),
                world
                    .resource::<crate::native::theme::Typography>()
                    .text(24.0),
                TextColor(Color::WHITE),
            ));
        }
        for (binding, caption, image, board) in entries {
            let [x, y, w, h] = binding.rectangle;
            let node = world
                .spawn((
                    ChildOf(body),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent((x * 100.0) as f32),
                        top: percent((y * 100.0) as f32),
                        width: percent((w * 100.0) as f32),
                        height: percent((h * 100.0) as f32),
                        padding: UiRect::all(px(10)),
                        overflow: Overflow::clip(),
                        column_gap: px(12),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.025, 0.04, 0.07)),
                ))
                .id();
            if let Some(board) = board {
                render_board(
                    world,
                    node,
                    root,
                    workspace,
                    &surface,
                    &binding.placement,
                    board,
                );
                continue;
            }
            if let Some(image) = image {
                world.spawn((
                    ChildOf(node),
                    ImageNode::new(image),
                    Node {
                        width: px(90),
                        height: px(90),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
            }
            world.spawn((
                ChildOf(node),
                Text::new(caption),
                world
                    .resource::<crate::native::theme::Typography>()
                    .text(28.0),
                TextColor(Color::WHITE),
            ));
        }
        world.get_mut::<Output>(entity).unwrap().signature = signature;
    }
    pointers(world);
    let old: Vec<_> = world
        .query_filtered::<Entity, With<crate::native::shader_effects::ProjectionDemand>>()
        .iter(world)
        .collect();
    for entity in old {
        if !demanded.contains(&entity) {
            world
                .entity_mut(entity)
                .remove::<crate::native::shader_effects::ProjectionDemand>();
        }
    }
    for entity in demanded {
        if world
            .get::<crate::native::shader_effects::ProjectionDemand>(entity)
            .is_none()
        {
            world
                .entity_mut(entity)
                .insert(crate::native::shader_effects::ProjectionDemand);
        }
    }
}
