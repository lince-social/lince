use crate::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
};
use bevy::{
    input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup},
    prelude::*,
    text::EditableText,
};

type SettingsResult = Result<(bool, bool), String>;

#[derive(Resource)]
struct Settings {
    sands: bool,
    records: bool,
    loaded: bool,
    pending: Option<tokio::sync::oneshot::Receiver<SettingsResult>>,
    error: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sands: true,
            records: true,
            loaded: false,
            pending: None,
            error: None,
        }
    }
}

#[derive(Component)]
struct Confirmation {
    overlay: Entity,
    targets: Vec<Entity>,
    previous: Option<Entity>,
    workspace: u64,
}

pub struct DeletionPlugin;

impl Plugin for DeletionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .add_systems(Update, settings);
    }
}

fn settings(world: &mut World) {
    let ready = world
        .resource_mut::<Settings>()
        .pending
        .as_mut()
        .and_then(|pending| match pending.try_recv() {
            Ok(result) => Some(result),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            Err(_) => Some(Err("Could not save deletion settings.".into())),
        });
    if let Some(result) = ready {
        let mut settings = world.resource_mut::<Settings>();
        settings.pending = None;
        match result {
            Ok((sands, records)) => {
                settings.sands = sands;
                settings.records = records;
                settings.error = None;
            }
            Err(error) => settings.error = Some(error),
        }
        let roots: Vec<_> = world
            .query_filtered::<Entity, With<crate::edit_mode::EditMode>>()
            .iter(world)
            .collect();
        for root in roots {
            crate::edit_mode::render_panel(world, root);
        }
    }
    if !world.resource::<Settings>().loaded && world.contains_resource::<crate::app::CellHandle>() {
        begin_settings(world, None);
    }
}

fn begin_settings(world: &mut World, change: Option<(bool, bool)>) {
    let Some(runtime) = world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.clone())
    else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut settings = world.resource_mut::<Settings>();
    settings.loaded = true;
    settings.pending = Some(receiver);
    handle.spawn(async move {
        let result = match change {
            Some((records, enabled)) => runtime.set_deletion_confirmation(records, enabled).await,
            None => runtime.deletion_confirmations().await,
        };
        let _ = sender.send(result.map_err(|error| error.to_string()));
        if let Some(wake) = wake {
            wake.ring();
        }
    });
}

#[derive(Clone, Copy)]
pub struct DeleteSelected;

impl Action for DeleteSelected {
    fn apply(&self, world: &mut World, root: Entity) {
        let mut focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
        while let Some(entity) = focus {
            if world.get::<EditableText>(entity).is_some() {
                return;
            }
            focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        let mut targets = crate::canvas_selection::selected(world, root);
        if targets.is_empty()
            && let Some(entity) = world
                .get::<crate::inspection::Inspection>(root)
                .and_then(|inspection| inspection.selected)
        {
            targets = crate::canvas_selection::group_members(world, root, entity);
        }
        request(world, root, targets);
    }
}

pub(crate) fn request(world: &mut World, root: Entity, mut targets: Vec<Entity>) {
    if world.get::<Confirmation>(root).is_some() {
        return;
    }
    targets.retain(|entity| crate::canvas_selection::eligible(world, root, *entity));
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        return;
    }
    if world
        .get_resource::<Settings>()
        .is_some_and(|settings| !settings.sands)
    {
        remove(world, root, targets);
        return;
    }
    let Some(workspace) = world
        .get::<crate::workspace::Workspaces>(root)
        .map(|spaces| spaces.active)
    else {
        return;
    };
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let overlay = world
        .spawn((
            crate::inspection::InspectionExcluded,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            GlobalZIndex(100),
            TabGroup::modal(),
            ChildOf(root),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::actions![Decision(false)],
            )]),
        ))
        .id();
    let panel = world
        .spawn((
            Node {
                width: px(400),
                max_width: percent(95),
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(overlay),
        ))
        .id();
    label(
        world,
        panel,
        &format!("Delete {} canvas item(s)?", targets.len()),
        22.0,
    );
    label(
        world,
        panel,
        "The selected items and their local contents will be removed. Records in the database are kept.",
        14.0,
    );
    let cancel = button(world, panel, root, Decision(false), "Cancel");
    button(world, panel, root, Decision(true), "Delete");
    world.entity_mut(root).insert(Confirmation {
        overlay,
        targets,
        previous,
        workspace,
    });
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(cancel, FocusCause::Navigated);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Decision(pub(crate) bool);

impl Action for Decision {
    fn apply(&self, world: &mut World, target: Entity) {
        let root = if world.get::<Confirmation>(target).is_some() {
            target
        } else {
            let Some(parent) = world.get::<ChildOf>(target) else {
                return;
            };
            parent.parent()
        };
        let Some(confirmation) = world.entity_mut(root).take::<Confirmation>() else {
            return;
        };
        world.despawn(confirmation.overlay);
        if self.0
            && world
                .get::<crate::workspace::Workspaces>(root)
                .is_some_and(|spaces| spaces.active == confirmation.workspace)
        {
            remove(world, root, confirmation.targets);
        }
        let restore = confirmation
            .previous
            .filter(|entity| world.get_entity(*entity).is_ok())
            .unwrap_or(root);
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.set(restore, FocusCause::Navigated);
        }
    }
}

