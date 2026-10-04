mod fields;
#[cfg(test)]
mod tests;
mod ui;

use super::*;
use crate::actions::Action;
use lince_interface::presentation::{Layout, Presentation, compare};
use lince_interface::settings::Values;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct State {
    pub view: Option<Presentation>,
    pub fields: BTreeMap<String, Values>,
    #[serde(default)]
    pub settings: Values,
    #[serde(default)]
    pub tokens: crate::tokens::TokenOverrides,
    #[serde(default)]
    pub part_tokens: BTreeMap<String, crate::tokens::TokenOverrides>,
}

impl State {
    pub(crate) fn valid(&self) -> bool {
        self.view.as_ref().is_none_or(Presentation::valid)
            && self
                .settings
                .valid(&lince_interface::presentation::settings())
            && self.tokens.validate()
            && self.part_tokens.len() <= 32
            && self.part_tokens.iter().all(|(field, tokens)| {
                protein::record_schema::fields()
                    .iter()
                    .any(|definition| definition.key == field)
                    && tokens.validate()
            })
            && self.fields.len() <= 32
            && self.fields.iter().all(|(field, values)| {
                protein::record_schema::fields()
                    .iter()
                    .any(|definition| definition.key == field)
                    && values.valid(&fields::definitions(field))
            })
    }
}

#[derive(Component, Clone)]
struct Session {
    owner: Entity,
    target: Presentation,
    include_missing: bool,
    keep_extra: bool,
    observed: Probe,
    message: String,
}

#[derive(Clone, PartialEq)]
struct Probe {
    fields: Vec<String>,
    rows: Vec<(String, Value)>,
    drafts: Vec<Draft>,
    saved: Option<Presentation>,
    query: Value,
}

#[derive(Clone, PartialEq)]
pub(super) struct Draft {
    pub(super) uid: String,
    pub(super) field: String,
    pub(super) text: String,
    pub(super) dirty: bool,
    pub(super) busy: bool,
}

#[derive(Clone)]
enum Previous {
    Row(Option<Presentation>),
    Area(Option<Presentation>, BTreeMap<String, Option<Presentation>>),
    Created(Entity),
}

#[derive(Component, Default)]
struct History(Vec<(Previous, Presentation)>);

#[derive(Component)]
pub(super) struct EditorSettings(pub(super) Values);

pub(crate) fn current(world: &World, row: Entity) -> Option<Presentation> {
    let mut view = raw(world, row)?;
    if let Some(state) = world.get::<State>(row) {
        view.settings.0.extend(state.settings.0.clone());
    }
    Some(view)
}

fn raw(world: &World, row: Entity) -> Option<Presentation> {
    world
        .get::<State>(row)
        .and_then(|state| state.view.clone())
        .or_else(|| {
            let binding = world.get::<RecordBinding>(row)?;
            world
                .get::<InfluenceArea>(binding.area)?
                .protein
                .as_ref()?
                .presentation
                .clone()
        })
}

fn base(world: &World, owner: Entity) -> Option<Config> {
    if let Some(editor) = world.get::<QueryEditor>(owner) {
        return base(world, editor.0);
    }
    if let Some(castle) = world.get::<ProteinCastle>(owner) {
        let query = castle.draft.compile().ok()?;
        if query.source != protein::Source::Record || query.aggregate.is_some() {
            return None;
        }
        let fields = world
            .get::<crate::protein_castle::ProteinResults>(owner)
            .filter(|results| results.current && !results.columns.is_empty())
            .map(|results| results.columns.clone())
            .or(query.fields)
            .unwrap_or_else(|| vec!["head".into(), "body".into()]);
        let bindings: Vec<_> = fields
            .into_iter()
            .filter(|field| {
                protein::record_schema::fields()
                    .iter()
                    .any(|definition| definition.key == field)
            })
            .map(|field| Binding::new(&field))
            .collect();
        return Some(Config {
            draft: castle.draft.clone(),
            bindings: if bindings.is_empty() {
                vec![Binding::new("head")]
            } else {
                bindings
            },
            ..default()
        });
    }
    let area = world
        .get::<RecordBinding>(owner)
        .map_or(owner, |binding| binding.area);
    world.get::<InfluenceArea>(area)?.protein.clone()
}

fn rows(world: &World, owner: Entity) -> Vec<Entity> {
    if let Some(editor) = world.get::<QueryEditor>(owner) {
        return rows(world, editor.0);
    }
    if world.get::<RecordBinding>(owner).is_some() {
        return vec![owner];
    }
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&owner))
        .map(|state| {
            state
                .row_entities
                .values()
                .copied()
                .filter(|entity| world.get_entity(*entity).is_ok())
                .collect()
        })
        .unwrap_or_default()
}

