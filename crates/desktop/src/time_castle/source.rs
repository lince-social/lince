use super::*;
use crate::actions::{Action, ActionButton};
use crate::protein_castle::{ProteinCastle, ProteinDraft};

#[derive(Component)]
#[relationship(relationship_target = SourceEditors)]
struct EditorOf(Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = EditorOf, linked_spawn)]
struct SourceEditors(Vec<Entity>);

#[derive(Component)]
struct EditorCell(Source);

pub(crate) fn editor(world: &World, entity: Entity) -> bool {
    world.get::<EditorOf>(entity).is_some()
}

pub(super) fn open(world: &mut World, owner: Entity) {
    if world
        .get::<SourceEditors>(owner)
        .is_some_and(|editors| !editors.0.is_empty())
    {
        return;
    }
    let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
        return;
    };
    let Some(workspace) = world
        .get::<crate::workspace::WorkspaceMember>(owner)
        .copied()
    else {
        return;
    };
    let (cell, filter) = match source_query(world, owner) {
        Ok(source) => source,
        Err(error) => {
            status(world, owner, &error);
            (
                crate::practice_cells::source(world, owner).map_or(Source::Local, Source::Organ),
                Vec::new(),
            )
        }
    };
    let draft = world
        .get::<TimeSettings>(owner)
        .unwrap()
        .0
        .source
        .as_ref()
        .map(|source| source.draft.clone())
        .unwrap_or_else(|| {
            let mut query = ProteinDraft::default().compile().unwrap();
            query.filter = filter;
            ProteinDraft::from_protein("Clock schedule".into(), String::new(), query)
        });
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(bevy::math::DVec2::ZERO, |view| view.center);
    let elevation = crate::topology::spatial(world, owner).elevation + 2.0;
    chrome::close_controls(world, owner);
    let entity = crate::protein_castle::spawn(world, root, workspace.0, position, draft);
    world.entity_mut(entity).insert((
        EditorOf(owner),
        EditorCell(cell.clone()),
        crate::actions::ControlOwner(owner),
        crate::topology::Spatial {
            elevation,
            depth: Some(1.0),
            world_pinned: true,
            ..default()
        },
    ));
    if crate::practice_cells::owns_source(world, &cell)
        && let Source::Organ(cell) = cell
    {
        world
            .entity_mut(entity)
            .insert(crate::practice_cells::PracticeSource(cell));
    }
    let buttons: Vec<_> = world
        .query::<(Entity, &ActionButton, &crate::icons::IconButton)>()
        .iter(world)
        .filter(|(_, action, _)| action.target == entity)
        .map(|(button, _, _)| button)
        .collect();
    for button in buttons {
        match world.get::<crate::icons::IconButton>(button).unwrap().icon {
            crate::icons::Icon::Play => {
                world
                    .get_mut::<crate::icons::IconButton>(button)
                    .unwrap()
                    .label = "Apply schedule source".into();
                world
                    .entity_mut(button)
                    .insert(crate::icons::Tooltip("Apply schedule source".into()));
            }
            crate::icons::Icon::Stop => {
                world.despawn(button);
            }
            _ => {}
        }
    }
    crate::protein_castle::refresh_editor(world, entity);
    crate::protein_castle::status(
        world,
        entity,
        "Apply to save this source to the clock and close the editor",
    );
}

pub(crate) fn controls(world: &mut World, parent: Entity, owner: Entity) {
    let cell = world.get::<EditorCell>(owner).unwrap().0.clone();
    let caption = match &cell {
        Source::Local => "Data source: local Cell".into(),
        Source::Organ(uid) => format!("Data source: {uid}"),
    };
    crate::edit_mode::label(world, parent, &caption, 14.0);
    crate::sand_panel::button(world, parent, owner, "Use local Cell", SetLocal);
    let value = match cell {
        Source::Local => String::new(),
        Source::Organ(uid) => uid,
    };
    let input = crate::sand_panel::field(world, parent, "Cell organ UID", &value);
    crate::sand_panel::button(world, parent, owner, "Use this Cell", SetCell(input));
    crate::edit_mode::label(
        world,
        parent,
        "The clock uses these Record filters with its aperture.",
        13.0,
    );
}

pub(crate) fn run_editor(world: &mut World, entity: Entity) -> bool {
    let Some(owner) = world.get::<EditorOf>(entity).map(|editor| editor.0) else {
        return false;
    };
    let Some(draft) = world
        .get::<ProteinCastle>(entity)
        .map(|castle| castle.draft.clone())
    else {
        return true;
    };
    let cell = world.get::<EditorCell>(entity).unwrap().0.clone();
    let source = model::ScheduleSource { cell, draft };
    if !source.valid() {
        crate::protein_castle::status(
            world,
            entity,
            "Choose a valid Cell and Record query for the clock",
        );
        return true;
    }
    if crate::practice_cells::source(world, owner)
        .is_some_and(|cell| source.cell != Source::Organ(cell))
    {
        crate::protein_castle::status(world, entity, "Choose a Protein from this practice Cell.");
        return true;
    }
    if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
        settings.0.area = None;
        settings.0.source = Some(source);
        status(world, owner, "Schedule source saved");
    }
    world.despawn(entity);
    true
}