fn remove(world: &mut World, root: Entity, targets: Vec<Entity>) {
    for entity in targets {
        if !crate::canvas_selection::eligible(world, root, entity) {
            continue;
        }
        if world.get::<crate::area::InfluenceArea>(entity).is_some() {
            crate::area_mutation::disarm(world, entity, "Property changes stopped after deletion.");
        }
        if world
            .get::<crate::protein_castle::ProteinCastle>(entity)
            .is_some()
        {
            crate::protein_castle::remove(world, entity);
        } else {
            world.despawn(entity);
        }
    }
    crate::canvas_selection::clear(world, root);
    if world
        .get_resource::<InputFocus>()
        .and_then(InputFocus::get)
        .is_some_and(|entity| world.get_entity(entity).is_err())
        && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
    {
        focus.set(root, FocusCause::Navigated);
    }
    if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
        inspection.selected = None;
    }
    if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
        editor.selected = None;
        editor.cancel();
    }
    if world.get::<crate::edit_mode::EditMode>(root).is_some() {
        crate::edit_mode::render_panel(world, root);
    }
}

#[derive(Clone, Copy)]
struct Toggle(bool);

impl Action for Toggle {
    fn apply(&self, world: &mut World, root: Entity) {
        let settings = world.resource::<Settings>();
        if settings.pending.is_some() {
            return;
        }
        let enabled = if self.0 {
            !settings.records
        } else {
            !settings.sands
        };
        if world.contains_resource::<crate::app::CellHandle>() {
            begin_settings(world, Some((self.0, enabled)));
        } else {
            let mut settings = world.resource_mut::<Settings>();
            if self.0 {
                settings.records = enabled;
            } else {
                settings.sands = enabled;
            }
        }
        crate::edit_mode::render_panel(world, root);
    }
}

pub(crate) fn controls(world: &mut World, root: Entity, panel: Entity) {
    let Some(settings) = world.get_resource::<Settings>() else {
        return;
    };
    let (sands, records, pending, error) = (
        settings.sands,
        settings.records,
        settings.pending.is_some(),
        settings.error.clone(),
    );
    label(world, panel, "Deletion confirmation", 18.0);
    for (record, enabled, name) in [(false, sands, "Canvas items"), (true, records, "Records")] {
        let entity = button(
            world,
            panel,
            root,
            Toggle(record),
            &format!("{name}: {}", if enabled { "ask" } else { "do not ask" }),
        );
        if pending {
            world
                .entity_mut(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
    }
    if pending {
        label(world, panel, "Saving settings…", 14.0);
    }
    if let Some(error) = error {
        label(world, panel, &error, 14.0);
    }
}

fn button(
    world: &mut World,
    parent: Entity,
    target: Entity,
    action: impl Action,
    title: &str,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            Node {
                min_height: px(40),
                padding: UiRect::axes(px(16), px(8)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            crate::sand::Square,
            crate::token_style::background(crate::tokens::Token::Surface),
            ActionButton::new(target, crate::actions![action]),
            ChildOf(parent),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    label(world, entity, title, 14.0);
    entity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::CanvasItem,
        canvas_selection::SandSelection,
        workspace::{WorkspaceMember, Workspaces},
    };

    fn fixture() -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<InputFocus>();
        world.init_resource::<Settings>();
        let root = world.spawn(Workspaces::default()).id();
        let first = world
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let second = world
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        world
            .entity_mut(root)
            .insert(SandSelection(vec![first, second]));
        (world, root, first, second)
    }

    #[test]
    fn cancel_preserves_group_and_confirm_removes_children_only_in_captured_selection() {
        let (mut world, root, first, second) = fixture();
        let child = world.spawn(ChildOf(first)).id();
        DeleteSelected.apply(&mut world, root);
        assert!(world.get_entity(first).is_ok());
        Decision(false).apply(&mut world, root);
        assert!(world.get_entity(child).is_ok());
        DeleteSelected.apply(&mut world, root);
        let third = world
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        world.entity_mut(root).insert(SandSelection(vec![third]));
        Decision(true).apply(&mut world, root);
        for entity in [first, second, child] {
            assert!(world.get_entity(entity).is_err());
        }
        assert!(world.get_entity(third).is_ok());
        assert!(world.get::<SandSelection>(root).unwrap().0.is_empty());
    }

    #[test]
    fn text_focus_and_foreign_workspaces_protect_items() {
        let (mut world, root, first, second) = fixture();
        let text = world
            .spawn((crate::sand::editable("draft"), ChildOf(first)))
            .id();
        world
            .resource_mut::<InputFocus>()
            .set(text, FocusCause::Pressed);
        DeleteSelected.apply(&mut world, root);
        assert!(world.get::<Confirmation>(root).is_none());
        world.resource_mut::<InputFocus>().clear();
        world.get_mut::<WorkspaceMember>(second).unwrap().0 = 2;
        DeleteSelected.apply(&mut world, root);
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
        Decision(true).apply(&mut world, root);
        assert!(world.get_entity(first).is_ok());
        assert!(world.get_entity(second).is_ok());
        world.get_mut::<Workspaces>(root).unwrap().active = 1;
        DeleteSelected.apply(&mut world, root);
        Decision(true).apply(&mut world, root);
        assert!(world.get_entity(first).is_err());
        assert!(world.get_entity(second).is_ok());
    }

    #[test]
    fn disabled_canvas_confirmation_keeps_record_setting_independent() {
        let (mut world, root, first, second) = fixture();
        world.resource_mut::<Settings>().sands = false;
        DeleteSelected.apply(&mut world, root);
        assert!(world.get_entity(first).is_err());
        assert!(world.get_entity(second).is_err());
        assert!(world.resource::<Settings>().records);
        assert!(world.get::<Confirmation>(root).is_none());
    }
}
