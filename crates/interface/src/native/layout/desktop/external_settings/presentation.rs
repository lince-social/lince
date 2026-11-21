// non-lean-yet
// Scope: struct Presentation, struct View, struct Binding, fn refresh.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 151, 152, 161.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (89, 100).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 107.

use super::*;
use bevy::ecs::query::QueryState;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Resource)]
struct Presentation {
    pages: QueryState<Entity, With<Page>>,
    pending: Vec<Entity>,
    snapshot: Snapshot,
    summary_source: Option<(bool, usize)>,
    summary: String,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query_filtered(),
            pending: Vec::new(),
            snapshot: Snapshot::default(),
            summary_source: None,
            summary: String::new(),
        }
    }
}

impl Presentation {
    fn observe(&mut self, world: &World) {
        if let Some(snapshot) = world.get_resource::<Snapshot>() {
            if self.snapshot != *snapshot {
                self.snapshot.clone_from(snapshot);
                engine::performance::add("external-settings-snapshots-prepared", 1);
            }
        } else if self.snapshot != Snapshot::default() {
            self.snapshot = Snapshot::default();
            engine::performance::add("external-settings-snapshots-prepared", 1);
        }
        let source = (self.snapshot.available, self.snapshot.settings.0.len());
        if self.summary_source != Some(source) {
            self.summary = if source.0 {
                format!(
                    "{} custom commands. Screenshots use Grim + Slurp unless overridden. {} actions available.",
                    source.1,
                    ExternalAction::ALL.len()
                )
            } else {
                "External commands are configured by a Park session.".into()
            };
            self.summary_source = Some(source);
            engine::performance::add("settings-summaries-prepared", 1);
        }
    }
}

#[derive(Default)]
pub(super) struct View {
    binding: Option<Binding>,
    unconfigured: Option<(ExternalAction, String)>,
}

struct Binding {
    action: ExternalAction,
    settings: Option<crate::native::shortcuts::Settings>,
    labels: Option<Arc<BTreeMap<String, crate::host::input::KeyLabel>>>,
    text: String,
}

impl View {
    fn binding(&mut self, world: &World, action: ExternalAction) -> &str {
        let settings = world.get_resource::<crate::native::shortcuts::Settings>();
        let keyboard = world.get_resource::<crate::host::input::KeyboardSnapshot>();
        let labels = keyboard.map(|keyboard| &keyboard.labels);
        if self.binding.as_ref().is_none_or(|previous| {
            previous.action != action
                || previous.settings.as_ref() != settings
                || previous.labels.as_ref() != labels
        }) {
            let source = settings.cloned().unwrap_or_default();
            self.binding = Some(Binding {
                action,
                settings: settings.cloned(),
                labels: labels.cloned(),
                text: format!(
                    "Edit binding · {}",
                    Shortcut::External(action).caption(&source, keyboard)
                ),
            });
            engine::performance::add("external-settings-bindings-prepared", 1);
        }
        &self.binding.as_ref().unwrap().text
    }

    fn unconfigured(&mut self, action: ExternalAction) -> &str {
        if self
            .unconfigured
            .as_ref()
            .is_none_or(|(previous, _)| *previous != action)
        {
            self.unconfigured = Some((action, format!("{} is not configured.", action.label())));
        }
        &self.unconfigured.as_ref().unwrap().1
    }
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("external-settings-presentation");
    world.init_resource::<Presentation>();
    world.resource_scope(|world, mut presentation: Mut<Presentation>| {
        presentation.pending.clear();
        let Presentation { pages, pending, .. } = &mut *presentation;
        pending.extend(pages.iter(world));
        if pending.is_empty() {
            presentation.snapshot = Snapshot::default();
            presentation.summary_source = None;
            presentation.summary.clear();
            return;
        }
        presentation.observe(world);
        render(world, &presentation);
    });
}

