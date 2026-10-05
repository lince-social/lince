use super::*;
use cell::{ClientMessage, ServerMessage};

#[cfg(feature = "instinct")]
pub(crate) mod tests;

#[derive(Component, Default)]
pub(super) struct Import {
    request: Option<String>,
    fingerprint: Option<String>,
    preview: Option<serde_json::Value>,
    message: String,
    committing: bool,
    imported: bool,
    started: Option<std::time::Instant>,
    timer: Option<tokio::task::JoinHandle<()>>,
    details: Option<usize>,
}

impl Drop for Import {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
    }
}

pub(super) fn active(imports: Query<&Import>) -> bool {
    imports.iter().any(|state| state.request.is_some())
}

#[derive(Clone)]
struct Inspect(String);

impl Action for Inspect {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = practice::root(world, owner) else {
            return;
        };
        let source = crate::practice_cells::source(world, owner)
            .map(crate::protein_area::Source::Organ)
            .unwrap_or(crate::protein_area::Source::Local);
        if let Some(view) = crate::full_record::open(world, root, &self.0, source.clone()) {
            if let crate::protein_area::Source::Organ(source) = source {
                world
                    .entity_mut(view)
                    .insert(crate::practice_cells::PracticeSource(source));
            }
            crate::instinct::practice::track_custom(world, root, &[view]);
        }
    }
}

#[derive(Clone, Copy)]
struct Details(usize);

impl Action for Details {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut state) = world.get_mut::<Import>(owner) {
            state.details = (state.details != Some(self.0)).then_some(self.0);
        }
        reader::render(world, owner);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ImportAction {
    Preview,
    Commit,
    Cancel,
}

impl Action for ImportAction {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Cancel) {
            if world
                .get::<Import>(owner)
                .is_some_and(|state| state.committing)
            {
                crate::notifications::report(
                    world,
                    "Instinct",
                    "The import was submitted to the Cell. Closing this status does not cancel the atomic import. Preview again to inspect the result.",
                );
            }
            world.entity_mut(owner).remove::<Import>();
            world.entity_mut(owner).insert(Cancelled);
            reader::render(world, owner);
            return;
        }
        if world
            .get::<Import>(owner)
            .is_some_and(|state| state.request.is_some())
        {
            return;
        }
        let action = match self {
            Self::Preview => engine::actions::Action::PreviewInstinct,
            Self::Commit => {
                let Some(fingerprint) = world
                    .get::<Import>(owner)
                    .and_then(|state| state.fingerprint.clone())
                else {
                    return;
                };
                engine::actions::Action::ImportInstinct { fingerprint }
            }
            Self::Cancel => return,
        };
        let id = format!("instinct-import-{}", nucleus::new_uid("request"));
        world.entity_mut(owner).remove::<Cancelled>();
        let result = crate::practice_cells::sender(world, owner)
            .ok_or("The Cell is disconnected.")
            .and_then(|sender| {
                sender
                    .try_send(ClientMessage::Act {
                        id: id.clone(),
                        action,
                    })
                    .map_err(
                        |_| "The Cell is busy or disconnected. Preview again when it is available.",
                    )
            });
        if world.get::<Import>(owner).is_none() {
            world.entity_mut(owner).insert(Import::default());
        }
        let mut state = world.get_mut::<Import>(owner).unwrap();
        state.committing = matches!(self, Self::Commit);
        state.imported = false;
        state.message = match result {
            Ok(()) => {
                state.request = Some(id);
                state.started = Some(std::time::Instant::now());
                "Waiting for the Cell…".into()
            }
            Err(message) => message.into(),
        };
        drop(state);
        if world.get::<Import>(owner).unwrap().request.is_some()
            && let Ok(handle) = tokio::runtime::Handle::try_current()
            && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
        {
            let timer = handle.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                wake.ring();
            });
            world.get_mut::<Import>(owner).unwrap().timer = Some(timer);
        }
        reader::render(world, owner);
    }
}

#[derive(Component)]
struct Cancelled;

pub(crate) fn preview_ready(world: &World, owner: Entity, conflicts: bool) -> bool {
    world.get::<Import>(owner).is_some_and(|state| {
        state.request.is_none()
            && state.fingerprint.is_some()
            && state
                .preview
                .as_ref()
                .and_then(|preview| preview["conflicts"].as_array())
                .is_some_and(|rows| rows.is_empty() != conflicts)
    })
}

pub(crate) fn cancelled(world: &World, owner: Entity) -> bool {
    world.get::<Cancelled>(owner).is_some() && world.get::<Import>(owner).is_none()
}

pub(crate) fn imported(world: &World, owner: Entity) -> bool {
    world
        .get::<Import>(owner)
        .is_some_and(|state| state.imported && state.request.is_none())
}

