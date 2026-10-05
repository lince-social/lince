use super::*;

#[derive(Clone, Default)]
pub(super) struct Restoration {
    previous_view: Option<crate::topology::view::View>,
    owned_view: Option<crate::topology::view::View>,
    previous_inspection: Option<crate::inspection::Inspection>,
    owned_inspection: Option<crate::inspection::Inspection>,
}

#[derive(Default)]
pub(super) struct State {
    initialized: bool,
    pub(super) restoration: Restoration,
}

pub(super) fn handles(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::AddShaderExample
            | Operation::SwitchSpatialView
            | Operation::ReturnFlatView
            | Operation::SaveCustomCastle
            | Operation::AddCustomCastle
            | Operation::InspectControl
            | Operation::InspectInformation
            | Operation::InspectSandCredits
            | Operation::InspectLaboratory
    )
}

pub(super) fn prepare(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    let subject = practice.runner.lesson.subject;
    if !matches!(
        subject,
        "shaders" | "topology" | "custom-castles" | "inspection" | "information" | "laboratory"
    ) {
        return;
    }
    if subject == "shaders" && !practice.records.contains(&practice.note) {
        if practice.setup.is_none() && !practice.records.is_empty() && practice.exercise.is_none() {
            let _ = lessons::begin_record(world, root, Operation::AddShaderExample);
        }
        return;
    }
    if subject == "custom-castles" && sample_directory(world, root).is_none() {
        return;
    }
    if !practice.visual.initialized {
        let previous_view = world.get::<crate::topology::view::View>(root).copied();
        let previous_inspection = world.get::<crate::inspection::Inspection>(root).cloned();
        let mut practice = world.get_mut::<Practice>(root).unwrap();
        practice.visual.initialized = true;
        practice.visual.restoration.previous_view = previous_view;
        practice.visual.restoration.previous_inspection = previous_inspection;
        drop(practice);
        if subject == "custom-castles" {
            crate::custom_castle::clear_example(world, root);
        }
    }
    if find(world, root, Role::Feature).is_none() {
        let practice = world.get::<Practice>(root).unwrap();
        let workspace = practice.workspace;
        let source = practice.source.clone();
        let owner = match subject {
            "shaders" => {
                let owner = crate::shader_castle::spawn(
                    world,
                    root,
                    workspace,
                    DVec2::new(800.0, 0.0),
                    "instinct-note",
                );
                crate::shader_castle::prepare_source(
                    world,
                    owner,
                    crate::protein_area::Source::Organ(source),
                );
                owner
            }
            "information" => {
                crate::ide::spawn(world, root, workspace, DVec2::new(800.0, 0.0), default())
            }
            "inspection" => {
                pair(world, root);
                let square = find(world, root, Role::Square).unwrap();
                world
                    .spawn((
                        crate::sand::button(0),
                        crate::actions::ActionButton::new(
                            square,
                            crate::actions![crate::sand_placement::PlacementAction::Pin],
                        ),
                        ChildOf(root),
                        WorkspaceMember(workspace),
                        crate::canvas::CanvasItem {
                            position: DVec2::new(440.0, 0.0),
                            size: Vec2::new(180.0, 60.0),
                        },
                    ))
                    .id()
            }
            _ => {
                pair(world, root);
                find(world, root, Role::Square).unwrap()
            }
        };
        if subject == "inspection" {
            crate::edit_mode::label(world, owner, "Pin sample", 14.0);
        }
        own(world, root, owner, Role::Feature);
        if subject == "custom-castles" {
            let text = find(world, root, Role::Text).unwrap();
            world
                .entity_mut(root)
                .insert(crate::canvas_selection::SandSelection(vec![owner, text]));
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Store.apply(world, root);
            remember_edit(world, root);
        }
    }
    if world.get::<Practice>(root).unwrap().pending.is_some()
        && let Some(operation) = world
            .get::<Practice>(root)
            .unwrap()
            .runner
            .current()
            .and_then(|step| step.operation)
    {
        let _ = execute(world, root, operation);
    }
}

