use super::*;
use bevy::{
    input_focus::{InputFocus, tab_navigation::TabIndex},
    ui::InteractionDisabled,
};
use lince_interface::practice::{Resolution, Target, resolve};

#[derive(Component)]
pub(in crate::instinct) struct Disabled {
    root: Entity,
    pickable: Option<Pickable>,
    tab: Option<i32>,
}

#[derive(PartialEq)]
pub(in crate::instinct) struct Snapshot(Entity, u64, usize, Mode, Phase, u64, bool);

#[cfg(feature = "instinct")]
#[derive(Resource, Default)]
pub(super) struct Metrics {
    pub refreshes: usize,
}

pub(in crate::instinct) fn changed(
    practices: Query<(Entity, &Practice, &Workspaces)>,
    controls: Query<
        (),
        Or<(
            Changed<Owned>,
            Changed<SemanticControl>,
            Changed<crate::actions::ControlOwner>,
            Changed<crate::actions::TutorialControl>,
            Changed<crate::actions::TutorialField>,
            Changed<crate::edit_mode::EditControl>,
            Changed<crate::actions::ActionButton>,
            Added<bevy::text::EditableText>,
            Added<CanvasItem>,
            Added<bevy::ui_widgets::Button>,
            Added<crate::slider::SliderSand>,
            Changed<WorkspaceMember>,
            Changed<ChildOf>,
        )>,
    >,
    mut removed: RemovedComponents<ChildOf>,
    mut previous: Local<Vec<Snapshot>>,
) -> bool {
    let next: Vec<_> = practices
        .iter()
        .map(|(root, practice, spaces)| {
            Snapshot(
                root,
                practice.runner.session,
                practice.runner.step,
                practice.runner.mode,
                practice.runner.phase.clone(),
                spaces.active,
                practice.summary,
            )
        })
        .collect();
    let changed = *previous != next || !controls.is_empty() || removed.read().count() > 0;
    *previous = next;
    changed
}

pub(in crate::instinct) fn guard_focus(
    disabled: Query<(), With<Disabled>>,
    mut focus: ResMut<InputFocus>,
) {
    if focus.get().is_some_and(|entity| disabled.contains(entity)) {
        focus.clear();
    }
}

fn restore(world: &mut World, entity: Entity) {
    let Some(state) = world.entity_mut(entity).take::<Disabled>() else {
        return;
    };
    world.entity_mut(entity).remove::<InteractionDisabled>();
    if world.get::<Pickable>(entity) == Some(&Pickable::IGNORE) {
        if let Some(pickable) = state.pickable {
            world.entity_mut(entity).insert(pickable);
        } else {
            world.entity_mut(entity).remove::<Pickable>();
        }
    }
    if world.get::<TabIndex>(entity).is_some_and(|tab| tab.0 == -1) {
        if let Some(tab) = state.tab {
            world.entity_mut(entity).insert(TabIndex(tab));
        } else {
            world.entity_mut(entity).remove::<TabIndex>();
        }
    }
}

