use crate::{
    canvas::CanvasItem,
    cell_bridge::{CellBridge, CellMessage, RECORDS, ReceiveCell},
    container::BoxRoot,
    sand::{InBox, Square, text_editor},
    theme::Typography,
};
use bevy::{
    input_focus::InputFocus,
    math::DVec2,
    prelude::*,
    text::{EditableText, TextEdit},
};
use cell::{ClientMessage, ServerMessage};
use engine::actions::Action;
use std::collections::{HashMap, HashSet};

#[derive(Component)]
#[require(AutoSave, crate::sand::Unsaved(false))]
pub struct RecordEditor {
    pub uid: String,
    pub confirmed: String,
    pub pending: Option<(String, String)>,
    pub status: Entity,
}

#[derive(Component, Default)]
struct AutoSave(Option<String>);

#[derive(Resource)]
struct RecordsView {
    root: Entity,
    records: HashMap<String, (Entity, Entity)>,
    next_slot: usize,
    next_request: u64,
}

pub struct RecordViewPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReceiveRecords;
impl Plugin for RecordViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, receive.in_set(ReceiveRecords).after(ReceiveCell))
            .add_systems(
                PostUpdate,
                save.after(bevy::text::EditableTextSystems)
                    .before(crate::sand::StyleSaveState)
                    .run_if(crate::laboratory::normal),
            );
    }
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    world.insert_resource(RecordsView {
        root,
        records: HashMap::new(),
        next_slot: 0,
        next_request: 0,
    });
}

fn save(
    mut editors: Query<
        (
            &EditableText,
            &mut RecordEditor,
            &mut AutoSave,
            &mut crate::save_feedback::SaveFeedback,
        ),
        Without<crate::record_binding::TextBinding>,
    >,
    mut labels: Query<&mut Text>,
    mut view: ResMut<RecordsView>,
    bridge: NonSend<CellBridge>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    for (text, mut record, mut automatic, mut feedback) in &mut editors {
        if text.is_composing() || pending_text(text) {
            continue;
        }
        if record.pending.is_some() {
            continue;
        }
        let head = text.value().to_string();
        if head == record.confirmed {
            if automatic.0.is_some() {
                automatic.0 = None;
            }
            continue;
        }
        if automatic.0.as_ref() == Some(&head) {
            continue;
        }
        view.next_request += 1;
        let id = format!("edit-{}", view.next_request);
        let request = ClientMessage::Act {
            id: id.clone(),
            action: Action::EditRecordText {
                target: record.uid.clone(),
                head: Some(head.clone()),
                body: None,
            },
        };
        let message = match bridge.outgoing.try_send(request) {
            Ok(()) => {
                feedback.set_failed(false);
                automatic.0 = Some(head.clone());
                record.pending = Some((id, head));
                "Saving…"
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                if let Some(wake) = &wake {
                    wake.ring();
                }
                "Waiting to send…"
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                feedback.set_failed(true);
                automatic.0 = Some(head);
                "Connection closed. Your draft is here; reopen Lince to reconnect."
            }
        };
        if let Ok(mut label) = labels.get_mut(record.status) {
            label.0 = message.into();
        }
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == RECORDS =>
            {
                snapshot(world, rows)
            }
            ServerMessage::ActionOk { id, warnings, .. } => acknowledge(world, &id, Ok(warnings)),
            ServerMessage::Error { id, message, .. } => {
                if id == crate::cell_bridge::CONNECTION {
                    let pending: Vec<_> = world
                        .query::<(Entity, &mut RecordEditor)>()
                        .iter_mut(world)
                        .filter_map(|(entity, mut record)| {
                            record.pending.take().map(|_| (entity, record.status))
                        })
                        .collect();
                    for (entity, status) in pending {
                        crate::save_feedback::set_failed(world, entity, true);
                        if let Some(mut text) = world.get_mut::<Text>(status) {
                            text.0 = "Save was not confirmed. Your draft is still here.".into();
                        }
                    }
                    crate::notifications::report(world, "interface::connection", &message);
                } else if id == RECORDS {
                    crate::notifications::report(
                        world,
                        "interface::records",
                        &format!(
                            "Could not load Records. Existing Records and drafts have been kept: {message}"
                        ),
                    );
                } else {
                    acknowledge(world, &id, Err(message.clone()));
                    crate::notifications::report(world, "interface::records", &message);
                }
            }
            _ => {}
        }
    }
}

