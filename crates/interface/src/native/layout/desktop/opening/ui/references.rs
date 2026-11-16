use super::*;
use bevy::ecs::query::QueryState;

#[derive(Default)]
struct Selection {
    counts: BTreeMap<String, usize>,
    ids: BTreeSet<String>,
}

impl Selection {
    fn replace<'a>(
        &mut self,
        previous: impl IntoIterator<Item = &'a str>,
        current: impl IntoIterator<Item = &'a str>,
    ) {
        let mut changes = BTreeMap::<&str, isize>::new();
        for uid in previous {
            *changes.entry(uid).or_default() -= 1;
        }
        for uid in current {
            *changes.entry(uid).or_default() += 1;
        }
        for (uid, change) in changes {
            if change == 0 {
                continue;
            }
            let previous = self.counts.get(uid).copied().unwrap_or_default();
            let count = previous
                .checked_add_signed(change)
                .expect("Opening reference sources must balance");
            if count == 0 {
                self.counts.remove(uid);
                self.ids.remove(uid);
                engine::performance::add("opening-reference-keys-changed", 1);
            } else if previous == 0 {
                self.counts.insert(uid.to_owned(), count);
                self.ids.insert(uid.to_owned());
                engine::performance::add("opening-reference-keys-changed", 1);
            } else {
                *self.counts.get_mut(uid).unwrap() = count;
            }
        }
        engine::performance::add("opening-reference-sources-changed", 1);
    }
}

struct EditorSource {
    window: bool,
    uid: String,
}

pub(in crate::native::layout::desktop::opening) struct References {
    pages: QueryState<(Entity, &'static Page)>,
    editors: QueryState<(Entity, &'static Editor)>,
    controls: QueryState<(Entity, &'static WindowControls)>,
    page_sources: HashMap<Entity, Vec<String>>,
    editor_sources: HashMap<Entity, EditorSource>,
    control_sources: HashMap<Entity, String>,
    active: Vec<String>,
    captured_windows: BTreeSet<String>,
    rules: Selection,
    windows: Selection,
}

impl FromWorld for References {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query(),
            editors: world.query(),
            controls: world.query(),
            page_sources: HashMap::new(),
            editor_sources: HashMap::new(),
            control_sources: HashMap::new(),
            active: Vec::new(),
            captured_windows: BTreeSet::new(),
            rules: Selection::default(),
            windows: Selection::default(),
        }
    }
}

impl References {
    pub(in crate::native::layout::desktop::opening) fn rules(&self) -> &BTreeSet<String> {
        &self.rules.ids
    }
    pub(in crate::native::layout::desktop::opening) fn windows(&self) -> &BTreeSet<String> {
        &self.windows.ids
    }

    pub(in crate::native::layout::desktop::opening) fn refresh(
        &mut self,
        world: &World,
        windows: &BTreeSet<String>,
    ) {
        let _selection = engine::performance::Span::new("opening-reference-selection");
        let active = active_rules(world);
        if self.active != active {
            self.rules.replace(
                self.active.iter().map(String::as_str),
                active.iter().map(String::as_str),
            );
            self.active.resize_with(active.len(), String::new);
            for (previous, current) in self.active.iter_mut().zip(active) {
                if previous != current {
                    previous.clone_from(current);
                }
            }
        }
        self.captured_windows.retain(|uid| {
            if windows.contains(uid) {
                return true;
            }
            self.windows.replace([uid.as_str()], []);
            false
        });
        for uid in windows {
            if !self.captured_windows.contains(uid) {
                self.windows.replace([], [uid.as_str()]);
                self.captured_windows.insert(uid.clone());
            }
        }
        self.page_sources.retain(|page, previous| {
            if world.get::<Page>(*page).is_some() {
                return true;
            }
            self.rules.replace(previous.iter().map(String::as_str), []);
            false
        });
        for (page, state) in self.pages.iter(world) {
            let previous = self.page_sources.entry(page).or_default();
            if *previous != state.order {
                self.rules.replace(
                    previous.iter().map(String::as_str),
                    state.order.iter().map(String::as_str),
                );
                previous.clone_from(&state.order);
            }
        }
        self.editor_sources.retain(|owner, previous| {
            if editor::state(world, *owner).is_some_and(|state| state.uid.is_some()) {
                return true;
            }
            let selection = if previous.window {
                &mut self.windows
            } else {
                &mut self.rules
            };
            selection.replace([previous.uid.as_str()], []);
            false
        });
        for (owner, editor) in self.editors.iter(world) {
            let Some(state) = editor.0.as_ref() else {
                continue;
            };
            let Some(uid) = &state.uid else {
                continue;
            };
            let window = matches!(&state.kind, Kind::Window);
            if self
                .editor_sources
                .get(&owner)
                .is_some_and(|previous| previous.window == window && previous.uid == *uid)
            {
                continue;
            }
            if let Some(previous) = self.editor_sources.remove(&owner) {
                let selection = if previous.window {
                    &mut self.windows
                } else {
                    &mut self.rules
                };
                selection.replace([previous.uid.as_str()], []);
            }
            let selection = if window {
                &mut self.windows
            } else {
                &mut self.rules
            };
            selection.replace([], [uid.as_str()]);
            self.editor_sources.insert(
                owner,
                EditorSource {
                    window,
                    uid: uid.clone(),
                },
            );
        }
        self.control_sources.retain(|panel, previous| {
            if world
                .get::<WindowControls>(*panel)
                .is_some_and(|controls| !controls.uid.is_empty())
            {
                return true;
            }
            self.windows.replace([previous.as_str()], []);
            false
        });
        for (panel, controls) in self.controls.iter(world) {
            if controls.uid.is_empty() {
                continue;
            }
            let previous = self.control_sources.entry(panel).or_default();
            if *previous != controls.uid {
                self.windows.replace(
                    (!previous.is_empty()).then_some(previous.as_str()),
                    [controls.uid.as_str()],
                );
                previous.clone_from(&controls.uid);
            }
        }
    }
}