#[derive(Clone)]
struct SetLocal;
impl Action for SetLocal {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut cell) = world.get_mut::<EditorCell>(owner) {
            cell.0 = Source::Local;
            crate::protein_castle::refresh_editor(world, owner);
        }
    }
}

#[derive(Clone)]
struct SetCell(Entity);
impl Action for SetCell {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Ok(value) = crate::sand_panel::value(world, self.0) else {
            return;
        };
        let value = value.trim().to_owned();
        if value.is_empty() || value.len() > 128 {
            crate::protein_castle::status(world, owner, "Enter a Cell organ UID");
            return;
        }
        if let Some(mut cell) = world.get_mut::<EditorCell>(owner) {
            cell.0 = Source::Organ(value);
            crate::protein_castle::refresh_editor(world, owner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<bevy::input_focus::InputFocus>();
        let root = world.spawn(crate::container::BoxRoot).id();
        let owner = world
            .spawn((
                Node::default(),
                ChildOf(root),
                crate::workspace::WorkspaceMember(1),
            ))
            .id();
        populate(&mut world, owner);
        (world, root, owner)
    }

    #[test]
    fn source_editor_applies_record_filters_is_temporary_and_closes_with_the_clock() {
        let (mut world, root, owner) = fixture();
        open(&mut world, owner);
        let editor = world.get::<SourceEditors>(owner).unwrap().0[0];
        assert!(!chrome::controls_open(&world, owner));
        let placement = crate::topology::spatial(&world, editor);
        assert!(placement.elevation > crate::topology::spatial(&world, owner).elevation);
        assert!(
            placement.elevation - placement.depth(Vec2::new(720.0, 680.0))
                > crate::topology::spatial(&world, owner).elevation
        );
        assert!(placement.world_pinned);
        open(&mut world, owner);
        assert_eq!(world.get::<SourceEditors>(owner).unwrap().0, vec![editor]);
        assert!(crate::protein_castle::snapshot(&mut world, root).is_empty());
        world.get_mut::<ProteinCastle>(editor).unwrap().draft.query["where"] =
            serde_json::json!([{"all":[{"text_contains":"Dental"}]}]);
        assert!(world.get::<TimeSettings>(owner).unwrap().0.source.is_none());
        assert!(run_editor(&mut world, editor));
        assert!(world.get_entity(editor).is_err());
        let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
        assert!(restored.valid());
        let (cell, filter) = source_query(&mut world, owner).unwrap();
        assert_eq!(cell, Source::Local);
        assert_eq!(
            serde_json::to_value(filter).unwrap(),
            serde_json::to_value(settings.source.unwrap().draft.compile().unwrap().filter).unwrap()
        );
        open(&mut world, owner);
        let editor = world.get::<SourceEditors>(owner).unwrap().0[0];
        world.despawn(owner);
        assert!(world.get_entity(editor).is_err());
        assert!(world.get_entity(root).is_ok());
    }

    #[test]
    fn cancelling_or_rejecting_a_source_does_not_change_the_clock() {
        let (mut world, _, owner) = fixture();
        let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        open(&mut world, owner);
        let editor = world.get::<SourceEditors>(owner).unwrap().0[0];
        world.get_mut::<ProteinCastle>(editor).unwrap().draft.query["source"] =
            serde_json::json!("concept");
        assert!(run_editor(&mut world, editor));
        assert!(world.get_entity(editor).is_ok());
        assert_eq!(world.get::<TimeSettings>(owner).unwrap().0, settings);
        crate::protein_castle::ProteinAction::Delete.apply(&mut world, editor);
        assert!(world.get_entity(editor).is_err());
        assert_eq!(world.get::<TimeSettings>(owner).unwrap().0, settings);
        assert!(
            world
                .get::<SourceEditors>(owner)
                .is_none_or(|editors| editors.0.is_empty())
        );
        open(&mut world, owner);
        let editor = world.get::<SourceEditors>(owner).unwrap().0[0];
        world
            .entity_mut(owner)
            .insert(crate::practice_cells::PracticeSource(
                "practice-cell".into(),
            ));
        world.get_mut::<EditorCell>(editor).unwrap().0 = Source::Organ("other-cell".into());
        assert!(run_editor(&mut world, editor));
        assert!(world.get_entity(editor).is_ok());
        assert_eq!(world.get::<TimeSettings>(owner).unwrap().0, settings);
    }
}