pub(super) fn release(world: &mut World, root: Entity) {
    let entities: Vec<_> = world
        .query::<(Entity, &Disabled)>()
        .iter(world)
        .filter(|(_, state)| state.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for entity in entities {
        restore(world, entity);
    }
}

pub(super) fn semantic(world: &mut World, root: Entity) -> Result<Entity, Resolution> {
    let practice = world.get::<Practice>(root).unwrap();
    let operation = practice
        .runner
        .current()
        .and_then(|step| step.operation)
        .ok_or(Resolution::Missing)?;
    if operation == Operation::OpenEdit {
        let controls: Vec<_> = world
            .query::<(Entity, &crate::edit_mode::EditControl)>()
            .iter(world)
            .filter(|(_, control)| {
                control.root == root && control.action == crate::edit_mode::EditAction::Toggle
            })
            .map(|(entity, _)| entity)
            .collect();
        return match controls.as_slice() {
            [entity] => Ok(*entity),
            [] => Err(Resolution::Missing),
            _ => Err(Resolution::Ambiguous),
        };
    }
    let owner = match operation {
        Operation::MoveSand | Operation::SetAppearance | Operation::ResetAppearance => {
            Some(Role::Square)
        }
        Operation::EditText => Some(Role::Text),
        Operation::Attract
        | Operation::Repel
        | Operation::ScaleArea
        | Operation::DisableAreaEffect
        | Operation::InspectArea => Some(Role::Area),
        Operation::PreviewProtein | Operation::PresentProperties | Operation::ArrangeProtein => {
            Some(Role::Spawn)
        }
        Operation::EnterArea | Operation::LeaveArea => Some(Role::Changes),
        Operation::ReadVocabulary
        | Operation::CompleteTask
        | Operation::UndoTask
        | Operation::MoveTask
        | Operation::StartTimer
        | Operation::StopTimer
        | Operation::OpenDatedRecord
        | Operation::SetOperation
        | Operation::CreateFrequency
        | Operation::PreviewRule
        | Operation::RunRule
        | Operation::PauseRule
        | Operation::PreviewHabit
        | Operation::ImportHabit
        | Operation::CompleteHabit
        | Operation::SaveCommand
        | Operation::RunCommand
        | Operation::StepSimulation
        | Operation::StopSimulation
        | Operation::InspectFiote
        | Operation::InspectOrgan
        | Operation::SwitchOrgan
        | Operation::PairContact
        | Operation::InspectAccess
        | Operation::ShareRecords
        | Operation::InspectDevices
        | Operation::InspectMail
        | Operation::SearchDiscovery
        | Operation::PrepareAnnouncement
        | Operation::OpenDiscoveryRequest
        | Operation::SendPracticeMessage
        | Operation::InspectCalls
        | Operation::CheckTransfer
        | Operation::AgreeTransfer
        | Operation::ActivateTransfer
        | Operation::PauseTransferRule
        | Operation::PreviewImportConflict
        | Operation::CancelImportPreview
        | Operation::PreviewCleanImport
        | Operation::ImportPreparedRecords
        | Operation::ExportPracticeFiles
        | Operation::EditPracticeFile
        | Operation::StopPracticeSync
        | Operation::SendPracticeCopy
        | Operation::AcceptPracticeCopy
        | Operation::InspectBackup
        | Operation::OpenPracticeFile
        | Operation::EditSavePracticeFile
        | Operation::InspectLanguageTools
        | Operation::NextDocumentPage
        | Operation::InspectTerminal
        | Operation::InspectFileChoices
        | Operation::CancelFileChoices
        | Operation::PreviewRecording
        | Operation::StopRecordingPlayback
        | Operation::EnableAreaSound
        | Operation::AssignAreaSound
        | Operation::PreviewAreaSound
        | Operation::AddShaderExample
        | Operation::SwitchSpatialView
        | Operation::ReturnFlatView
        | Operation::SaveCustomCastle
        | Operation::AddCustomCastle
        | Operation::InspectControl
        | Operation::InspectInformation
        | Operation::InspectSandCredits
        | Operation::InspectLaboratory => Some(Role::Feature),
        _ => None,
    };
    if let Some(role) = owner {
        let practice = world.get::<Practice>(root).unwrap();
        let session = practice.runner.session;
        let workspace = practice.workspace;
        let controls: Vec<_> = world
            .query::<(Entity, &Owned, &WorkspaceMember)>()
            .iter(world)
            .filter(|(entity, owned, member)| {
                owned.session == session
                    && owned.role == role
                    && member.0 == workspace
                    && super::root(world, *entity) == Some(root)
            })
            .map(|(entity, _, _)| entity)
            .collect();
        return match controls.as_slice() {
            [entity] if requires_control(operation) => {
                native_control(world, root, *entity, operation)
            }
            [entity] => Ok(*entity),
            [] => Err(Resolution::Missing),
            _ => Err(Resolution::Ambiguous),
        };
    }
    let target = Target {
        window: root.to_bits(),
        workspace: practice.workspace,
        owner: practice.runner.session.to_string(),
        role: format!("{operation:?}"),
    };
    let candidates: Vec<_> = world
        .query::<(Entity, &SemanticControl)>()
        .iter(world)
        .filter(|(entity, _)| super::root(world, *entity) == Some(root))
        .map(|(entity, control)| {
            (
                Target {
                    window: root.to_bits(),
                    workspace: target.workspace,
                    owner: control.session.to_string(),
                    role: format!("{:?}", control.operation),
                },
                entity,
            )
        })
        .collect();
    resolve(&target, candidates)
}

pub(super) fn requires_control(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::CompleteTask
            | Operation::UndoTask
            | Operation::StartTimer
            | Operation::StopTimer
            | Operation::CreateFrequency
            | Operation::PreviewHabit
            | Operation::ImportHabit
            | Operation::CompleteHabit
            | Operation::SaveCommand
            | Operation::RunCommand
            | Operation::StepSimulation
            | Operation::StopSimulation
            | Operation::EditSavePracticeFile
            | Operation::InspectLanguageTools
            | Operation::NextDocumentPage
            | Operation::PreviewRecording
            | Operation::StopRecordingPlayback
            | Operation::EnableAreaSound
            | Operation::AssignAreaSound
            | Operation::PreviewAreaSound
            | Operation::AddShaderExample
            | Operation::SwitchSpatialView
            | Operation::ReturnFlatView
            | Operation::SaveCustomCastle
            | Operation::AddCustomCastle
            | Operation::InspectSandCredits
    )
}

