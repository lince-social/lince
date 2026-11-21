// non-lean-yet
// Scope: struct Source, struct Presentation, fn candidate, fn refresh.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 28, 29, 185, 187,
//   218.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (153, 183, 195, 217, 236, 243).

use super::*;
use crate::host::input::InputResponse;
use bevy::ecs::query::QueryState;
use std::collections::HashMap;

struct Source {
    revision: u64,
    settings: KeyboardSettings,
    response: Option<InputResponse>,
    active: String,
    repeat: [String; 2],
    summary: String,
}

impl Source {
    fn read(previous: &mut Option<Self>, snapshot: Option<&KeyboardSnapshot>) {
        let Some(snapshot) = snapshot else {
            *previous = None;
            return;
        };
        let active = snapshot
            .layouts
            .get(snapshot.active_layout as usize)
            .map_or("system default", String::as_str);
        let fresh = previous.is_none();
        let source = previous.get_or_insert_with(|| Self {
            revision: snapshot.revision,
            settings: snapshot.settings.clone(),
            response: snapshot.response.clone(),
            active: active.to_owned(),
            repeat: Default::default(),
            summary: String::new(),
        });
        let rates_changed = fresh
            || source.settings.repeat_rate != snapshot.settings.repeat_rate
            || source.settings.repeat_delay_ms != snapshot.settings.repeat_delay_ms;
        let active_changed = source.active != active;
        if rates_changed {
            source.repeat = [
                snapshot.settings.repeat_rate.to_string(),
                snapshot.settings.repeat_delay_ms.to_string(),
            ];
        }
        if fresh || rates_changed || active_changed {
            source.summary = format!(
                "Active layout: {active}. Repeat: {} Hz after {} ms.",
                snapshot.settings.repeat_rate, snapshot.settings.repeat_delay_ms
            );
            engine::performance::add("settings-summaries-prepared", 1);
        }
        if source.settings != snapshot.settings {
            source.settings.clone_from(&snapshot.settings);
        }
        if source.response != snapshot.response {
            source.response.clone_from(&snapshot.response);
        }
        if active_changed {
            active.clone_into(&mut source.active);
        }
        source.revision = snapshot.revision;
    }

    fn write_fields(&self, world: &mut World, fields: [Entity; 5]) {
        write_values(
            world,
            fields,
            [
                &self.settings.layout,
                &self.settings.variant,
                &self.settings.options,
                &self.repeat[0],
                &self.repeat[1],
            ],
        );
    }
}

#[derive(Resource)]
struct Presentation {
    pages: QueryState<(Entity, &'static Page)>,
    pending: Vec<Entity>,
    drafts: HashMap<Entity, prepared::Prepared<(), KeyboardSettings>>,
    source: Option<Source>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query(),
            pending: Vec::new(),
            drafts: HashMap::new(),
            source: None,
        }
    }
}

pub(super) fn candidate(
    world: &World,
    page: Entity,
    fields: [Entity; 5],
) -> Option<&Result<KeyboardSettings, String>> {
    world
        .get_resource::<Presentation>()?
        .drafts
        .get(&page)?
        .get(world, fields, &())
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("keyboard-settings-presentation");
    let mut presentation = world
        .remove_resource::<Presentation>()
        .unwrap_or_else(|| Presentation::from_world(world));
    render(world, &mut presentation);
    world.insert_resource(presentation);
}

fn render(world: &mut World, presentation: &mut Presentation) {
    let changed = world.is_resource_changed::<KeyboardSnapshot>();
    presentation.pending.clear();
    presentation.pending.extend(
        presentation
            .pages
            .iter(world)
            .filter_map(|(entity, state)| {
                (state.refresh
                    || changed
                    || state.fields.iter().any(|field| {
                        world
                            .get_entity(*field)
                            .ok()
                            .and_then(|entity| {
                                entity
                                    .get_ref::<EditableText>()
                                    .map(|text| text.is_changed())
                            })
                            .unwrap_or(false)
                    }))
                .then_some(entity)
            }),
    );
    presentation
        .drafts
        .retain(|page, _| world.get::<Page>(*page).is_some());
    if presentation.pending.is_empty() {
        return;
    }
    Source::read(
        &mut presentation.source,
        world.get_resource::<KeyboardSnapshot>(),
    );
    for page in presentation.pending.drain(..) {
        let state = world.get::<Page>(page).unwrap();
        let (fields, summary, status, save, cancel) = (
            state.fields,
            state.summary,
            state.status,
            state.save,
            state.cancel,
        );
        let draft = presentation
            .drafts
            .entry(page)
            .or_default()
            .prepare(world, fields, &(), || read_fields(world, fields))
            .as_ref()
            .ok();
        if let Some(source) = &presentation.source {
            let response = state.pending.as_ref().and_then(|(request, _)| {
                source
                    .response
                    .as_ref()
                    .filter(|response| response.request == *request)
            });
            if let Some(response) = response {
                let unchanged = draft == state.pending.as_ref().map(|(_, settings)| settings);
                match &response.outcome {
                    InputOutcome::Pending => {}
                    InputOutcome::Saved { warning } => {
                        if unchanged {
                            source.write_fields(world, fields);
                        }
                        let mut state = world.get_mut::<Page>(page).unwrap();
                        state.pending = None;
                        state.origin = Some((source.revision, source.settings.clone()));
                        let mut message = warning
                            .clone()
                            .unwrap_or_else(|| "Keyboard settings saved.".into());
                        if !unchanged {
                            message.push_str(" Newer edits remain unsaved in this draft.");
                        }
                        text(world, status, message);
                    }
                    InputOutcome::Failed(reason) => {
                        world.get_mut::<Page>(page).unwrap().pending = None;
                        text(world, status, reason);
                    }
                }
            } else if state.pending.is_none() {
                let clean = state
                    .origin
                    .as_ref()
                    .is_none_or(|(_, settings)| draft == Some(settings));
                let origin_matches = state
                    .origin
                    .as_ref()
                    .is_some_and(|(_, settings)| settings == &source.settings);
                let revision_matches = state
                    .origin
                    .as_ref()
                    .is_some_and(|(revision, _)| *revision == source.revision);
                if clean || origin_matches {
                    if clean {
                        source.write_fields(world, fields);
                    }
                    if !origin_matches || !revision_matches {
                        world.get_mut::<Page>(page).unwrap().origin =
                            Some((source.revision, source.settings.clone()));
                    }
                } else if !revision_matches {
                    text(
                        world,
                        status,
                        "The keyboard changed while editing. Your draft is preserved; Cancel loads the active settings.",
                    );
                }
            }
            text(world, summary, &source.summary);
        } else {
            text(
                world,
                summary,
                "Keyboard settings are available in a Park session. Sandbox uses its host compositor's keyboard.",
            );
        }
        let busy = world.get::<Page>(page).unwrap().pending.is_some();
        let available = presentation.source.is_some() && world.contains_resource::<HostRequests>();
        disable(world, save, !available || busy);
        disable(world, cancel, !available || busy);
        for field in fields {
            disable(world, field, !available);
        }
        world.get_mut::<Page>(page).unwrap().refresh = false;
    }
}