fn acknowledge(world: &mut World, id: &str, result: Result<Vec<String>, String>) {
    let mut editors = world.query::<(Entity, &mut RecordEditor, &mut AutoSave)>();
    let mut label = None;
    for (entity, mut record, mut automatic) in editors.iter_mut(world) {
        if record
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| pending == id)
        {
            let (_, head) = record.pending.take().unwrap();
            let text = match &result {
                Ok(warnings) => {
                    automatic.0 = None;
                    record.confirmed = head;
                    if warnings.is_empty() {
                        "Saved".into()
                    } else {
                        format!("Saved: {}", warnings.join("; "))
                    }
                }
                Err(message) => format!("Not saved: {message}"),
            };
            label = Some((entity, record.status, text));
            break;
        }
    }
    if let Some((editor, status, text)) = label {
        crate::save_feedback::set_failed(world, editor, result.is_err());
        world.get_mut::<Text>(status).unwrap().0 = text;
    }
}

pub(crate) fn pending_text(input: &EditableText) -> bool {
    input.pending_paste.is_some()
        || input.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                TextEdit::Cut
                    | TextEdit::Paste
                    | TextEdit::Insert(_)
                    | TextEdit::Backspace
                    | TextEdit::BackspaceWord
                    | TextEdit::Delete
                    | TextEdit::DeleteWord
                    | TextEdit::ImeSetCompose { .. }
                    | TextEdit::ImeCommit { .. }
            )
        })
}

pub(crate) fn hide_placement(world: &mut World, root: Entity, entity: Entity) {
    if let Some(uid) = world.get::<crate::workspace::RecordPlacement>(entity).map(|placement| placement.0.clone()) {
        if let Some(mut view) = world.get_resource_mut::<RecordsView>() {
            if view.root == root && view.records.get(&uid).is_some_and(|(card, _)| *card == entity) {
                view.records.remove(&uid);
            }
        }
        if let Some(mut spaces) = world.get_mut::<crate::workspace::Workspaces>(root) { spaces.saved_records.remove(&uid); spaces.hidden_records.insert(uid); }
    }
}

