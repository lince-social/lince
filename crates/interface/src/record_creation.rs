use bevy::{prelude::*, text::EditableText};
use engine::record_creation::{Assertion, Draft};
use serde_json::json;

use crate::{
    actions::{Action, ActionButton},
    protein_area::Source,
};

mod search;

#[derive(EntityEvent)]
pub struct CreateRecord {
    pub entity: Entity,
}

#[derive(Component)]
struct Form {
    area: Entity,
    original: Option<crate::protein_area::Config>,
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
        if !app.is_plugin_added::<crate::assertion_editor::AssertionEditorPlugin>() {
            app.add_plugins(crate::assertion_editor::AssertionEditorPlugin);
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
            .add_systems(Update, receive.after(crate::cell_bridge::ReceiveCell))
            .add_systems(
                PostUpdate,
                (gate, search::keys).before(bevy::text::EditableTextSystems),
            )
            .add_systems(PostUpdate, fit.after(bevy::ui::UiSystems::PostLayout))
            .add_systems(
                PostUpdate,
                search::update
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

fn fit(world: &mut World) {
    let forms: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .map(|(entity, form)| (entity, form.area))
        .collect();
    for (entity, area) in forms {
        if world.get::<crate::layout::LayoutBox>(entity).is_none() {
            let nodes: Vec<_> = world
                .get::<Children>(entity)
                .into_iter()
                .flatten()
                .filter_map(|child| world.get::<ComputedNode>(*child))
                .collect();
            let height = 24.0
                + nodes.len().saturating_sub(1) as f32 * 8.0
                + nodes
                    .iter()
                    .map(|node| {
                        node.size().y.max(node.content_size.y) * node.inverse_scale_factor()
                    })
                    .sum::<f32>();
            if height > 48.0 {
                world
                    .get_mut::<crate::canvas::CanvasItem>(entity)
                    .unwrap()
                    .size
                    .y = height.clamp(120.0, 1000.0);
            }
        }
        crate::full_record::fit_source(world, area, entity);
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
    crate::edit_mode::label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor(value, world.resource::<crate::theme::Typography>(), 0);
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

fn button(world: &mut World, parent: Entity, owner: Entity, title: &str, action: impl Action) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            ActionButton::new(owner, crate::actions![action]),
            Node {
                min_height: px(32),
                flex_shrink: 0.0,
                padding: UiRect::all(px(6)),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::edit_mode::label(world, entity, title, 14.0);
}

pub fn open(world: &mut World, target: Entity) -> Option<Entity> {
    if crate::laboratory::active(world) || !world.contains_resource::<crate::theme::Typography>() {
        return None;
    }
    let existing = world.get::<crate::area::InfluenceArea>(target);
    let original = existing.and_then(|area| area.protein.clone());
    let root = if existing.is_some() {
        world.get::<ChildOf>(target)?.parent()
    } else {
        target
    };
    let area = if original.is_some() {
        target
    } else {
        crate::full_record::open(world, root, "pending", Source::Local)?
    };
    if let Some(entity) = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .find(|(_, form)| form.area == area)
        .map(|(entity, _)| entity)
    {
        return Some(entity);
    }
    let workspace = world.get::<crate::workspace::WorkspaceMember>(area)?.0;
    let position = world.get::<crate::canvas::CanvasItem>(area)?.position;
    let placement = crate::topology::spatial(world, area);
    world
        .get_mut::<crate::area::InfluenceArea>(area)?
        .protein
        .as_mut()?
        .enabled = false;
    world
        .get_mut::<crate::area::InfluenceArea>(area)?
        .protein
        .as_mut()?
        .source = Source::Local;
    let form = world
        .spawn((
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(520.0, 640.0),
            },
            crate::workspace::WorkspaceMember(workspace),
            placement,
            crate::sand::InBox(root),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(root),
        ))
        .id();
    let mut id = [0; 16];
    getrandom::fill(&mut id).expect("group identity");
    let group = world
        .get::<crate::canvas_selection::SandGroup>(area)
        .copied()
        .unwrap_or(crate::canvas_selection::SandGroup(id));
    world.entity_mut(area).insert(group);
    world.entity_mut(form).insert(group);
    crate::topology::groups::attach(world, &[area, form]);
    crate::edit_mode::label(world, form, "Find or create a Record", 20.0);
    let content = column(world, form);
    world.get_mut::<Node>(content).unwrap().flex_shrink = 1.0;
    world.get_mut::<Node>(content).unwrap().min_height = px(0);
    crate::scroll_sand::attach(world, content);
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
    for field in protein::record_schema::fields()
        .into_iter()
        .filter(|field| {
            matches!(
                field.key,
                "head" | "body" | "slug" | "quantity" | "start_date" | "due_date" | "estimate_min"
            )
        })
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
        let entity = input(world, parent, field.title, "", field.key == "body");
        fields.push((field.key, entity));
    }
    crate::assertion_editor::spawn(
        world,
        assertions,
        crate::protein_area::RecordBinding {
            area,
            uid: String::new(),
            source: Source::Local,
        },
        &json!({}),
        true,
    );
    let additions = column(world, content);
    button(world, content, form, "Add assignee", AddAssignee);
    button(world, content, form, "Add work log", AddLog);
    let status = crate::edit_mode::label(
        world,
        form,
        "Fill any fields to filter. Enter opens the first match. Create saves a new Record.",
        13.0,
    );
    let controls = column(world, form);
    world.get_mut::<Node>(controls).unwrap().flex_direction = FlexDirection::Row;
    button(world, controls, form, "Create", Submit);
    button(world, controls, form, "Cancel", Cancel);
    world.entity_mut(form).insert(Form {
        area,
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
    search::start(world, form, root);
    Some(form)
}

fn value(world: &World, entity: Entity) -> String {
    world
        .get::<EditableText>(entity)
        .map(|text| text.value().to_string())
        .unwrap_or_default()
}

fn optional(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn collect(world: &World, form: &Form) -> Result<Draft, String> {
    let editors = form
        .fields
        .iter()
        .map(|(_, entity)| *entity)
        .chain(crate::assertion_editor::input(world, form.assertions))
        .chain(
            form.relations
                .iter()
                .flat_map(|(_, _, fields)| fields.iter().copied()),
        )
        .chain(form.logs.iter().flat_map(|(_, start, end)| [*start, *end]));
    if editors.into_iter().any(|entity| {
        world.get::<EditableText>(entity).is_some_and(|text| {
            text.is_composing() || text.pending_paste.is_some() || !text.pending_edits.is_empty()
        })
    }) {
        return Err("Finish editing before creating the Record".into());
    }
    let mut draft = form.draft.clone();
    draft.work = json!({});
    for (property, entity) in &form.fields {
        let value = value(world, *entity);
        match *property {
            "head" => draft.head = value,
            "body" => draft.body = value,
            "slug" => draft.slug = optional(value),
            "quantity" => {
                draft.quantity = if value.trim().is_empty() {
                    "0".into()
                } else {
                    value.trim().into()
                }
            }
            "start_date" | "due_date" => {
                if let Some(value) = optional(value) {
                    draft.work[if *property == "start_date" {
                        "start"
                    } else {
                        "due"
                    }] = json!(value);
                }
            }
            "estimate_min" => {
                if let Some(value) = optional(value) {
                    draft.work["estimate_min"] = json!(
                        value
                            .parse::<f64>()
                            .map_err(|_| "Enter an estimate in minutes")?
                    );
                }
            }
            _ => {}
        }
    }
    draft.assertions = crate::assertion_editor::draft(world, form.assertions)?;
    for (_, assignee, fields) in &form.relations {
        let values: Vec<_> = fields.iter().map(|field| value(world, *field)).collect();
        if values.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        draft.assertions.push(if *assignee {
            Assertion {
                predicate: "assigned-to".into(),
                object: optional(values[0].clone()),
                quantity: None,
                unit: None,
            }
        } else {
            Assertion {
                predicate: values[0].trim().trim_start_matches('#').into(),
                object: optional(values[1].clone()),
                quantity: optional(values[2].clone()),
                unit: optional(values[3].clone()),
            }
        });
    }
    let logs: Vec<_> = form
        .logs
        .iter()
        .filter_map(|(_, start, end)| {
            let start = value(world, *start);
            let end = optional(value(world, *end));
            (!start.trim().is_empty() || end.is_some())
                .then(|| json!({"start":start.trim(), "end":end}))
        })
        .collect();
    if !logs.is_empty() {
        draft.work["logs"] = json!(logs);
    }
    draft.validate().map_err(|error| error.to_string())?;
    Ok(draft)
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
    let fields: Vec<_> = form
        .fields
        .iter()
        .map(|(_, entity)| *entity)
        .chain(crate::assertion_editor::input(world, form.assertions))
        .chain(
            form.relations
                .iter()
                .flat_map(|(_, _, fields)| fields.iter().copied()),
        )
        .chain(form.logs.iter().flat_map(|(_, start, end)| [*start, *end]))
        .collect();
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
        text.pending_edits.clear();
        text.pending_paste = None;
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
        let result = collect(world, form).and_then(|draft| {
            let id = format!("create-record-{}", draft.uid);
            let bridge = world
                .get_non_send::<crate::cell_bridge::CellBridge>()
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
            if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(area) {
                area.protein = Some(original);
            }
        } else {
            world.despawn(area);
        }
    }
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let events: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::ActionOk { id, created, .. } => (
                id,
                created.ok_or_else(|| "The Organ did not return the new Record".to_string()),
            ),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let forms: Vec<_> = world
            .query::<(Entity, &Form)>()
            .iter(world)
            .filter(|(_, form)| {
                form.pending
                    .as_ref()
                    .is_some_and(|pending| pending == &id || id == crate::cell_bridge::CONNECTION)
            })
            .map(|(entity, _)| entity)
            .collect();
        for entity in forms {
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
            .insert_resource(crate::theme::Typography(Handle::default()))
            .add_plugins(RecordCreationPlugin);
        let root = app
            .world_mut()
            .spawn((
                crate::workspace::Workspaces::default(),
                crate::canvas::CanvasView::default(),
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
                .query::<&crate::area::InfluenceArea>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn existing_castle_enters_creation_mode_and_cancel_restores_its_record() {
        let (mut app, root) = app();
        let area =
            crate::full_record::open(app.world_mut(), root, "existing", Source::Local).unwrap();
        let original = app
            .world()
            .get::<crate::area::InfluenceArea>(area)
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
                .get::<crate::area::InfluenceArea>(area)
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
                .get::<crate::area::InfluenceArea>(area)
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
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        };
        let (mut app, root) = app();
        app.insert_resource(crate::app::CellHandle(runtime))
            .insert_resource(crate::wake::WakeSignal::new(|| {}))
            .add_plugins(crate::cell_bridge::CellBridgePlugin);
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
                .get::<crate::area::InfluenceArea>(area)
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