fn render(world: &mut World, presentation: &Presentation) {
    let snapshot = &presentation.snapshot;
    for &entity in &presentation.pending {
        let Some(mut page) = take(world, entity) else {
            continue;
        };
        if page.origin.is_none() && snapshot.available {
            load(world, entity, &mut page, snapshot);
        }
        text(world, page.summary, &presentation.summary);
        if !page.expanded && page.pending.is_none() {
            restore(world, entity, page);
            continue;
        }
        let mut preparation = std::mem::take(&mut page.preparation);
        let candidate = preparation
            .prepare(world, &mut page)
            .as_ref()
            .and_then(draft::Capture::validated);
        if let Some((id, saved)) = page.pending.take() {
            if let Some(response) = snapshot
                .response
                .as_ref()
                .filter(|response| response.request == id)
            {
                page.pending = None;
                match &response.outcome {
                    InputOutcome::Saved { warning } => {
                        page.alert = warning.is_some();
                        page.origin = Some((snapshot.revision, snapshot.settings.clone()));
                        page.message = warning.clone().unwrap_or_else(|| {
                            if candidate == Ok(&saved) {
                                "External commands saved.".into()
                            } else {
                                "Saved; newer edits remain unsaved.".into()
                            }
                        });
                    }
                    InputOutcome::Failed(error) => {
                        page.message = error.clone();
                        page.alert = true;
                    }
                    InputOutcome::Pending => page.pending = Some((id, saved)),
                }
            } else if snapshot.last_request > id {
                page.pending = None;
                page.message =
                    "Another editor changed commands; your draft is preserved. Cancel to reload."
                        .into();
                page.alert = true;
            } else {
                page.pending = Some((id, saved));
            }
        } else if let Some((revision, origin)) = &page.origin
            && *revision != snapshot.revision
        {
            if candidate == Ok(origin) {
                load(world, entity, &mut page, snapshot);
            } else {
                page.message =
                    "Commands changed elsewhere; your draft is preserved. Cancel to reload.".into();
                page.alert = true;
            }
        }
        let allow_locked = page
            .draft
            .0
            .get(&page.selected)
            .is_some_and(|command| command.allow_when_locked);
        caption(
            world,
            page.locked,
            if !page.selected.lockable() {
                "Unavailable while locked"
            } else if allow_locked {
                "Allow while locked: on"
            } else {
                "Allow while locked: off"
            },
        );
        disable(
            world,
            page.locked.0,
            !snapshot.available || !page.selected.lockable(),
        );
        let binding = page.presentation.binding(world, page.selected);
        caption(world, page.binding, binding);
        let stale = page
            .origin
            .as_ref()
            .is_none_or(|(revision, _)| *revision != snapshot.revision);
        disable(
            world,
            page.save,
            !snapshot.available || page.pending.is_some() || candidate.is_err() || stale,
        );
        disable(world, page.cancel, page.pending.is_some());
        for input in [
            page.program,
            page.directory,
            page.capture_directory,
            page.capture_pointer.0,
            page.capture_defaults,
            page.capture_custom,
        ]
        .into_iter()
        .chain(page.arguments.iter().copied())
        {
            disable(world, input, !snapshot.available);
        }
        let dirty = candidate.is_ok_and(|candidate| {
            page.origin
                .as_ref()
                .is_some_and(|(_, origin)| candidate != origin)
        });
        let message = snapshot
            .message
            .as_deref()
            .or_else(|| candidate.err().map(String::as_str))
            .unwrap_or_else(|| {
                if page.alert || page.pending.is_some() {
                    &page.message
                } else if dirty {
                    "Unsaved command changes. Save to apply; no program has been run."
                } else if page
                    .draft
                    .0
                    .get(&page.selected)
                    .is_none_or(|command| command.command.program.is_empty())
                {
                    page.presentation.unconfigured(page.selected)
                } else if page.message.is_empty() {
                    "Saved locally. Editing never runs the command."
                } else {
                    &page.message
                }
            });
        text(world, page.status, message);
        page.preparation = preparation;
        restore(world, entity, page);
    }
}
