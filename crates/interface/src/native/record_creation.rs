// non-lean-yet
// Scope: mod search, struct CreateRecord, struct Form, struct Frozen.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 176, 358, 403,
//   410, 411, 412, 569, 597, 646.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 572.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (233).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   375.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (101, 136, 243, 244, 250, 266, 305, 450, 479, 503, 577, 631, 661).

use bevy::{prelude::*, text::EditableText};
use engine::record_creation::Draft;
use serde_json::json;

use crate::native::{
    actions::{Action, ActionButton},
    protein_area::Source,
};

mod draft;
mod search;
use draft::{collect, editors, value};

#[derive(EntityEvent)]
pub struct CreateRecord {
    pub entity: Entity,
}

/// What the find-or-create form searches and creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Any Record, created from a full Record draft.
    Record,
    /// Command Records only, created as a Command with a title, slug and shell script.
    Command,
}

#[derive(Component)]
struct Form {
    area: Entity,
    mode: Mode,
    original: Option<crate::native::protein_area::Config>,
    draft: Draft,
    fields: Vec<(&'static str, Entity)>,
    relations: Vec<(Entity, bool, Vec<Entity>)>,
    logs: Vec<(Entity, Entity, Entity)>,
    additions: Entity,
    assertions: Entity,
    status: Entity,
    pending: Option<String>,
}

#[derive(Component)]
struct Frozen;

pub struct RecordCreationPlugin;

impl Plugin for RecordCreationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<crate::native::assertion_editor::AssertionEditorPlugin>() {
            app.add_plugins(crate::native::assertion_editor::AssertionEditorPlugin);
        }
        app.init_resource::<search::Searches>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_observer(|event: On<CreateRecord>, mut commands: Commands| {
                let root = event.entity;
                commands.queue(move |world: &mut World| {
                    open(world, root);
                });
            })
            .add_systems(
                Update,
                receive.after(crate::native::cell_bridge::ReceiveCell),
            )
            .add_systems(
                PostUpdate,
                (gate, search::keys).before(bevy::text::EditableTextSystems),
            )
            .add_systems(PostUpdate, fit.after(bevy::ui::UiSystems::PostLayout))
            .add_systems(
                PostUpdate,
                search::update
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions),
            );
    }
}

fn fit(
    world: &mut World,
    forms: &mut QueryState<(Entity, &'static Form)>,
    mut targets: Local<Vec<(Entity, Entity)>>,
) {
    let _fit = nucleus::performance::Span::new("record-creation-fit");
    targets.clear();
    targets.extend(forms.iter(world).map(|(entity, form)| (entity, form.area)));
    nucleus::performance::add("record-creation-fit-targets", targets.len() as u64);
    if targets.is_empty() {
        targets.shrink_to_fit();
    }
    for (entity, area) in targets.drain(..) {
        if world
            .get::<crate::native::layout::LayoutBox>(entity)
            .is_none()
        {
            let (count, content) = world
                .get::<Children>(entity)
                .into_iter()
                .flatten()
                .filter_map(|child| world.get::<ComputedNode>(*child))
                .fold((0usize, 0.0f32), |(count, height), node| {
                    (
                        count + 1,
                        height
                            + node.size().y.max(node.content_size.y) * node.inverse_scale_factor(),
                    )
                });
            let height = 24.0 + count.saturating_sub(1) as f32 * 8.0 + content;
            if height > 48.0 {
                let mut item = world
                    .get_mut::<crate::native::canvas::CanvasItem>(entity)
                    .unwrap();
                let height = height.clamp(120.0, 1000.0);
                if item.size.y != height {
                    item.size.y = height;
                    nucleus::performance::add("record-creation-size-publications", 1);
                }
            }
        }
        crate::native::full_record::fit_source(world, area, entity);
    }
}

fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn input(world: &mut World, parent: Entity, title: &str, value: &str, multiline: bool) -> Entity {
    let caption = crate::native::edit_mode::label(world, parent, title, 14.0);
    wrap(world, caption);
    let bundle = crate::native::sand::text_editor(
        value,
        world.resource::<crate::native::theme::Typography>(),
        0,
    );
    let entity = world.spawn(bundle).id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.max_characters = Some(if multiline { 65536 } else { 4096 });
    text.allow_newlines = multiline;
    text.visible_lines = Some(if multiline { 4.0 } else { 1.0 });
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_height: px(if multiline { 100 } else { 32 }),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(parent),
    ));
    entity
}