fn native_control(
    world: &mut World,
    root: Entity,
    owner: Entity,
    operation: Operation,
) -> Result<Entity, Resolution> {
    let global = matches!(
        operation,
        Operation::SwitchSpatialView
            | Operation::ReturnFlatView
            | Operation::SaveCustomCastle
            | Operation::AddCustomCastle
    );
    let mut controls: Vec<_> = world
        .query::<(Entity, &crate::actions::ActionButton)>()
        .iter(world)
        .filter(|(entity, button)| {
            if !button.actions.teaches(operation) || super::root(world, *entity) != Some(root) {
                return false;
            }
            if world.get::<SemanticControl>(*entity).is_some() {
                return false;
            }
            if button.target == owner || (global && button.target == root) {
                return true;
            }
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if entity == owner
                    || world
                        .get::<crate::actions::ControlOwner>(entity)
                        .is_some_and(|context| context.0 == owner)
                {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
        .map(|(entity, _)| entity)
        .collect();
    controls.extend(
        world
            .query::<(Entity, &crate::actions::TutorialControl)>()
            .iter(world)
            .filter(|(entity, control)| {
                control.owner == owner
                    && control.operation == operation
                    && super::root(world, *entity) == Some(root)
            })
            .map(|(entity, _)| entity),
    );
    match controls.as_slice() {
        [entity] => Ok(*entity),
        [] => Err(Resolution::Missing),
        _ => Err(Resolution::Ambiguous),
    }
}

pub(crate) fn refresh(world: &mut World) {
    #[cfg(feature = "instinct")]
    if let Some(mut metrics) = world.get_resource_mut::<Metrics>() {
        metrics.refreshes += 1;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Practice>>()
        .iter(world)
        .collect();
    for root in roots {
        let resolution = semantic(world, root);
        world
            .get_mut::<Practice>(root)
            .unwrap()
            .runner
            .set_target(resolution.map(|_| ()));
        if matches!(
            world.get::<Practice>(root).unwrap().runner.phase,
            Phase::Ready
        ) {
            let status = world.get::<Practice>(root).unwrap().status;
            let message = match resolution {
                Ok(_) => {
                    "Try the highlighted action, or use Next to perform it. Skip leaves it unchanged."
                }
                Err(Resolution::Missing) => {
                    "The intended control is unavailable. Interaction is released. Next can prepare the sample again; Skip or Close is always available."
                }
                Err(Resolution::Ambiguous) => {
                    "More than one control matches this step. Interaction is released. Choose Free, Skip or Close; guidance will resume when the target is unique."
                }
            };
            if let Some(text) = world.get::<Text>(status)
                && text.0 != message
            {
                world.get_mut::<Text>(status).unwrap().0 = message.into();
            }
        }
        if visible(world, root)
            && let Ok(target) = resolution
        {
            crate::tutorial::highlight::update_entities(world, root, vec![target]);
        } else {
            crate::tutorial::highlight::clear(world, root);
        }
        if !world.get::<Practice>(root).unwrap().runner.restricted() || !visible(world, root) {
            release(world, root);
            if !visible(world, root) {
                crate::tutorial::highlight::clear(world, root);
            }
            continue;
        }
        let candidates: Vec<_> = world
            .query_filtered::<Entity, Or<(
                With<crate::actions::ActionButton>,
                With<bevy::text::EditableText>,
                With<CanvasItem>,
                With<bevy::ui_widgets::Button>,
                With<crate::slider::SliderSand>,
            )>>()
            .iter(world)
            .filter(|entity| super::root(world, *entity) == Some(root))
            .collect();
        for entity in candidates {
            let allowed = permits_target(world, entity)
                || world
                    .get::<crate::actions::ActionButton>(entity)
                    .is_some_and(|button| button.actions.permitted(world, button.target));
            if allowed {
                if world.get::<Disabled>(entity).is_some() {
                    restore(world, entity);
                }
            } else if world.get::<Disabled>(entity).is_none()
                && world.get::<InteractionDisabled>(entity).is_none()
            {
                let state = Disabled {
                    root,
                    pickable: world.get::<Pickable>(entity).copied(),
                    tab: world.get::<TabIndex>(entity).map(|tab| tab.0),
                };
                world.entity_mut(entity).insert((
                    state,
                    InteractionDisabled,
                    Pickable::IGNORE,
                    TabIndex(-1),
                ));
            }
        }
        if world
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get)
            .is_some_and(|entity| {
                super::root(world, entity) == Some(root) && world.get::<Disabled>(entity).is_some()
            })
        {
            world.resource_mut::<InputFocus>().clear();
        }
    }
}
