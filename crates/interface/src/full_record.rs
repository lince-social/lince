use crate::{
    actions::{Action, ActionButton},
    protein_area::{Binding, Config, RecordBinding, Source},
};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use serde_json::json;

pub fn config(reference: &str, source: Source) -> Config {
    let mut config = Config {
        enabled: true,
        show_labels: true,
        source,
        width: 520.0,
        columns: 1,
        bindings: protein::record_schema::fields()
            .into_iter()
            .map(|field| Binding {
                editable: field.editable,
                width: 488.0,
                overflow: crate::protein_area::OverflowMode::GrowDown,
                height: if matches!(field.key, "body" | "threads") {
                    160.0
                } else {
                    48.0
                },
                ..Binding::new(field.key)
            })
            .collect(),
        ..Default::default()
    };
    let predicate = if nucleus::valid_uid(reference, "r") {
        json!({"uid_eq":reference})
    } else {
        json!({"slug_eq":reference.trim_start_matches('#')})
    };
    config.draft.name = "Record".into();
    config.draft.query =
        json!({"source":"record","where":[{"all":[predicate]}],"order":[],"limit":1});
    config
}

pub fn open(world: &mut World, root: Entity, reference: &str, source: Source) -> Option<Entity> {
    if reference.trim().is_empty() {
        return None;
    }
    let workspace = world.get::<crate::workspace::Workspaces>(root)?.active;
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    let mut area = crate::area::InfluenceArea::new(
        crate::area::AreaShape::Polygon(vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ]),
        position,
        DVec2::new(560.0, 720.0),
    );
    area.name = "Record Castle".into();
    area.protein = Some(config(reference, source));
    crate::area::spawn_area(world, root, workspace, area)
}

#[derive(Clone)]
pub struct Open(pub RecordBinding);

impl Action for Open {
    fn apply(&self, world: &mut World, _: Entity) {
        let mut cursor = Some(self.0.area);
        while let Some(entity) = cursor {
            if world.get::<crate::workspace::Workspaces>(entity).is_some() {
                open(world, entity, &self.0.uid, self.0.source.clone());
                return;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

#[derive(Component)]
struct Reference(Entity);

#[derive(Clone)]
struct AddCastle;

#[derive(Clone)]
struct AddThreads;

impl Action for AddThreads {
    fn apply(&self, world: &mut World, root: Entity) {
        let reference = world
            .get::<Reference>(root)
            .and_then(|reference| world.get::<EditableText>(reference.0))
            .map(|input| input.value().to_string())
            .unwrap_or_default();
        if crate::thread_castle::open(world, root, reference.trim(), Source::Local).is_none() {
            crate::notifications::report(
                world,
                "interface::threads",
                "Enter a Record slug or identity to open its threads.",
            );
        }
    }
}

impl Action for AddCastle {
    fn apply(&self, world: &mut World, root: Entity) {
        let input = world.get::<Reference>(root).map(|reference| reference.0);
        let reference = input
            .and_then(|entity| world.get::<EditableText>(entity))
            .map(|text| text.value().to_string())
            .unwrap_or_default();
        if open(world, root, reference.trim(), Source::Local).is_none() {
            crate::notifications::report(
                world,
                "interface::record",
                "Enter a Record slug or identity to open its Castle.",
            );
        }
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::edit_mode::label(world, parent, "Record Castle", 18.0);
    let input = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(parent),
            crate::icons::Tooltip("Record slug or identity".into()),
        ))
        .id();
    world.entity_mut(root).insert(Reference(input));
    let button = world
        .spawn((
            crate::sand::Square,
            ActionButton::new(root, crate::actions![AddCastle]),
            ChildOf(parent),
        ))
        .id();
    crate::edit_mode::label(world, button, "Open Record", 16.0);
    crate::description::button(world, parent, root, "Open threads", AddThreads);
}