fn remember_edit(world: &mut World, root: Entity) {
    let revision = world
        .get::<crate::edit_mode::EditMode>(root)
        .map(|mode| mode.revision);
    world.get_mut::<Practice>(root).unwrap().owned_edit_revision = revision;
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    let Some(owner) = find(world, root, Role::Feature) else {
        return Ok(());
    };
    match operation {
        Operation::AddShaderExample => crate::shader_castle::add_example(world, owner),
        Operation::SwitchSpatialView | Operation::ReturnFlatView => {
            let spatial = operation == Operation::SwitchSpatialView;
            if world
                .get::<crate::topology::view::View>(root)
                .is_none_or(|view| view.spatial != spatial)
            {
                crate::topology::ui::TopologyAction::ToggleView.apply(world, root);
                crate::canvas_controls::CanvasAction::Recenter.apply(world, root);
                let view = world.get::<crate::topology::view::View>(root).copied();
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .visual
                    .restoration
                    .owned_view = view;
            }
        }
        Operation::SaveCustomCastle => crate::custom_castle::save_example(world, root),
        Operation::AddCustomCastle => crate::custom_castle::add_example(world, root),
        Operation::InspectControl => {
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::General.apply(world, root);
            crate::inspection::inspect_control(world, root, owner);
            let state = world.get::<crate::inspection::Inspection>(root).cloned();
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .visual
                .restoration
                .owned_inspection = state;
            remember_edit(world, root);
        }
        Operation::InspectInformation => {
            crate::information::OpenInformation.apply(world, root);
            remember_edit(world, root);
        }
        Operation::InspectSandCredits => crate::sand_panel::show_credits(world, owner),
        Operation::InspectLaboratory => super::laboratory_practice::open(world, root),
        _ => return Err("This visual lesson is unavailable.".into()),
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    let Some(owner) = find(world, root, Role::Feature) else {
        return false;
    };
    match operation {
        Operation::AddShaderExample => crate::shader_castle::example_visible(world, owner),
        Operation::SwitchSpatialView => world
            .get::<crate::topology::view::View>(root)
            .is_some_and(|view| view.spatial),
        Operation::ReturnFlatView => world
            .get::<crate::topology::view::View>(root)
            .is_some_and(|view| !view.spatial),
        Operation::SaveCustomCastle => crate::custom_castle::saved_example(world, root),
        Operation::AddCustomCastle => crate::custom_castle::added_example(world, root),
        Operation::InspectControl => {
            world
                .get::<crate::inspection::Inspection>(root)
                .is_some_and(|state| state.click && state.selected == Some(owner))
                && !crate::inspection::connections(world, owner).is_empty()
        }
        Operation::InspectInformation => {
            world
                .get::<crate::edit_mode::EditMode>(root)
                .is_some_and(|mode| mode.enabled)
                && crate::information::panel_visible(world, root)
        }
        Operation::InspectSandCredits => crate::sand_panel::credits_visible(world, owner),
        Operation::InspectLaboratory => world
            .get::<Practice>(root)
            .unwrap()
            .results
            .contains_key(&operation),
        _ => false,
    }
}

pub(super) fn restore(world: &mut World, root: Entity) {
    if let Some(restoration) = world
        .get::<Practice>(root)
        .map(|practice| practice.visual.restoration.clone())
    {
        restore_value(world, root, restoration);
    }
}

pub(super) fn source_only(world: &mut World, root: Entity, operation: Operation) -> bool {
    operation == Operation::AddShaderExample
        && !world.contains_resource::<bevy::render::renderer::RenderAdapterInfo>()
        && complete(world, root, operation)
}

pub(super) fn restore_value(world: &mut World, root: Entity, restoration: Restoration) {
    if world.get_entity(root).is_err() {
        return;
    }
    if restoration
        .owned_view
        .as_ref()
        .is_some_and(|view| world.get::<crate::topology::view::View>(root) == Some(view))
    {
        if let Some(view) = restoration.previous_view {
            world.entity_mut(root).insert(view);
        } else {
            world
                .entity_mut(root)
                .remove::<crate::topology::view::View>();
        }
    }
    if restoration
        .owned_inspection
        .as_ref()
        .is_some_and(|state| world.get::<crate::inspection::Inspection>(root) == Some(state))
    {
        if let Some(state) = restoration.previous_inspection {
            world.entity_mut(root).insert(state);
        } else {
            world
                .entity_mut(root)
                .remove::<crate::inspection::Inspection>();
        }
    }
    crate::custom_castle::clear_example(world, root);
}