/// Lets a label shrink with its container and wrap long words instead of overflowing it.
pub(crate) fn wrap(world: &mut World, label: Entity) {
    if let Some(mut node) = world.get_mut::<Node>(label) {
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.max_width = percent(100);
    }
    world
        .entity_mut(label)
        .insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
}

fn button(world: &mut World, parent: Entity, owner: Entity, title: &str, action: impl Action) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            ActionButton::new(owner, crate::native::actions![action]),
            Node {
                min_height: px(32),
                flex_shrink: 0.0,
                padding: UiRect::all(px(6)),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::native::edit_mode::label(world, entity, title, 14.0);
}

pub fn open(world: &mut World, target: Entity) -> Option<Entity> {
    open_mode(world, target, Mode::Record)
}

/// Opens the find-or-create form for Commands. `target` is either the canvas root, which
/// places a new Command Castle, or an existing Command Castle's Area, which retargets it.
/// Nothing is created until Create is pressed, so placing a Command Castle can reuse a saved
/// Command instead of adding another one.
pub fn open_command(world: &mut World, target: Entity) -> Option<Entity> {
    open_mode(world, target, Mode::Command)
}

fn open_mode(world: &mut World, target: Entity, mode: Mode) -> Option<Entity> {
    if crate::native::laboratory::blocked(world, target)
        || !world.contains_resource::<crate::native::theme::Typography>()
    {
        return None;
    }
    let existing = world.get::<crate::native::area::InfluenceArea>(target);
    let original = existing.and_then(|area| area.protein.clone());
    let root = if existing.is_some() {
        world.get::<ChildOf>(target)?.parent()
    } else {
        target
    };
    let area = if original.is_some() {
        target
    } else {
        crate::native::full_record::open(world, root, "pending", Source::Local)?
    };
    if original.is_none() && mode == Mode::Command {
        let mut influence = world.get_mut::<crate::native::area::InfluenceArea>(area)?;
        influence.name = "Command Castle".into();
        let config = influence.protein.as_mut()?;
        config.command = Some(Default::default());
        config.width = 820.0;
    }
    if let Some(entity) = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .find(|(_, form)| form.area == area)
        .map(|(entity, _)| entity)
    {
        return Some(entity);
    }
    let workspace = world
        .get::<crate::native::workspace::WorkspaceMember>(area)?
        .0;
    let position = world
        .get::<crate::native::canvas::CanvasItem>(area)?
        .position;
    let placement = crate::native::topology::spatial(world, area);
    world
        .get_mut::<crate::native::area::InfluenceArea>(area)?
        .protein
        .as_mut()?
        .enabled = false;
    world
        .get_mut::<crate::native::area::InfluenceArea>(area)?
        .protein
        .as_mut()?
        .source = Source::Local;
    let form = world
        .spawn((
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(
                    if mode == Mode::Command { 620.0 } else { 520.0 },
                    640.0,
                ),
            },
            crate::native::workspace::WorkspaceMember(workspace),
            placement,
            crate::native::sand::InBox(root),
            crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(root),
        ))
        .id();
    let mut id = [0; 16];
    getrandom::fill(&mut id).expect("group identity");
    let group = world
        .get::<crate::native::canvas_selection::SandGroup>(area)
        .copied()
        .unwrap_or(crate::native::canvas_selection::SandGroup(id));
    world.entity_mut(area).insert(group);
    world.entity_mut(form).insert(group);
    crate::native::topology::groups::attach(world, &[area, form]);
    let heading = crate::native::edit_mode::label(
        world,
        form,
        match mode {
            Mode::Record => "Find or create a Record",
            Mode::Command => "Find or create a Command",
        },
        20.0,
    );
    wrap(world, heading);
    let content = column(world, form);
    world.get_mut::<Node>(content).unwrap().flex_shrink = 1.0;
    world.get_mut::<Node>(content).unwrap().min_height = px(0);
    crate::native::scroll_sand::attach(world, content);
    let mut fields = Vec::new();
    let title = column(world, content);
    let identity = column(world, content);
    {
        let mut node = world.get_mut::<Node>(identity).unwrap();
        node.flex_direction = FlexDirection::Row;
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(8);
    }
    let assertions = column(world, content);
    let body = column(world, content);
    let properties = column(world, content);
    let keys: &[&str] = match mode {
        Mode::Record => &[
            "head",
            "body",
            "slug",
            "quantity",
            "start_date",
            "due_date",
            "estimate_min",
        ],
        Mode::Command => &["head", "slug", "body"],
    };
    for field in protein::record_schema::FIELDS
        .iter()
        .filter(|field| keys.contains(&field.key))
    {
        let parent = if matches!(field.key, "slug" | "quantity") {
            let parent = column(world, identity);
            let mut node = world.get_mut::<Node>(parent).unwrap();
            node.width = px(0);
            node.min_width = px(100);
            node.flex_grow = 1.0;
            parent
        } else {
            match field.key {
                "head" => title,
                "body" => body,
                _ => properties,
            }
        };
        let caption = match (mode, field.key) {
            (Mode::Command, "head") => "Title",
            (Mode::Command, "body") => "Shell script",
            _ => field.title,
        };
        let entity = input(world, parent, caption, "", field.key == "body");
        if matches!(field.key, "start_date" | "due_date") {
            crate::native::schedule_editor::attach(world, parent, entity);
        }
        if mode == Mode::Command && field.key == "body" {
            let font = crate::native::terminal::code_font(world);
            world.entity_mut(entity).insert(font);
            world.get_mut::<Node>(entity).unwrap().min_height = px(140);
        }
        fields.push((field.key, entity));
    }
    if mode == Mode::Record {
        crate::native::assertion_editor::spawn(
            world,
            assertions,
            crate::native::protein_area::RecordBinding {
                area,
                uid: String::new(),
                source: Source::Local,
            },
            &json!({}),
            true,
        );
    }
    let additions = column(world, content);
    if mode == Mode::Record {
        button(world, content, form, "Add assignee", AddAssignee);
        button(world, content, form, "Add work log", AddLog);
    }
    let status = crate::native::edit_mode::label(
        world,
        form,
        match mode {
            Mode::Record => {
                "Fill any fields to filter. Enter opens the first match. Create saves a new Record."
            }
            Mode::Command => {
                "Type a title or slug to find a saved Command. Enter opens the selected match. Create saves a new Command."
            }
        },
        13.0,
    );
    wrap(world, status);
    let controls = column(world, form);
    world.get_mut::<Node>(controls).unwrap().flex_direction = FlexDirection::Row;
    world.get_mut::<Node>(controls).unwrap().column_gap = px(8);
    button(world, controls, form, "Create", Submit);
    button(world, controls, form, "Cancel", Cancel);
    world.entity_mut(form).insert(Form {
        area,
        mode,
        original,
        draft: Draft::default(),
        fields,
        relations: Vec::new(),
        logs: Vec::new(),
        additions,
        assertions,
        status,
        pending: None,
    });
    search::start(world, form, root, mode);
    Some(form)
}