fn snapshot(world: &mut World, rows: Vec<serde_json::Value>) {
    let mut identities = HashSet::new();
    if rows.iter().any(|row| {
        row["uid"]
            .as_str()
            .is_none_or(|uid| uid.is_empty() || !identities.insert(uid))
            || !row["head"].is_string()
    }) {
        crate::notifications::report(
            world,
            "interface::records",
            "The Cell returned incomplete Records. Existing Records and drafts have been kept.",
        );
        return;
    }
    let mut view = world.remove_resource::<RecordsView>().unwrap();
    let mut seen = HashSet::new();
    for row in rows {
        let (Some(uid), Some(head)) = (row["uid"].as_str(), row["head"].as_str()) else {
            continue;
        };
        if !seen.insert(uid.to_string()) {
            continue;
        }
        if world.get::<crate::workspace::Workspaces>(view.root).is_some_and(|spaces| spaces.hidden_records.contains(uid)) { continue; }
        if let Some((card, editor)) = view.records.get(uid) {
            let properties = crate::area::RecordProperties(row.clone());
            if world.get::<crate::area::RecordProperties>(*card) != Some(&properties) {
                world.entity_mut(*card).insert(properties);
            }
            if crate::record_binding::active(world, *editor) {
                continue;
            }
            let current = world
                .get::<EditableText>(*editor)
                .unwrap()
                .value()
                .to_string();
            let record = world.get::<RecordEditor>(*editor).unwrap();
            let pending = record.pending.is_some();
            let pristine = current == record.confirmed;
            let input = world.get::<EditableText>(*editor).unwrap();
            if !pending
                && pristine
                && current != head
                && !pending_text(input)
                && !input.is_composing()
            {
                world
                    .get_mut::<EditableText>(*editor)
                    .unwrap()
                    .editor
                    .set_text(head);
            }
            if !pending {
                world.get_mut::<RecordEditor>(*editor).unwrap().confirmed = head.into();
            }
            continue;
        }
        let index = view.next_slot;
        view.next_slot += 1;
        let card = world
            .spawn((
                Square,
                crate::area::RecordProperties(row.clone()),
                InBox(view.root),
                ChildOf(view.root),
                CanvasItem {
                    position: DVec2::new(
                        (index % 3) as f64 * 252.0 - 252.0,
                        (index / 3) as f64 * 196.0,
                    ),
                    size: Vec2::new(232.0, 176.0),
                },
                Node {
                    padding: UiRect::all(px(16)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                crate::token_style::background(crate::tokens::Token::Surface),
            ))
            .id();
        let small = world.resource::<Typography>().text(14.0);
        let status = world
            .spawn((
                Text::new(""),
                small,
                crate::token_style::text(crate::tokens::Token::Ink),
                ChildOf(card),
            ))
            .id();
        let editor_bundle = text_editor(head, world.resource::<Typography>(), 0);
        let editor = world
            .spawn((
                editor_bundle,
                ChildOf(card),
                RecordEditor {
                    uid: uid.into(),
                    confirmed: head.into(),
                    pending: None,
                    status,
                },
            ))
            .id();
        world.entity_mut(editor).insert(EditableText {
            allow_newlines: false,
            max_characters: Some(4096),
            visible_lines: Some(2.0),
            ..crate::sand::editable(head)
        });
        world.entity_mut(card).replace_children(&[editor, status]);
        let button = world
            .spawn((
                Square,
                crate::actions::ActionButton::new(
                    card,
                    crate::actions![crate::full_record::Open(
                        crate::protein_area::RecordBinding {
                            area: card,
                            uid: uid.into(),
                            source: crate::protein_area::Source::Local
                        }
                    )],
                ),
                ChildOf(card),
            ))
            .id();
        crate::edit_mode::label(world, button, "Open Record", 14.0);
        if crate::record_binding::enabled(world) {
            crate::record_binding::attach(
                world,
                editor,
                crate::protein_area::RecordBinding {
                    area: card,
                    uid: uid.into(),
                    source: crate::protein_area::Source::Local,
                },
                "head",
                Some(status),
            );
        }
        crate::workspace::place_record(world, view.root, card, uid);
        view.records.insert(uid.into(), (card, editor));
    }
    view.records.retain(|uid, (card, editor)| {
        if seen.contains(uid) {
            return true;
        }
        let focus = world.resource::<InputFocus>().get();
        if focus == Some(*editor)
            || focus.is_some_and(|entity| {
                world
                    .get::<ChildOf>(entity)
                    .is_some_and(|parent| parent.parent() == *card)
            })
        {
            world.resource_mut::<InputFocus>().clear();
        }
        world.despawn(*card);
        false
    });
    if let Some(mut spaces) = world.get_mut::<crate::workspace::Workspaces>(view.root)
        && spaces.saved_records.keys().any(|uid| !seen.contains(uid))
    {
        spaces.saved_records.retain(|uid, _| seen.contains(uid));
    }
    world.insert_resource(view);
}

pub(crate) mod tests {
    use super::*;

    #[cfg(test)]
    #[test]
    fn hidden_canvas_record_does_not_respawn_on_updates_or_remove_other_records() {
        let mut world = fixture();
        let root = world.resource::<RecordsView>().root;
        world.entity_mut(root).insert(crate::workspace::Workspaces::default());
        let (card, _) = world.resource::<RecordsView>().records["record-a"];
        hide_placement(&mut world, root, card);
        world.despawn(card);
        let rows = vec![
            serde_json::json!({"uid": "record-a", "head": "Still retained"}),
            serde_json::json!({"uid": "record-b", "head": "Visible"}),
        ];
        snapshot(&mut world, rows.clone());
        snapshot(&mut world, rows);
        assert!(!world.resource::<RecordsView>().records.contains_key("record-a"));
        assert!(world.resource::<RecordsView>().records.contains_key("record-b"));
        assert!(world.get::<crate::workspace::Workspaces>(root).unwrap().hidden_records.contains("record-a"));
    }

    #[cfg_attr(test, test)]
    fn records_keep_their_workspace_and_draft_while_receiving_live_updates() {
        let mut world = fixture();
        let root = world.resource::<RecordsView>().root;
        world
            .entity_mut(root)
            .insert(crate::workspace::Workspaces::default());
        let (card, editor) = world.resource::<RecordsView>().records["record-a"];
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("My draft");
        crate::workspace::create(&mut world, root);
        snapshot(
            &mut world,
            vec![
                serde_json::json!({"uid": "record-a", "head": "External edit"}),
                serde_json::json!({"uid": "record-b", "head": "New Record"}),
            ],
        );
        let (new_card, _) = world.resource::<RecordsView>().records["record-b"];
        for entity in [card, new_card] {
            assert_eq!(
                world
                    .get::<crate::workspace::WorkspaceMember>(entity)
                    .unwrap()
                    .0,
                1
            );
        }
        assert_eq!(
            world.get::<RecordEditor>(editor).unwrap().confirmed,
            "External edit"
        );
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "My draft"
        );
        crate::workspace::switch(&mut world, root, 1);
        assert_eq!(
            world.resource::<RecordsView>().records["record-a"],
            (card, editor)
        );
    }

    fn fixture() -> World {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Typography>();
        world.init_resource::<InputFocus>();
        world.insert_resource(crate::notifications::Notifications::new(
            cell::Diagnostics::default(),
        ));
        setup(&mut world);
        snapshot(
            &mut world,
            vec![serde_json::json!({"uid": "record-a", "head": "Original"})],
        );
        world
    }

    #[cfg_attr(test, test)]
    fn malformed_replies_preserve_records_and_report_the_problem() {
        let mut world = fixture();
        let (card, editor) = world.resource::<RecordsView>().records["record-a"];
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Keep my draft");
        for rows in [
            vec![serde_json::json!({"uid": "record-a"})],
            vec![
                serde_json::json!({"uid": "record-a", "head": "One"}),
                serde_json::json!({"uid": "record-a", "head": "Two"}),
            ],
        ] {
            snapshot(&mut world, rows);
        }
        assert!(world.get_entity(card).is_ok());
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Keep my draft"
        );
        assert_eq!(
            world.get::<RecordEditor>(editor).unwrap().confirmed,
            "Original"
        );
        assert_eq!(
            world
                .resource::<crate::notifications::Notifications>()
                .log
                .snapshot()
                .1[0]
                .occurrences,
            2
        );
    }

    #[cfg_attr(test, test)]
    fn connection_failure_releases_pending_saves_and_keeps_drafts() {
        let mut world = fixture();
        let (_, editor) = world.resource::<RecordsView>().records["record-a"];
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Keep my draft");
        world.get_mut::<RecordEditor>(editor).unwrap().pending =
            Some(("pending".into(), "Sent version".into()));
        world.init_resource::<Messages<CellMessage>>();
        world.write_message(CellMessage(ServerMessage::Error {
            id: crate::cell_bridge::CONNECTION.into(),
            message: "Connection stopped".into(),
            code: None,
        }));
        let mut schedule = Schedule::default();
        schedule.add_systems(receive);
        schedule.run(&mut world);
        let record = world.get::<RecordEditor>(editor).unwrap();
        assert!(record.pending.is_none());
        assert_eq!(record.confirmed, "Original");
        assert!(
            world
                .get::<Text>(record.status)
                .unwrap()
                .0
                .contains("not confirmed")
        );
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Keep my draft"
        );
        assert_eq!(
            world
                .resource::<crate::notifications::Notifications>()
                .log
                .snapshot()
                .1
                .len(),
            1
        );
    }

    #[cfg_attr(test, test)]
    fn incoming_text_does_not_overwrite_queued_input() {
        let mut world = fixture();
        let (_, editor) = world.resource::<RecordsView>().records["record-a"];
        snapshot(
            &mut world,
            vec![serde_json::json!({"uid": "record-a", "head": "Incoming title"})],
        );
        let input = world.get::<EditableText>(editor).unwrap();
        assert_eq!(input.value().to_string(), "Incoming title");
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .queue_edit(bevy::text::TextEdit::Insert("draft".into()));
        snapshot(
            &mut world,
            vec![serde_json::json!({"uid": "record-a", "head": "Another title"})],
        );
        let input = world.get::<EditableText>(editor).unwrap();
        assert_eq!(input.value().to_string(), "Incoming title");
        assert!(
            input
                .pending_edits
                .contains(&TextEdit::Insert("draft".into()))
        );
    }

    #[cfg_attr(test, test)]
    fn snapshots_keep_entities_positions_and_unsaved_typing() {
        let mut world = fixture();
        let (card, editor) = world.resource::<RecordsView>().records["record-a"];
        world.get_mut::<CanvasItem>(card).unwrap().position = DVec2::splat(1000.0);
        world
            .entity_mut(editor)
            .insert(crate::sand::editable("My draft"));
        snapshot(
            &mut world,
            vec![serde_json::json!({"uid": "record-a", "head": "External edit"})],
        );
        assert_eq!(
            world.resource::<RecordsView>().records["record-a"],
            (card, editor)
        );
        assert_eq!(
            world.get::<CanvasItem>(card).unwrap().position,
            DVec2::splat(1000.0)
        );
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "My draft"
        );
        assert_eq!(
            world.get::<RecordEditor>(editor).unwrap().confirmed,
            "External edit"
        );
    }

    #[cfg_attr(test, test)]
    fn late_acknowledgment_and_failure_preserve_newer_typing() {
        let mut world = fixture();
        let (_, editor) = world.resource::<RecordsView>().records["record-a"];
        world.get_mut::<RecordEditor>(editor).unwrap().pending =
            Some(("edit-1".into(), "Submitted".into()));
        world
            .entity_mut(editor)
            .insert(crate::sand::editable("Newer typing"));
        acknowledge(&mut world, "edit-1", Ok(Vec::new()));
        snapshot(
            &mut world,
            vec![serde_json::json!({"uid": "record-a", "head": "Submitted"})],
        );
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Newer typing"
        );
        world.get_mut::<RecordEditor>(editor).unwrap().pending =
            Some(("edit-2".into(), "Newer typing".into()));
        acknowledge(&mut world, "edit-2", Err("Permission denied".into()));
        let state = world.get::<RecordEditor>(editor).unwrap();
        assert_eq!(state.confirmed, "Submitted");
        assert!(state.pending.is_none());
        assert_eq!(
            world.get::<Text>(state.status).unwrap().0,
            "Not saved: Permission denied"
        );
        assert_eq!(
            world
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Newer typing"
        );
    }

    crate::laboratory_cases! {
        records_keep_their_workspace_and_draft_while_receiving_live_updates,
        malformed_replies_preserve_records_and_report_the_problem,
        connection_failure_releases_pending_saves_and_keeps_drafts,
        incoming_text_does_not_overwrite_queued_input,
        snapshots_keep_entities_positions_and_unsaved_typing,
        late_acknowledgment_and_failure_preserve_newer_typing,
    }
}
