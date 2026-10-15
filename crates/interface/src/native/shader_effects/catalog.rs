use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn decode(value: &Value) -> Option<Extension> {
    serde_json::from_value::<Extension>(value.clone())
        .ok()
        .filter(Extension::bounded)
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Draft {
    pub base: Extension,
    pub selected: String,
    explicit: bool,
    drafts: BTreeMap<String, Effect>,
    working: BTreeMap<String, Effect>,
    labels: BTreeMap<String, String>,
    removed: BTreeSet<String>,
    default_override: Option<String>,
    new_id: String,
    new_label: String,
    #[serde(skip)]
    pub pending: Option<Extension>,
}

impl Draft {
    pub fn new(revision: u64, effect: Effect) -> Self {
        Self {
            base: Extension::single(revision, effect),
            selected: "main".into(),
            explicit: false,
            drafts: default(),
            working: default(),
            labels: default(),
            removed: default(),
            default_override: None,
            new_id: "effect1".into(),
            new_label: "Effect 1".into(),
            pending: None,
        }
    }

    pub fn ids(&self) -> BTreeSet<String> {
        self.base
            .effects
            .keys()
            .chain(self.drafts.keys())
            .filter(|id| !self.removed.contains(*id))
            .cloned()
            .collect()
    }

    pub fn choose(&mut self, id: String) {
        self.selected = id;
        self.explicit = true;
    }

    pub fn active(&self) -> Option<&Effect> {
        if self.removed.contains(&self.selected) {
            return None;
        }
        self.drafts
            .get(&self.selected)
            .or_else(|| self.base.selected(Some(&self.selected)))
    }

    pub fn capture(&mut self, effect: &Effect) {
        if self.active().is_some() {
            self.drafts.insert(self.selected.clone(), effect.clone());
        }
    }

    pub fn capture_working(&mut self, effect: &Effect, working: Option<&Effect>) {
        if let Some(working) = working
            && (working == effect
                || self.base.selected(Some(&self.selected)) == Some(working)
                || self.working.get(&self.selected) == Some(working))
        {
            self.working.insert(self.selected.clone(), working.clone());
        }
    }

    pub fn working_target(&self) -> Effect {
        self.working
            .get(&self.selected)
            .or_else(|| self.base.selected(Some(&self.selected)))
            .or_else(|| self.active().filter(|effect| effect.validate().is_ok()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn dirty(&self, effect: &Effect) -> bool {
        self.active().is_some() && self.base.selected(Some(&self.selected)) != Some(effect)
            || self.drafts.iter().any(|(id, effect)| {
                !self.removed.contains(id) && self.base.selected(Some(id)) != Some(effect)
            })
            || self.labels.iter().any(|(id, label)| {
                self.base.effects.get(id).map(|named| &named.label) != Some(label)
            })
            || self
                .removed
                .iter()
                .any(|id| self.base.effects.contains_key(id))
            || self
                .default_override
                .as_ref()
                .is_some_and(|id| id != &self.base.default_effect)
    }

    pub fn valid(&self) -> bool {
        self.base.validate().is_ok()
            && model::valid_effect_id(&self.selected)
            && self.ids().len() <= 8
            && self.drafts.len() <= 8
            && self
                .drafts
                .iter()
                .all(|(id, effect)| model::valid_effect_id(id) && draft_valid(effect))
            && self.working.len() <= 8
            && self
                .working
                .iter()
                .all(|(id, effect)| model::valid_effect_id(id) && effect.validate().is_ok())
            && self.labels.len() <= 8
            && self
                .labels
                .iter()
                .all(|(id, label)| model::valid_effect_id(id) && label.len() <= 512)
            && self.removed.len() <= 8
            && self.removed.iter().all(|id| model::valid_effect_id(id))
            && self
                .default_override
                .as_ref()
                .is_none_or(|id| model::valid_effect_id(id))
            && self.new_id.len() <= 512
            && self.new_label.len() <= 512
    }

    pub fn install(&mut self, extension: Extension, reload_active: bool) {
        if !self.explicit {
            self.selected = extension.default_effect.clone();
        }
        if reload_active {
            self.drafts.remove(&self.selected);
            self.labels.remove(&self.selected);
            self.working.remove(&self.selected);
        } else {
            self.drafts.clear();
            self.labels.clear();
            self.working.clear();
        }
        self.removed.clear();
        self.default_override = None;
        self.base = extension;
    }

    pub fn payload(&self, effect: Effect, revision: u64) -> Result<Extension, String> {
        if self.active().is_none() {
            return Err("Choose an existing effect or create a new effect before saving.".into());
        }
        let mut extension = self.base.clone();
        extension.revision = revision;
        for id in &self.removed {
            extension.effects.remove(id);
        }
        let label = self
            .labels
            .get(&self.selected)
            .cloned()
            .or_else(|| {
                self.base
                    .effects
                    .get(&self.selected)
                    .map(|named| named.label.clone())
            })
            .unwrap_or_else(|| "Effect".into());
        extension
            .effects
            .insert(self.selected.clone(), model::NamedEffect { label, effect });
        if let Some(id) = &self.default_override {
            extension.default_effect = id.clone();
        }
        extension.validate()?;
        Ok(extension)
    }

    pub fn acknowledge(&mut self) {
        if let Some(extension) = self.pending.take() {
            self.drafts
                .retain(|id, effect| extension.selected(Some(id)) != Some(effect));
            self.labels.retain(|id, label| {
                extension.effects.get(id).map(|named| &named.label) != Some(label)
            });
            self.base = extension;
            self.removed.clear();
            self.default_override = None;
        }
    }
}

pub(super) fn load(world: &mut World, owner: Entity, extension: Extension, reload_active: bool) {
    let mut editor = world.get_mut::<Editor>(owner).unwrap();
    editor.catalog.install(extension, reload_active);
    editor.revision = editor.catalog.base.revision;
    editor.effect = editor.catalog.active().cloned().unwrap_or_default();
    editor.original = editor
        .catalog
        .base
        .selected(Some(&editor.catalog.selected))
        .cloned()
        .unwrap_or_default();
    if !editor.effect.programs.contains_key(&editor.selected) {
        editor.selected = editor
            .effect
            .programs
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
    }
    editor.rebuild = true;
    let missing = editor.catalog.active().is_none();
    let preview = editor.preview;
    let bootstrap = editor.catalog.working_target();
    editor.bootstrap = Some(bootstrap.clone());
    if missing {
        editor.message =
            "Selected effect is unavailable. Choose another effect or create one.".into();
    }
    let mut data = world.get_mut::<shader::EffectPreview>(preview).unwrap();
    data.disabled = missing;
    data.frozen = false;
    data.effect = bootstrap;
}

#[derive(Clone)]
enum CatalogAction {
    Select(String),
    Create,
    Remove,
    Default,
    UseHere,
}

impl Action for CatalogAction {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world.get::<Editor>(owner).is_none() {
            return;
        }
        capture(world, owner);
        if world.get::<Editor>(owner).unwrap().pending.is_some() {
            return;
        }
        if matches!(self, Self::UseHere) {
            let id = world.get::<Editor>(owner).unwrap().catalog.selected.clone();
            placement::select_effect(world, owner, Some(id));
            render_editor(world, owner);
            return;
        }
        let mut editor = world.get_mut::<Editor>(owner).unwrap();
        let active = editor.effect.clone();
        editor.catalog.capture(&active);
        let mut selected = false;
        match self {
            Self::Select(id) => {
                if editor.catalog.ids().contains(id) {
                    editor.catalog.selected = id.clone();
                    editor.catalog.explicit = true;
                    selected = true;
                }
            }
            Self::Create => {
                let id = editor.catalog.new_id.trim().to_owned();
                let label = editor.catalog.new_label.clone();
                if editor.catalog.ids().len() >= 8
                    || !model::valid_effect_id(&id)
                    || editor.catalog.ids().contains(&id)
                    || label.trim().is_empty()
                    || label.len() > 64
                    || label.chars().any(char::is_control)
                {
                    editor.message = "Choose a new stable ID and readable label; catalogs support at most eight effects.".into();
                } else {
                    editor.catalog.drafts.insert(id.clone(), active);
                    editor.catalog.labels.insert(id.clone(), label);
                    editor.catalog.removed.remove(&id);
                    editor.catalog.selected = id;
                    editor.catalog.explicit = true;
                    editor.catalog.new_id = (1..=9)
                        .map(|index| format!("effect{index}"))
                        .find(|id| !editor.catalog.ids().contains(id))
                        .unwrap_or_default();
                    selected = true;
                }
            }
            Self::Remove => {
                if editor.catalog.ids().len() <= 1 {
                    editor.message = "Keep at least one effect in the catalog.".into();
                } else {
                    let id = editor.catalog.selected.clone();
                    editor.catalog.removed.insert(id.clone());
                    editor.catalog.drafts.remove(&id);
                    editor.catalog.working.remove(&id);
                    editor.catalog.labels.remove(&id);
                    let next = editor.catalog.ids().into_iter().next().unwrap();
                    if editor
                        .catalog
                        .default_override
                        .as_ref()
                        .unwrap_or(&editor.catalog.base.default_effect)
                        == &id
                    {
                        editor.catalog.default_override = Some(next.clone());
                    }
                    editor.catalog.selected = next;
                    editor.catalog.explicit = true;
                    selected = true;
                }
            }
            Self::Default => {
                editor.catalog.default_override = Some(editor.catalog.selected.clone())
            }
            Self::UseHere => {}
        }
        if selected {
            editor.effect = editor.catalog.active().cloned().unwrap_or_default();
            editor.original = editor
                .catalog
                .base
                .selected(Some(&editor.catalog.selected))
                .cloned()
                .unwrap_or_default();
            if !editor.effect.programs.contains_key(&editor.selected) {
                editor.selected = editor
                    .effect
                    .programs
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_default();
            }
        }
        let preview = editor.preview;
        let bootstrap = editor.catalog.working_target();
        if selected {
            editor.bootstrap = Some(bootstrap.clone());
        }
        if selected {
            let mut data = world.get_mut::<shader::EffectPreview>(preview).unwrap();
            data.disabled = false;
            data.frozen = false;
            data.effect = bootstrap;
        }
        render_editor(world, owner);
    }
}

pub(super) fn controls(world: &mut World, owner: Entity, panel: Entity) {
    let editor = world.get::<Editor>(owner).unwrap();
    if !matches!(editor.target, Target::Record(_)) {
        return;
    }
    let state = editor.catalog.clone();
    crate::native::edit_mode::label(world, panel, "Record effects", 16.0);
    for id in state.ids() {
        let label = state
            .labels
            .get(&id)
            .cloned()
            .or_else(|| state.base.effects.get(&id).map(|named| named.label.clone()))
            .unwrap_or_else(|| id.clone());
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            &format!(
                "{}{} · {id}",
                if id == state.selected {
                    "Selected: "
                } else {
                    ""
                },
                label
            ),
            CatalogAction::Select(id),
        );
    }
    if state.active().is_some() {
        let label = state
            .labels
            .get(&state.selected)
            .or_else(|| {
                state
                    .base
                    .effects
                    .get(&state.selected)
                    .map(|named| &named.label)
            })
            .cloned()
            .unwrap_or_default();
        input(
            world,
            panel,
            owner,
            InputKey::EffectLabel,
            "Effect label",
            &label,
            false,
        );
    }
    input(
        world,
        panel,
        owner,
        InputKey::NewEffectId,
        "New effect ID",
        &state.new_id,
        false,
    );
    input(
        world,
        panel,
        owner,
        InputKey::NewEffectLabel,
        "New effect label",
        &state.new_label,
        false,
    );
    for (caption, action) in [
        ("Create effect from selected", CatalogAction::Create),
        ("Remove selected effect", CatalogAction::Remove),
        (
            "Make selected effect the Record default",
            CatalogAction::Default,
        ),
        ("Use selected effect in this Castle", CatalogAction::UseHere),
    ] {
        crate::native::sand_panel::button(world, panel, owner, caption, action);
    }
    crate::native::edit_mode::label(
        world,
        panel,
        "Save / Apply publishes the selected effect, removals and default choice. Other effect drafts stay local. An explicit missing selection stays unavailable.",
        12.0,
    );
}

pub(super) fn edit(world: &mut World, owner: Entity, key: &InputKey, value: String) {
    let mut editor = world.get_mut::<Editor>(owner).unwrap();
    match key {
        InputKey::EffectLabel => {
            let id = editor.catalog.selected.clone();
            editor.catalog.labels.insert(id, value);
        }
        InputKey::NewEffectId => editor.catalog.new_id = value,
        InputKey::NewEffectLabel => editor.catalog.new_label = value,
        _ => {}
    }
}

pub(super) fn reference_status(world: &World, surface: &Surface) -> String {
    let Some(uid) = &surface.record else {
        return String::new();
    };
    if surface.quantity_only {
        return String::new();
    }
    let binding = RecordBinding {
        area: Entity::PLACEHOLDER,
        uid: uid.clone(),
        source: surface.source.clone(),
    };
    let extension = world
        .get_resource::<Feeds>()
        .and_then(|feeds| feeds.0.get(&feed_key(&binding)))
        .and_then(|feed| feed.row.as_ref())
        .and_then(|row| serde_json::from_value::<Extension>(row["shader_effect"].clone()).ok());
    if extension
        .as_ref()
        .and_then(|extension| extension.selected(surface.effect_id.as_deref()))
        .is_none()
    {
        "Referenced effect is unavailable".into()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    async fn until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                shader::prepare_test_previews(app.world_mut());
                app.update();
                if condition(app.world_mut()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn press(world: &mut World, owner: Entity, caption: &str) {
        let button = world
            .query::<(
                &crate::native::actions::ActionButton,
                &crate::native::icons::Tooltip,
            )>()
            .iter(world)
            .find(|(button, tooltip)| button.target == owner && tooltip.0 == caption)
            .unwrap()
            .0
            .clone();
        button.actions.run(world, owner);
    }

    fn fill(world: &mut World, owner: Entity, label: &str, value: &str) {
        let entity = world
            .query::<(Entity, &Input, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .find(|(_, input, node)| input.editor == owner && node.label() == Some(label))
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
    }

    #[tokio::test]
    async fn native_catalog_creation_saving_switching_and_removal_preserve_independent_effects() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let organ = store::organs::ensure_local(&engine.store.pool, "http://shader-catalog")
            .await
            .unwrap()
            .uid;
        engine
            .set_signer(engine::trust::Signer::generate(&organ, "key"))
            .await
            .unwrap();
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Catalog".into(),
                    body: "Keep description".into(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<crate::native::tokens::ThemeSettings>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
                commands: default(),
                engine: engine.clone(),
                store: engine.store.clone(),
                lanes: Arc::new(cell::LaneHub::new()),
                wire: default(),
                fiote: None,
                speech: None,
                information: None,
            }))
            .add_plugins((
                crate::native::cell_bridge::CellBridgePlugin,
                crate::native::protein_area::ProteinAreaPlugin,
                crate::native::record_binding::RecordBindingPlugin,
                crate::native::description::DescriptionPlugin,
                ShaderEffectsPlugin,
            ));
        let root = app
            .world_mut()
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let mut owner = open(
            app.world_mut(),
            root,
            Target::Record(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: uid.clone(),
                source: Source::Local,
            }),
            Effect::default(),
            0,
            None,
        );
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            editor.loaded && shader::working(world, editor.preview) == Some(&editor.effect)
        })
        .await;
        assert_eq!(app.world().get::<Editor>(owner).unwrap().revision, 0);
        fill(app.world_mut(), owner, "New effect ID", "calm");
        fill(app.world_mut(), owner, "New effect label", "Calm");
        press(app.world_mut(), owner, "Create effect from selected");
        Command::Preset(model::CALM.into()).apply(app.world_mut(), owner);
        press(app.world_mut(), owner, "Add numeric input");
        fill(
            app.world_mut(),
            owner,
            "Record UID (empty uses this Record or the Sand's reference)",
            &uid,
        );
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            shader::working(world, editor.preview) == Some(&editor.effect)
        })
        .await;
        let calm = app.world().get::<Editor>(owner).unwrap().effect.clone();
        Command::Save.apply(app.world_mut(), owner);
        until(&mut app, |world| {
            world.get::<Editor>(owner).unwrap().revision == 1
        })
        .await;
        let stored = store::records::get_extension(&engine.store.pool, &uid, model::NAMESPACE)
            .await
            .unwrap()
            .unwrap();
        let saved: Extension = serde_json::from_value(stored).unwrap();
        assert_eq!(saved.effects.len(), 2);
        assert_eq!(saved.selected(Some("calm")), Some(&calm));
        let main = saved.selected(Some("main")).unwrap().clone();
        fill(
            app.world_mut(),
            owner,
            "WGSL: fn shade(uv, inputs) → vec4<f32>",
            "invalid wgsl",
        );
        app.update();
        capture(app.world_mut(), owner);
        let invalid = app.world().get::<Editor>(owner).unwrap().effect.clone();
        assert!(invalid.validate().is_err());
        press(app.world_mut(), owner, "Main · main");
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            shader::working(world, editor.preview) == Some(&main)
        })
        .await;
        fill(app.world_mut(), owner, "Effect label", "Renamed main");
        Command::Save.apply(app.world_mut(), owner);
        until(&mut app, |world| {
            world.get::<Editor>(owner).unwrap().revision == 2
        })
        .await;
        let stored = store::records::get_extension(&engine.store.pool, &uid, model::NAMESPACE)
            .await
            .unwrap()
            .unwrap();
        let saved: Extension = serde_json::from_value(stored).unwrap();
        assert_eq!(saved.effects["main"].label, "Renamed main");
        assert_eq!(saved.selected(Some("calm")), Some(&calm));
        press(app.world_mut(), owner, "Calm · calm");
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            editor.effect == invalid && shader::working(world, editor.preview) == Some(&calm)
        })
        .await;
        app.world_mut()
            .entity_mut(owner)
            .insert(crate::native::workspace::WorkspaceMember(1));
        let saved = capture_editor(app.world_mut(), owner).unwrap();
        let saved: SavedEditor =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert!(saved.valid());
        app.world_mut().despawn(owner);
        owner = saved.restore_owner(app.world_mut(), root, None);
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            editor.loaded
                && editor.catalog.selected == "calm"
                && editor.effect == invalid
                && shader::working(world, editor.preview) == Some(&calm)
                && !world
                    .get::<shader::EffectPreview>(editor.preview)
                    .unwrap()
                    .error
                    .is_empty()
                && world
                    .get::<shader::EffectPreview>(editor.preview)
                    .unwrap()
                    .numeric
                    .get(0)
                    == Some("-1")
        })
        .await;
        assert_eq!(
            app.world().get::<Editor>(owner).unwrap().effect.bindings[0]
                .source
                .record
                .as_deref(),
            Some(uid.as_str())
        );
        press(app.world_mut(), owner, "Remove selected effect");
        until(&mut app, |world| {
            let editor = world.get::<Editor>(owner).unwrap();
            shader::working(world, editor.preview) == Some(&editor.effect)
        })
        .await;
        Command::Save.apply(app.world_mut(), owner);
        until(&mut app, |world| {
            world.get::<Editor>(owner).unwrap().revision == 3
        })
        .await;
        let stored = store::records::get_extension(&engine.store.pool, &uid, model::NAMESPACE)
            .await
            .unwrap()
            .unwrap();
        let saved: Extension = serde_json::from_value(stored).unwrap();
        assert_eq!(saved.effects.len(), 1);
        assert!(saved.selected(Some("calm")).is_none());
        assert_eq!(saved.selected(None), Some(&main));
    }

    fn catalog() -> Extension {
        let mut extension = Extension::single(4, Effect::default());
        let mut calm = Effect::default();
        calm.programs = [("calm".into(), model::CALM.into())].into();
        calm.rules.clear();
        calm.fallback = "calm".into();
        extension.effects.insert(
            "calm".into(),
            model::NamedEffect {
                label: "Calm".into(),
                effect: calm,
            },
        );
        extension
    }

    #[test]
    fn selected_save_preserves_siblings_and_their_invalid_local_drafts() {
        let extension = catalog();
        let mut draft = Draft::new(0, Effect::default());
        draft.install(extension.clone(), false);
        draft.choose("calm".into());
        let working = draft.active().unwrap().clone();
        draft.capture_working(&working, Some(&working));
        let mut invalid = working.clone();
        invalid
            .programs
            .insert("calm".into(), "invalid wgsl".into());
        draft.capture(&invalid);
        draft.labels.insert("calm".into(), "Local label".into());
        draft.choose("main".into());
        let mut changed = draft.active().unwrap().clone();
        changed
            .programs
            .insert(changed.fallback.clone(), model::GRADIENT.into());
        draft.capture(&changed);
        let payload = draft.payload(changed.clone(), 5).unwrap();
        assert_eq!(payload.effects["calm"], extension.effects["calm"]);
        assert_eq!(payload.selected(Some("main")), Some(&changed));
        draft.pending = Some(payload);
        draft.acknowledge();
        assert!(draft.dirty(&changed));
        draft.choose("calm".into());
        assert_eq!(draft.active(), Some(&invalid));
        assert_eq!(draft.working_target(), working);
        assert_eq!(draft.labels["calm"], "Local label");
        let saved = serde_json::to_value(&draft).unwrap();
        let restored: Draft = serde_json::from_value(saved).unwrap();
        assert!(restored.valid());
        assert_eq!(restored.active(), Some(&invalid));
        assert_eq!(restored.working_target(), working);
    }

    #[test]
    fn reload_is_scoped_and_deleted_explicit_selection_does_not_fall_back() {
        let extension = catalog();
        let mut draft = Draft::new(0, Effect::default());
        draft.install(extension.clone(), false);
        let main = draft.active().unwrap().clone();
        let mut changed = main.clone();
        changed
            .programs
            .insert(changed.fallback.clone(), model::RIPPLE.into());
        draft.capture(&changed);
        draft.choose("calm".into());
        let mut invalid = draft.active().unwrap().clone();
        invalid.programs.clear();
        draft.capture(&invalid);
        draft.install(extension.clone(), true);
        assert_eq!(draft.active(), extension.selected(Some("calm")));
        assert_eq!(draft.drafts.get("main"), Some(&changed));
        let mut deleted = extension;
        deleted.effects.remove("calm");
        deleted.revision += 1;
        draft.install(deleted, true);
        assert_eq!(draft.selected, "calm");
        assert!(draft.active().is_none());
        assert!(draft.payload(main, 6).is_err());
    }

    #[test]
    fn clean_remote_updates_replace_cached_working_definitions() {
        let mut extension = catalog();
        let mut draft = Draft::new(0, Effect::default());
        draft.install(extension.clone(), false);
        let old = draft.active().unwrap().clone();
        draft.capture_working(&old, Some(&old));
        let effect = &mut extension.effects.get_mut("main").unwrap().effect;
        effect
            .programs
            .insert(effect.fallback.clone(), model::GRADIENT.into());
        extension.revision += 1;
        draft.install(extension.clone(), false);
        assert_eq!(&draft.working_target(), extension.selected(None).unwrap());
        assert_ne!(draft.working_target(), old);
    }
}
