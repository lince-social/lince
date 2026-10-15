use super::*;

#[derive(Component)]
struct PracticeLaboratory {
    root: Entity,
}

#[derive(Clone, Copy)]
struct Recover {
    root: Entity,
    command: Command,
}

#[cfg(feature = "instinct")]
pub(super) fn recover(world: &mut World, root: Entity, lab_root: Entity, command: Command) {
    crate::native::actions::ActionSequence::default()
        .then(Recover { root, command })
        .run(world, lab_root);
}

impl Action for Recover {
    fn practice_intent(&self) -> crate::native::actions::PracticeIntent {
        crate::native::actions::PracticeIntent::Recovery
    }
    fn apply(&self, world: &mut World, _: Entity) {
        return_to_workspace(world, self.root);
        self.command.apply(world, self.root);
    }
}

pub(super) fn open(world: &mut World, root: Entity) {
    if world
        .get::<Practice>(root)
        .unwrap()
        .results
        .contains_key(&Operation::InspectLaboratory)
        || crate::native::laboratory::active(world)
    {
        return;
    }
    crate::native::laboratory::LaboratoryAction::Open.apply(world, root);
    let Some(lab_root) = world
        .resource::<crate::native::laboratory::Laboratory>()
        .root
    else {
        return;
    };
    world
        .entity_mut(lab_root)
        .insert(PracticeLaboratory { root });
    crate::native::laboratory::LaboratoryAction::Resources.apply(world, lab_root);
    let controls = world
        .spawn((
            ChildOf(lab_root),
            Recovery,
            GlobalZIndex(150),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(12),
                left: px(12),
                right: px(12),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                ..default()
            },
        ))
        .id();
    crate::native::edit_mode::label(
        world,
        controls,
        "Instinct: inspect the captured Sand resources. Normal workspaces are suspended; these controls always return you to the tutorial.",
        14.0,
    );
    let row = crate::native::sand_panel::row(world, controls);
    for (title, command) in [
        ("Next", Command::Next),
        ("Skip", Command::Skip),
        ("Free", Command::Free),
        ("Assisted", Command::Assisted),
        ("Close tutorial", Command::Close),
    ] {
        crate::native::description::button(world, row, lab_root, title, Recover { root, command });
    }
}

pub(crate) fn closed(world: &mut World) {
    let Some(lab_root) = world
        .get_resource::<crate::native::laboratory::Laboratory>()
        .and_then(|lab| lab.root)
    else {
        return;
    };
    let Some(root) = world
        .get::<PracticeLaboratory>(lab_root)
        .map(|context| context.root)
    else {
        return;
    };
    let visible = crate::native::laboratory::resources_visible(world);
    let snapshot = &world
        .resource::<crate::native::laboratory::Laboratory>()
        .resources;
    if visible && !snapshot.sands.is_empty() {
        let result =
            serde_json::json!({"sands":snapshot.sands.len(),"graphics":snapshot.graphics.status});
        if let Some(mut practice) = world.get_mut::<Practice>(root) {
            practice
                .results
                .insert(Operation::InspectLaboratory, result);
        }
    }
    world.entity_mut(lab_root).remove::<PracticeLaboratory>();
}

pub(super) fn return_to_workspace(world: &mut World, root: Entity) {
    if world
        .get_resource::<crate::native::laboratory::Laboratory>()
        .and_then(|lab| lab.root)
        .and_then(|lab| world.get::<PracticeLaboratory>(lab))
        .is_some_and(|context| context.root == root)
    {
        crate::native::laboratory::close(world);
    }
}