fn supported(world: &World, owner: Entity) -> bool {
    let binding = world.get::<RecordBinding>(owner);
    let area = binding.map_or(owner, |binding| binding.area);
    world
        .get::<InfluenceArea>(area)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| {
            !config.fiote
                && config.command.is_none()
                && !config.relations
                && world.get::<filter::Subscription>(owner).is_none()
                && !(config.record_cards
                    && world
                        .get::<super::rows::Row>(owner)
                        .is_some_and(|row| row.data["kind"] == "command"))
        })
}

fn probe(world: &World, owner: Entity) -> Option<Probe> {
    if let Some(root) = root(world, owner)
        && let Some(member) = world.get::<crate::workspace::WorkspaceMember>(owner)
        && world
            .get::<crate::workspace::Workspaces>(root)
            .is_some_and(|spaces| member.0 != spaces.active)
    {
        return None;
    }
    let config = base(world, owner)?;
    if config.fiote
        || config.command.is_some()
        || config.relations
        || world.get::<filter::Subscription>(owner).is_some()
    {
        return None;
    }
    let saved = if world.get::<RecordBinding>(owner).is_some() {
        current(world, owner)
    } else {
        config.presentation.clone()
    };
    let mut original = saved
        .as_ref()
        .map(|view| view.fields.clone())
        .unwrap_or_else(|| {
            config
                .bindings
                .iter()
                .map(|binding| binding.property.clone())
                .collect()
        });
    let mut data = Vec::new();
    if world.get::<QueryEditor>(owner).is_none()
        && world.get::<ProteinCastle>(owner).is_some()
        && let Some(results) = world
            .get::<crate::protein_castle::ProteinResults>(owner)
            .filter(|results| results.current)
    {
        data.extend(results.rows.iter().enumerate().map(|(index, value)| {
            (
                value["uid"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| (index + 1).to_string()),
                value.clone(),
            )
        }));
    }
    let mut drafts = Vec::new();
    let runtime = world.get_resource::<Runtime>();
    let mut entities = rows(world, owner);
    entities.sort();
    for row in entities {
        let binding = world.get::<RecordBinding>(row)?;
        if world.get::<RecordBinding>(owner).is_none()
            && let Some(view) = current(world, row)
        {
            for field in view.fields {
                if !original.contains(&field) {
                    original.push(field);
                }
            }
        }
        let value = runtime
            .and_then(|runtime| runtime.areas.get(&binding.area))
            .and_then(|state| {
                state
                    .data
                    .iter()
                    .find(|value| value["uid"].as_str() == Some(&binding.uid))
            })
            .cloned()
            .unwrap_or(Value::Null);
        if config.record_cards && value["kind"] == "command" {
            return None;
        }
        data.push((binding.uid.clone(), value));
        drafts.extend(super::rows::drafts(world, row));
    }
    Some(Probe {
        fields: original,
        rows: data,
        drafts,
        saved,
        query: serde_json::to_value(&config.draft).ok()?,
    })
}