#[derive(Clone)]
struct AddAssignee;
impl Action for AddAssignee {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() || form.relations.len() >= 40 {
            return;
        }
        let parent = form.additions;
        let row = column(world, parent);
        let mut fields = Vec::new();
        fields.push(input(world, row, "Assignee (slug or identity)", "", false));
        button(world, row, entity, "Remove", Remove(row));
        world
            .get_mut::<Form>(entity)
            .unwrap()
            .relations
            .push((row, true, fields));
    }
}

#[derive(Clone)]
struct AddLog;
impl Action for AddLog {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() || form.logs.len() >= 40 {
            return;
        }
        let parent = form.additions;
        let row = column(world, parent);
        let start = input(
            world,
            row,
            "Work started (date, time and timezone)",
            "",
            false,
        );
        let end = input(world, row, "Work ended (optional)", "", false);
        button(world, row, entity, "Remove", Remove(row));
        world
            .get_mut::<Form>(entity)
            .unwrap()
            .logs
            .push((row, start, end));
    }
}

#[derive(Clone)]
struct Remove(Entity);
impl Action for Remove {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(mut form) = world.get_mut::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        form.relations.retain(|(row, _, _)| *row != self.0);
        form.logs.retain(|(row, _, _)| *row != self.0);
        world.despawn(self.0);
    }
}