pub(super) fn controls(world: &mut World, owner: Entity, parent: Entity) {
    if !cfg!(feature = "instinct") {
        return;
    }
    crate::description::button(
        world,
        parent,
        owner,
        "Preview handbook import",
        ImportAction::Preview,
    );
    let Some(state) = world.get::<Import>(owner) else {
        return;
    };
    let (message, preview, ready, committing, imported, details) = (
        state.message.clone(),
        state.preview.clone(),
        state.request.is_none() && state.fingerprint.is_some(),
        state.committing,
        state.imported,
        state.details,
    );
    crate::edit_mode::label(world, parent, &message, 14.0);
    crate::description::button(
        world,
        parent,
        owner,
        if committing || imported {
            "Close import status"
        } else {
            "Cancel import"
        },
        ImportAction::Cancel,
    );
    if let Some(preview) = preview {
        crate::edit_mode::label(
            world,
            parent,
            &format!(
                "{} new · {} reusable · {} conflicts",
                preview["created"],
                preview["reused"],
                preview["conflicts"].as_array().map_or(0, Vec::len)
            ),
            14.0,
        );
        let pane = reader::scrolling(world, parent, "Handbook import Records");
        world.get_mut::<Node>(pane).unwrap().max_height = px(180);
        for conflict in preview["conflicts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            crate::edit_mode::label(world, pane, conflict, 14.0);
        }
        for (index, record) in preview["records"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let head = record["head"].as_str().unwrap_or("");
            let slug = record["slug"].as_str().unwrap_or("");
            let uid = record["projection"]["uid"].as_str().unwrap_or("");
            let assertions = record["projection"]["assertions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|assertion| {
                    let predicate = assertion["predicate"].as_str().unwrap_or("");
                    let prefix = if assertion["identity"].as_bool() == Some(true) {
                        "is "
                    } else {
                        ""
                    };
                    let object = assertion["object"]["title"]
                        .as_str()
                        .map(|title| format!(" → {title}"))
                        .unwrap_or_default();
                    let quantity = assertion["quantity"]
                        .as_str()
                        .map(|amount| format!(": {amount}"))
                        .unwrap_or_default();
                    let unit = assertion["unit"]
                        .as_str()
                        .map(|unit| format!(" @{unit}"))
                        .unwrap_or_default();
                    format!("{prefix}#{predicate}{object}{quantity}{unit}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            let quantity = record["projection"]["quantity"]
                .as_array()
                .map(|value| {
                    format!(
                        "{}{}",
                        value[0].as_str().unwrap_or("0"),
                        value[1]
                            .as_str()
                            .map(|unit| format!(" @{unit}"))
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_else(|| "No quantity".into());
            crate::edit_mode::label(
                world,
                pane,
                &format!(
                    "{head} · @{slug} · {uid}\nQuantity: {}\nAssertions and identity: {}",
                    quantity, assertions
                ),
                13.0,
            );
            crate::description::button(
                world,
                pane,
                owner,
                if details == Some(index) {
                    "Hide text"
                } else {
                    "Review text"
                },
                Details(index),
            );
            if details == Some(index) {
                crate::edit_mode::label(world, pane, record["body"].as_str().unwrap_or(""), 13.0);
            }
            if imported {
                crate::description::button(
                    world,
                    pane,
                    owner,
                    "Inspect Record",
                    Inspect(uid.into()),
                );
            }
        }
        let concepts = preview["concepts"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, uid)| format!("#{name} · {}", uid.as_str().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");
        crate::edit_mode::label(
            world,
            pane,
            &format!(
                "Lince Instinct Vocabulary: {}\nConcepts and units:\n{}",
                preview["vocabulary"].as_str().unwrap_or(""),
                concepts
            ),
            13.0,
        );
        if ready && preview["conflicts"].as_array().is_some_and(Vec::is_empty) {
            crate::description::button(
                world,
                parent,
                owner,
                "Import these Records",
                ImportAction::Commit,
            );
        }
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let messages: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Import>>()
        .iter(world)
        .collect();
    for owner in owners {
        if world.get::<Import>(owner).is_some_and(|state| {
            state.request.is_some()
                && state
                    .started
                    .is_some_and(|started| started.elapsed().as_secs() >= 30)
        }) {
            let mut state = world.get_mut::<Import>(owner).unwrap();
            state.request = None;
            state.fingerprint = None;
            state.message = if state.committing { "The Cell has not confirmed the import. It may still finish. Preview again to check before resubmitting." } else { "Preview timed out. The database was not changed. Preview again when the Cell is available." }.into();
            if let Some(timer) = state.timer.take() {
                timer.abort();
            }
            drop(state);
            reader::render(world, owner);
        }
        for message in &messages {
            let request = world
                .get::<Import>(owner)
                .and_then(|state| state.request.as_ref());
            match message {
                ServerMessage::ActionOk {
                    id,
                    data: Some(data),
                    ..
                } if request == Some(id) => {
                    if data["fingerprint"].as_str().is_none()
                        && let Some(source) = crate::practice_cells::source(world, owner)
                    {
                        let records = engine::instinct::records().unwrap_or_default();
                        if let Some(mut cells) =
                            world.get_resource_mut::<crate::practice_cells::PracticeCells>()
                        {
                            cells
                                .records
                                .entry(source)
                                .or_default()
                                .extend(records.into_iter().map(|record| record.projection.uid));
                        }
                    }
                    let mut state = world.get_mut::<Import>(owner).unwrap();
                    state.request = None;
                    if let Some(timer) = state.timer.take() {
                        timer.abort();
                    }
                    state.fingerprint = data["fingerprint"].as_str().map(str::to_owned);
                    if state.fingerprint.is_some() {
                        state.preview = Some(data.clone());
                    }
                    state.imported = state.fingerprint.is_none();
                    state.committing = false;
                    state.message = if state.fingerprint.is_some() {
                        "Review the exact Records below. Cancel makes no changes.".into()
                    } else {
                        format!(
                            "Imported {} Records; reused {}. Reading and learning progress are separate.",
                            data["created"], data["reused"]
                        )
                    };
                    reader::render(world, owner);
                }
                ServerMessage::Error { id, message, .. } if request == Some(id) => {
                    let mut state = world.get_mut::<Import>(owner).unwrap();
                    state.request = None;
                    if let Some(timer) = state.timer.take() {
                        timer.abort();
                    }
                    state.fingerprint = None;
                    state.committing = false;
                    state.message = message.clone();
                    reader::render(world, owner);
                }
                _ => {}
            }
        }
    }
}