fn root(world: &World, owner: Entity) -> Option<Entity> {
    let mut cursor = Some(owner);
    while let Some(entity) = cursor {
        if world.get::<crate::edit_mode::EditMode>(entity).is_some() {
            return Some(entity);
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    None
}

#[derive(Clone)]
pub(crate) struct Open(pub Entity);
impl Action for Open {
    fn apply(&self, world: &mut World, _: Entity) {
        let Some(root) = root(world, self.0) else {
            return;
        };
        let Some(observed) = probe(world, self.0) else {
            if world.get::<ProteinCastle>(self.0).is_some() {
                crate::protein_castle::status(
                    world,
                    self.0,
                    "Presentations need a Record query without aggregation",
                );
            }
            return;
        };
        let target = observed
            .saved
            .clone()
            .unwrap_or_else(|| template("Record Castle", Layout::Castle, &observed.fields));
        world.entity_mut(root).insert(Session {
            owner: self.0,
            target,
            include_missing: false,
            keep_extra: false,
            observed,
            message: String::new(),
        });
        crate::edit_mode::show_customization(world, root);
    }
}

pub(super) fn choose_record(world: &mut World, owner: Entity) {
    Open(owner).apply(world, owner);
    let Some(root) = root(world, owner) else {
        return;
    };
    if let Some(mut session) = world.get_mut::<Session>(root) {
        session.target = template(
            "Record Castle",
            Layout::Castle,
            &Config::records()
                .bindings
                .iter()
                .map(|binding| binding.property.clone())
                .collect::<Vec<_>>(),
        );
        crate::edit_mode::render_panel(world, root);
    }
}

fn template(name: &str, layout: Layout, fields: &[impl AsRef<str>]) -> Presentation {
    Presentation {
        name: name.into(),
        layout,
        fields: fields.iter().map(|field| field.as_ref().into()).collect(),
        fills: Default::default(),
        settings: lince_interface::settings::Values(BTreeMap::from([(
            "labels".into(),
            lince_interface::settings::Value::Toggle(layout == Layout::Castle),
        )])),
    }
}

fn result(session: &Session) -> Presentation {
    let comparison = compare(&session.observed.fields, &session.target.fields);
    let mut view = session.target.clone();
    if !session.include_missing {
        view.fields
            .retain(|field| !comparison.missing.contains(field));
    }
    if session.keep_extra {
        view.fields.extend(comparison.extra);
    }
    if let Some(old) = &session.observed.saved {
        for (field, value) in &old.fills {
            view.fills.entry(field.clone()).or_insert(value.clone());
        }
    }
    view.fills.retain(|field, _| view.fields.contains(field));
    view
}

fn save(world: &mut World, row: Entity) {
    let Some(binding) = world.get::<RecordBinding>(row).cloned() else {
        return;
    };
    let state = capture(world, row);
    world.entity_mut(row).insert(state.clone());
    if let Some(mut area) = world.get_mut::<InfluenceArea>(binding.area) {
        area.records.entry(binding.uid).or_default().state = state;
    }
}

pub(crate) fn capture(world: &World, row: Entity) -> State {
    let mut state = world.get::<State>(row).cloned().unwrap_or_default();
    state.tokens = crate::token_style::overrides(world, row);
    fields::capture_tokens(world, row, &mut state);
    state
}

pub(crate) fn styles_changed(world: &mut World, entity: Entity) {
    if world.get::<State>(entity).is_some() {
        save(world, entity);
    } else if let Some(row) = fields::owner(world, entity) {
        save(world, row);
    }
}

fn set_view(world: &mut World, owner: Entity, view: Presentation) -> bool {
    if !view.valid() {
        return false;
    }
    let previous = if world.get::<RecordBinding>(owner).is_some() {
        let previous = world
            .get::<State>(owner)
            .and_then(|state| state.view.clone());
        world.entity_mut(owner).entry::<State>().or_default();
        world.get_mut::<State>(owner).unwrap().view = Some(view.clone());
        save(world, owner);
        Previous::Row(previous)
    } else if world.get::<ProteinCastle>(owner).is_some()
        && world.get::<QueryEditor>(owner).is_none()
    {
        let Some(mut config) = base(world, owner) else {
            return false;
        };
        let Some(root) = root(world, owner) else {
            return false;
        };
        let Some(workspace) = world
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return false;
        };
        config.presentation = Some(view.clone());
        let position = world
            .get::<crate::canvas::CanvasItem>(owner)
            .map(|item| item.position + bevy::math::DVec2::new(600.0, 0.0))
            .unwrap_or_default();
        let data = world
            .get::<crate::protein_castle::ProteinResults>(owner)
            .filter(|results| results.current)
            .map(|results| results.rows.clone())
            .unwrap_or_default();
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            position,
            bevy::math::DVec2::splat(640.0),
        );
        area.name = if config.draft.name.trim().is_empty() {
            "Protein presentation".into()
        } else {
            config.draft.name.clone()
        };
        area.protein = Some(config.clone());
        let Some(area) = crate::area::spawn_area(world, root, workspace, area) else {
            return false;
        };
        super::start(world, area, config);
        let state = world
            .resource_mut::<Runtime>()
            .into_inner()
            .areas
            .get_mut(&area)
            .unwrap();
        state.data = data;
        state.dirty = true;
        super::rows::reconcile(world, area);
        crate::protein_castle::stop_results(world, owner);
        world.entity_mut(owner).insert(QueryEditor(area));
        Previous::Created(area)
    } else {
        let area = world
            .get::<QueryEditor>(owner)
            .map_or(owner, |editor| editor.0);
        let Some(mut area) = world.get_mut::<InfluenceArea>(area) else {
            return false;
        };
        let Some(config) = area.protein.as_mut() else {
            return false;
        };
        let previous = config.presentation.replace(view.clone());
        let states = area
            .records
            .iter()
            .map(|(uid, saved)| (uid.clone(), saved.state.view.clone()))
            .collect();
        for saved in area.records.values_mut() {
            saved.state.view = None;
        }
        for row in rows(world, owner) {
            world.get_mut::<State>(row).unwrap().view = None;
        }
        Previous::Area(previous, states)
    };
    world.entity_mut(owner).entry::<History>().or_default();
    {
        let mut history = world.get_mut::<History>(owner).unwrap();
        if history.0.len() == 20 {
            history.0.remove(0);
        }
        history.0.push((previous, view));
    }
    update(world);
    true
}