fn status(world: &mut World, entity: Entity, message: &str) {
    if let Some(label) = world.get::<Form>(entity).map(|form| form.status) {
        world.get_mut::<Text>(label).unwrap().0 = message.into();
    }
}

fn freeze(world: &mut World, entity: Entity, frozen: bool) {
    let Some(form) = world.get::<Form>(entity) else {
        return;
    };
    let fields: Vec<_> = editors(world, form).collect();
    for field in fields {
        if frozen {
            world
                .entity_mut(field)
                .insert((Frozen, bevy::ui::InteractionDisabled));
        } else {
            world
                .entity_mut(field)
                .remove::<(Frozen, bevy::ui::InteractionDisabled)>();
        }
    }
}

fn gate(mut fields: Query<&mut EditableText, With<Frozen>>) {
    for mut text in &mut fields {
        if !text.pending_edits.is_empty() {
            text.pending_edits.clear();
        }
        if text.pending_paste.is_some() {
            text.pending_paste = None;
        }
    }
}

#[derive(Clone)]
struct Submit;
impl Action for Submit {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        if form.mode == Mode::Command {
            let result = command_action(world, form).and_then(|action| {
                let id = format!("create-command-{}", form.draft.uid);
                let bridge = world
                    .get_non_send::<crate::native::cell_bridge::CellBridge>()
                    .ok_or("The local Organ is not connected. Your draft is kept.")?;
                bridge
                    .outgoing
                    .try_send(cell::ClientMessage::Act {
                        id: id.clone(),
                        action,
                    })
                    .map_err(|_| "The local Organ is busy or disconnected. Try again.")?;
                Ok(id)
            });
            match result {
                Ok(id) => {
                    world.get_mut::<Form>(entity).unwrap().pending = Some(id);
                    freeze(world, entity, true);
                    status(world, entity, "Creating…");
                }
                Err(error) => status(world, entity, &error),
            }
            return;
        }
        let result = collect(world, form).and_then(|mut draft| {
            crate::native::area::creation::apply(world, entity, &mut draft)?;
            let id = format!("create-record-{}", draft.uid);
            let bridge = world
                .get_non_send::<crate::native::cell_bridge::CellBridge>()
                .ok_or("The local Organ is not connected. Your draft is kept.")?;
            bridge
                .outgoing
                .try_send(cell::ClientMessage::Act {
                    id: id.clone(),
                    action: engine::actions::Action::CreateRecordDraft { draft },
                })
                .map_err(|_| "The local Organ is busy or disconnected. Try again.")?;
            Ok(id)
        });
        match result {
            Ok(id) => {
                world.get_mut::<Form>(entity).unwrap().pending = Some(id);
                freeze(world, entity, true);
                status(world, entity, "Creating…");
            }
            Err(error) => status(world, entity, &error),
        }
    }
}