#[derive(Clone)]
pub(crate) struct Undo(pub Entity);
impl Action for Undo {
    fn apply(&self, world: &mut World, _: Entity) {
        if let Some(editor) = world.get::<QueryEditor>(self.0)
            && world
                .get::<History>(editor.0)
                .is_some_and(|history| !history.0.is_empty())
        {
            Undo(editor.0).apply(world, editor.0);
            return;
        }
        let Some((previous, applied)) = world
            .get::<History>(self.0)
            .and_then(|history| history.0.last())
            .cloned()
        else {
            return;
        };
        let now = match &previous {
            Previous::Row(_) => world
                .get::<State>(self.0)
                .and_then(|state| state.view.clone()),
            Previous::Area(_, _) | Previous::Created(_) => {
                base(world, self.0).and_then(|config| config.presentation)
            }
        };
        if now.as_ref() != Some(&applied) {
            return;
        }
        match previous {
            Previous::Row(view) => {
                world.get_mut::<State>(self.0).unwrap().view = view;
                save(world, self.0);
            }
            Previous::Area(view, saved) => {
                if rows(world, self.0).iter().any(|row| {
                    world
                        .get::<State>(*row)
                        .is_some_and(|state| state.view.is_some())
                }) {
                    return;
                }
                let target = world
                    .get::<QueryEditor>(self.0)
                    .map_or(self.0, |editor| editor.0);
                let Some(mut area) = world.get_mut::<InfluenceArea>(target) else {
                    return;
                };
                area.protein.as_mut().unwrap().presentation = view;
                for (uid, record) in &mut area.records {
                    record.state.view = saved.get(uid).cloned().flatten();
                }
                for row in rows(world, self.0) {
                    let uid = world.get::<RecordBinding>(row).unwrap().uid.clone();
                    world.get_mut::<State>(row).unwrap().view = saved.get(&uid).cloned().flatten();
                }
            }
            Previous::Created(area) => {
                if probe(world, area)
                    .is_some_and(|probe| probe.drafts.iter().any(|draft| draft.dirty || draft.busy))
                    || rows(world, area).iter().any(|row| {
                        world
                            .get::<State>(*row)
                            .is_some_and(|state| state.view.is_some())
                    })
                {
                    crate::protein_castle::status(
                        world,
                        self.0,
                        "Save local Record edits and undo individual presentation changes before returning to the table",
                    );
                    return;
                }
                super::stop(world, area);
                world.despawn(area);
                world.entity_mut(self.0).remove::<QueryEditor>();
                crate::protein_castle::resume_results(world, self.0);
            }
        }
        world.get_mut::<History>(self.0).unwrap().0.pop();
        update(world);
        if let Some(root) = root(world, self.0) {
            crate::edit_mode::render_panel(world, root);
        }
    }
}

#[derive(Component)]
pub(super) struct Controls;

pub(super) fn controls(world: &mut World, row: Entity) {
    if !supported(world, row) {
        return;
    }
    let controls = crate::sand_panel::row(world, row);
    world.entity_mut(controls).insert(Controls);
    crate::sand_panel::button(world, controls, row, "Change presentation…", Open(row));
    crate::sand_panel::button(world, controls, row, "Undo presentation", Undo(row));
    if let Some(binding) = world.get::<RecordBinding>(row).cloned() {
        crate::sand_panel::button(
            world,
            controls,
            row,
            "Edit Record data…",
            crate::full_record::Open(binding),
        );
    }
}

pub(super) fn update(world: &mut World) {
    let rows: Vec<_> = world
        .query_filtered::<Entity, (With<RecordBinding>, With<super::rows::Row>)>()
        .iter(world)
        .collect();
    for row in rows {
        if !supported(world, row) {
            continue;
        }
        if world
            .get::<crate::sand_settings::Declaration>(row)
            .is_none()
        {
            fields::prepare(world, row);
        }
        if let Some(view) = current(world, row) {
            fields::arrange(world, row, &view);
        } else {
            fields::restore(world, row);
        }
    }
}

pub(crate) fn cancel(world: &mut World, root: Entity) {
    world.entity_mut(root).remove::<Session>();
}

pub(super) fn rebuilding(world: &mut World, row: Entity) {
    fields::rebuilding(world, row);
}

pub(super) fn refresh_fields(world: &mut World, row: Entity) {
    fields::refresh(world, row);
}

pub(crate) fn panel(world: &mut World, root: Entity, panel: Entity) -> bool {
    ui::panel(world, root, panel)
}

pub(super) fn inputs(world: &mut World) {
    ui::inputs(world);
}