/// Builds the Command creation from the form's title, slug and shell script.
fn command_action(world: &World, form: &Form) -> Result<engine::actions::Action, String> {
    if form.fields.iter().any(|(_, entity)| {
        world.get::<EditableText>(*entity).is_some_and(|text| {
            text.is_composing() || text.pending_paste.is_some() || !text.pending_edits.is_empty()
        })
    }) {
        return Err("Finish editing before creating the Command".into());
    }
    let field = |key: &str| {
        form.fields
            .iter()
            .find(|(property, _)| *property == key)
            .map(|(_, entity)| value(world, *entity))
            .unwrap_or_default()
    };
    let head = field("head").trim().to_owned();
    let slug = field("slug").trim().trim_start_matches('@').to_owned();
    Ok(engine::actions::Action::CreateRecord {
        slug: (!slug.is_empty()).then_some(slug),
        kind: nucleus::RecordKind::Command,
        head: if head.is_empty() {
            "Command".into()
        } else {
            head
        },
        body: field("body"),
        quantity: 0.0,
    })
}

#[derive(Clone)]
struct Cancel;
impl Action for Cancel {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let area = form.area;
        let original = form.original.clone();
        world.despawn(entity);
        if let Some(original) = original {
            if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(area) {
                area.protein = Some(original);
            }
        } else {
            world.despawn(area);
        }
    }
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
    mut wave: Local<crate::native::cell_bridge::replies::Wave>,
    forms: &mut QueryState<(Entity, &'static Form)>,
    mut pending: Local<Vec<Entity>>,
) {
    use crate::native::cell_bridge::{CONNECTION, CellMessage, replies};
    let _receive = nucleus::performance::Span::new("record-creation-receive");
    let Some(messages) = world.get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
    else {
        return;
    };
    wave.capture(&mut cursor, messages, |message| {
        replies::action_id(message).is_some()
    });
    if wave.len() == 0 {
        pending.shrink_to_fit();
        return;
    }
    for sequence in wave.drain() {
        let event = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
        let id = replies::action_id(event).unwrap();
        pending.clear();
        pending.extend(
            forms
                .iter(world)
                .filter(|(_, form)| {
                    form.pending
                        .as_ref()
                        .is_some_and(|pending| pending == id || id == CONNECTION)
                })
                .map(|(entity, _)| entity),
        );
        if pending.is_empty() {
            continue;
        }
        let event = event.clone();
        nucleus::performance::add("record-creation-action-payloads", 1);
        let result = match event.as_ref() {
            cell::ServerMessage::ActionOk { created, .. } => created
                .as_deref()
                .ok_or("The Organ did not return the new Record"),
            cell::ServerMessage::Error { message, .. } => Err(message.as_str()),
            _ => unreachable!(),
        };
        for entity in pending.drain(..) {
            match &result {
                Ok(uid) => {
                    search::open_record(world, entity, uid);
                }
                Err(error) => {
                    world.get_mut::<Form>(entity).unwrap().pending = None;
                    freeze(world, entity, false);
                    status(world, entity, error);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    pub(super) fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::native::theme::Typography(Handle::default()))
            .add_plugins(RecordCreationPlugin);
        let root = app
            .world_mut()
            .spawn((
                crate::native::workspace::Workspaces::default(),
                crate::native::canvas::CanvasView::default(),
            ))
            .id();
        (app, root)
    }

    pub(super) fn fill(world: &mut World, form: Entity, key: &str, value: &str) {
        let field = world
            .get::<Form>(form)
            .unwrap()
            .fields
            .iter()
            .find(|(property, _)| *property == key)
            .unwrap()
            .1;
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(value);
        let fields: Vec<_> = world
            .get::<Form>(form)
            .unwrap()
            .fields
            .iter()
            .map(|(_, entity)| *entity)
            .collect();
        for field in fields {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .pending_edits
                .clear();
        }
    }

    #[test]
    fn event_opens_a_draft_and_cancel_needs_no_connection() {
        let (mut app, root) = app();
        app.world_mut().trigger(CreateRecord { entity: root });
        app.update();
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Form>>()
            .single(app.world())
            .unwrap();
        fill(app.world_mut(), entity, "head", "Keep my input");
        Submit.apply(app.world_mut(), entity);
        assert_eq!(
            collect(app.world(), app.world().get::<Form>(entity).unwrap())
                .unwrap()
                .head,
            "Keep my input"
        );
        assert!(app.world().get::<Form>(entity).unwrap().pending.is_none());
        Cancel.apply(app.world_mut(), entity);
        assert_eq!(
            app.world_mut()
                .query::<&crate::native::area::InfluenceArea>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn existing_castle_enters_creation_mode_and_cancel_restores_its_record() {
        let (mut app, root) = app();
        let area =
            crate::native::full_record::open(app.world_mut(), root, "existing", Source::Local)
                .unwrap();
        let original = app
            .world()
            .get::<crate::native::area::InfluenceArea>(area)
            .unwrap()
            .protein
            .clone();
        app.world_mut().trigger(CreateRecord { entity: area });
        app.update();
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Form>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<Form>(entity).unwrap().area, area);
        assert!(
            !app.world()
                .get::<crate::native::area::InfluenceArea>(area)
                .unwrap()
                .protein
                .as_ref()
                .unwrap()
                .enabled
        );
        app.world_mut().trigger(CreateRecord { entity: area });
        app.update();
        assert_eq!(
            app.world_mut().query::<&Form>().iter(app.world()).count(),
            1
        );
        Cancel.apply(app.world_mut(), entity);
        assert_eq!(
            app.world()
                .get::<crate::native::area::InfluenceArea>(area)
                .unwrap()
                .protein,
            original
        );
    }

    #[tokio::test]
    async fn creation_waits_for_submit_retains_errors_and_opens_the_created_record() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        store_local(&engine).await;
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let (mut app, root) = app();
        app.insert_resource(crate::native::app::CellHandle(runtime))
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .add_plugins(crate::native::cell_bridge::CellBridgePlugin);
        app.update();
        app.world_mut().trigger(CreateRecord { entity: root });
        app.update();
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Form>>()
            .single(app.world())
            .unwrap();
        let uid = app.world().get::<Form>(entity).unwrap().draft.uid.clone();
        let area = app.world().get::<Form>(entity).unwrap().area;
        let query: protein::Protein = serde_json::from_value(json!({"source":"record", "where":[{"uid_eq":uid}], "fields":["uid", "head", "quantity"]})).unwrap();
        assert!(
            protein::execute(&engine.store, &query)
                .await
                .unwrap()
                .is_empty()
        );
        fill(app.world_mut(), entity, "head", "Prepared title");
        fill(app.world_mut(), entity, "slug", "bad slug");
        Submit.apply(app.world_mut(), entity);
        for _ in 0..500 {
            app.update();
            if app.world().get::<Form>(entity).unwrap().pending.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert_eq!(
            collect(app.world(), app.world().get::<Form>(entity).unwrap())
                .unwrap()
                .head,
            "Prepared title"
        );
        assert!(
            protein::execute(&engine.store, &query)
                .await
                .unwrap()
                .is_empty()
        );
        fill(app.world_mut(), entity, "slug", "prepared-title");
        fill(app.world_mut(), entity, "quantity", "3.125");
        Submit.apply(app.world_mut(), entity);
        Submit.apply(app.world_mut(), entity);
        for _ in 0..500 {
            app.update();
            if app.world().get::<Form>(entity).is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert!(app.world().get::<Form>(entity).is_none());
        let rows = protein::execute(&engine.store, &query).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["head"], "Prepared title");
        assert_eq!(rows[0]["quantity"], "3.125");
        assert!(
            app.world()
                .get::<crate::native::area::InfluenceArea>(area)
                .unwrap()
                .protein
                .as_ref()
                .unwrap()
                .enabled
        );
    }

    async fn store_local(engine: &engine::Engine) {
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Existing".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap();
    }
}
