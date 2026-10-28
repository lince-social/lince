use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandContext {
    workspace: Option<WorkspaceLayout>,
    park: bool,
    focused: bool,
    row: bool,
    inspection: Option<&'static str>,
    application: bool,
    candidates: bool,
    fullscreen: bool,
    windowed_fullscreen: bool,
    compact: bool,
    compact_eligible: bool,
    compact_candidates: bool,
    compact_busy: bool,
    scratch_markable: bool,
    scratch_marked: bool,
    scratch_items: bool,
    scratch_blocked: bool,
    magnification: Option<u8>,
    spatial: bool,
    selection: bool,
    workspaces: usize,
    adjacent: [Option<&'static str>; 2],
    reorder: [Option<&'static str>; 2],
    external: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_command_availability_tracks_reordering_without_changing_focus_or_camera() {
        let (mut world, root, _, _) = super::super::tests::fixture();
        let mut other = world.get::<Workspaces>(root).unwrap().entries[0].clone();
        other.id = 31;
        world
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .push(other);
        let before = *world.get::<CanvasView>(root).unwrap();
        let initial = CommandContext::new(&world, root);
        assert!(
            initial
                .unavailable(DesktopAction::PreviousWorkspace)
                .unwrap()
                .contains("first")
        );
        assert!(
            initial
                .unavailable(DesktopAction::MoveWorkspaceEarlier)
                .unwrap()
                .contains("first")
        );
        assert!(initial.unavailable(DesktopAction::NextWorkspace).is_none());
        assert!(
            initial
                .unavailable(DesktopAction::MoveWorkspaceLater)
                .is_none()
        );
        crate::native::workspace::navigation::reorder(&mut world, root, 1).unwrap();
        let changed = CommandContext::new(&world, root);
        assert!(
            changed
                .unavailable(DesktopAction::PreviousWorkspace)
                .is_none()
        );
        assert!(
            changed
                .unavailable(DesktopAction::MoveWorkspaceEarlier)
                .is_none()
        );
        assert!(
            changed
                .unavailable(DesktopAction::NextWorkspace)
                .unwrap()
                .contains("last")
        );
        assert!(
            changed
                .unavailable(DesktopAction::MoveWorkspaceLater)
                .unwrap()
                .contains("last")
        );
        let current = world.get::<CanvasView>(root).unwrap();
        assert_eq!((current.center, current.zoom), (before.center, before.zoom));
        assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
    }

    #[test]
    fn row_transfer_requires_a_focused_tiled_member_and_missing_numbered_destinations_are_available()
     {
        let (mut world, root, _, application) = super::super::tests::fixture();
        let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
        world.insert_resource(HostRequests(sender));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Tiled);
        super::super::super::engine::resolve(&mut world);
        super::super::focus_item(&mut world, application);
        let action = DesktopAction::MoveRowToOutput(crate::host::Direction::Right);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(action)
                .is_none()
        );
        for number in 1..=9 {
            assert!(
                CommandContext::new(&world, root)
                    .unavailable(DesktopAction::Workspace(number))
                    .is_none()
            );
            assert!(
                CommandContext::new(&world, root)
                    .unavailable(DesktopAction::MoveToWorkspace(number))
                    .is_none()
            );
        }
        for number in [0, 10, u32::MAX] {
            assert!(
                CommandContext::new(&world, root)
                    .unavailable(DesktopAction::Workspace(number))
                    .is_some()
            );
        }
        crate::native::actions::Action::apply(&DesktopAction::ToggleFloating, &mut world, root);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(action)
                .unwrap()
                .contains("tiled row")
        );
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(action)
                .is_some()
        );
    }

    #[test]
    fn availability_tracks_live_application_focus_visibility_and_layout_without_changing_them() {
        let (mut world, root, native, application) = super::super::tests::fixture();
        super::super::super::engine::resolve(&mut world);
        super::super::focus_item(&mut world, application);
        let context = CommandContext::new(&world, root);
        assert!(
            context
                .unavailable(DesktopAction::InspectWindowRecord)
                .unwrap()
                .contains("Park session")
        );
        let (sender, _receiver) = std::sync::mpsc::sync_channel(2);
        world.insert_resource(HostRequests(sender));
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::InspectWindowRecord)
                .is_none()
        );
        world
            .entity_mut(application)
            .insert(bevy::ecs::entity_disabling::Disabled);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::Close)
                .is_some()
        );
        world
            .entity_mut(application)
            .remove::<bevy::ecs::entity_disabling::Disabled>();
        super::super::focus_item(&mut world, native);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::InspectWindowRecord)
                .unwrap()
                .contains("Record")
        );
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::ToggleFloating)
                .unwrap()
                .contains("tiled")
        );
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .toggle_fullscreen();
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::CycleItemWidth)
                .unwrap()
                .contains("Leave fullscreen")
        );
        assert!(
            CommandContext::new(&world, root)
                .unavailable(DesktopAction::ToggleFullscreen)
                .is_none()
        );
    }
}

impl CommandContext {
    pub(crate) fn new(world: &World, root: Entity) -> Self {
        let spaces = world.get::<Workspaces>(root);
        let workspace = spaces.and_then(|spaces| {
            spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)
        });
        let focused = workspace.and_then(|space| {
            let focused = space.desktop.focused()?;
            world.get::<Children>(root)?.iter().find(|entity| {
                world
                    .get::<Managed>(*entity)
                    .is_some_and(|managed| managed.id == focused)
                    && focus::eligible(world, root, space.id, *entity)
            })
        });
        Self {
            workspace: workspace.map(|space| space.desktop.mode()),
            park: world.contains_resource::<HostRequests>(),
            focused: focused.is_some(),
            row: focused.zip(workspace).is_some_and(|(entity, workspace)| {
                transfers::row_available(world, root, workspace.id, entity)
            }),
            inspection: inspection::capture(world, root).err(),
            application: focused.is_some_and(|entity| world.get::<ApplicationWindow>(entity).is_some() && world.get::<TransientParent>(entity).is_none()),
            candidates: focus::status(world, root).1,
            fullscreen: workspace.is_some_and(|space| space.desktop.fullscreen().is_some()),
            windowed_fullscreen: focused.zip(workspace).is_some_and(|(entity, space)| world.get::<Managed>(entity).is_some_and(|managed| space.desktop.windowed_fullscreen(managed.id))),
            compact: focused.zip(workspace).is_some_and(|(entity, space)| world.get::<Managed>(entity).is_some_and(|managed| space.desktop.compact_group(managed.id).is_some())),
            compact_eligible: focused.is_some_and(|entity| compact::groupable(world, root, entity)),
            compact_candidates: focused.is_some_and(|entity| compact::has_candidates(world, root, entity)),
            compact_busy: compact::busy(world),
            scratch_markable: scratch::can_mark(world, root),
            scratch_marked: scratch::marked(world, root),
            scratch_items: scratch::has_items(world, root),
            scratch_blocked: world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active)),
            magnification: world.get::<HostViewport>(root).and_then(|viewport| world.get_resource::<crate::host::magnification::Snapshot>()
                .filter(|snapshot| snapshot.available).and_then(|snapshot| snapshot.head(viewport.0.id)))
                .map(|head| u8::from(head.requested < 4.0) | u8::from(head.requested > 1.0) << 1),
            spatial: crate::native::topology::view::spatial(world, root),
            selection: !crate::native::canvas_selection::selected(world, root).is_empty(),
            workspaces: crate::native::workspace::numbers(world, root).len(),
            adjacent: [-1, 1].map(|offset| {
                crate::native::workspace::navigation::destination(world, root, offset).err()
            }),
            reorder: [-1, 1].map(|offset| {
                crate::native::workspace::navigation::reorder_destination(world, root, offset).err()
            }),
            external: world
                .get_resource::<crate::host::external::Snapshot>()
                .filter(|snapshot| snapshot.available)
                .map(|snapshot| {
                    snapshot
                        .settings
                        .0
                        .keys()
                        .fold(0, |mask, action| mask | 1 << action.index())
                }),
        }
    }

    pub(crate) fn unavailable(self, action: DesktopAction) -> Option<&'static str> {
        if action.requires_host() && !self.park {
            return Some("This command requires a Park session.");
        }
        if self.workspace.is_none() {
            return Some("The workspace is not ready.");
        }
        match action {
            DesktopAction::Magnify(_) if self.scratch_blocked => Some("Unlock and activate Park before changing display magnification."),
            DesktopAction::Magnify(_) if self.compact_busy => Some("Finish the active input interaction before changing magnification."),
            DesktopAction::Magnify(_) if self.magnification.is_none() => Some("Display magnification requires an available Park output and local configuration."),
            DesktopAction::Magnify(crate::host::magnification::Adjustment::Increase) if self.magnification.is_some_and(|mask| mask & 1 == 0) => Some("Display magnification is already 4×."),
            DesktopAction::Magnify(crate::host::magnification::Adjustment::Decrease | crate::host::magnification::Adjustment::Reset) if self.magnification.is_some_and(|mask| mask & 2 == 0) => Some("Display magnification is already 1×."),
            DesktopAction::MarkScratch | DesktopAction::ShowScratch | DesktopAction::HideScratch | DesktopAction::UnmarkScratch if self.scratch_blocked => Some("Scratch controls are unavailable while Park is locked or inactive."),
            DesktopAction::MarkScratch | DesktopAction::ShowScratch | DesktopAction::HideScratch | DesktopAction::UnmarkScratch if self.compact_busy => Some("Finish the current drag before changing scratch items."),
            DesktopAction::MarkScratch if self.compact => Some("Remove the item from its compact group before marking it as scratch."),
            DesktopAction::MarkScratch if !self.scratch_markable => Some("Focus or select an eligible item that is not already marked as scratch."),
            DesktopAction::ShowScratch if !self.scratch_items => Some("Mark an existing item as scratch first. This command never launches a program."),
            DesktopAction::HideScratch | DesktopAction::UnmarkScratch if !self.scratch_marked => Some("Focus a scratch item, or use its original item properties."),
            DesktopAction::ToggleWindowedFullscreen if !self.park => Some("This application command requires a Park session."),
            DesktopAction::ToggleWindowedFullscreen if !self.application => Some("Focus a normal application window to change windowed fullscreen."),
            DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers | DesktopAction::NextCompactMember
            | DesktopAction::PreviousCompactMember | DesktopAction::MoveCompactMember(_) | DesktopAction::DetachCompactMember
            | DesktopAction::DissolveCompactGroup if self.compact_busy => Some("Finish the current drag before changing compact membership or selection."),
            DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers | DesktopAction::NextCompactMember
            | DesktopAction::PreviousCompactMember | DesktopAction::MoveCompactMember(_) if self.workspace != Some(WorkspaceLayout::Tiled) => Some("Compact presentation is available while the workspace is tiled."),
            DesktopAction::CreateCompactGroup if self.fullscreen => Some("Leave fullscreen before creating a compact group."),
            DesktopAction::CreateCompactGroup if !self.compact_eligible => Some("Focus an eligible non-floating item outside a compact group."),
            DesktopAction::AddCompactMembers | DesktopAction::NextCompactMember | DesktopAction::PreviousCompactMember
            | DesktopAction::MoveCompactMember(_) | DesktopAction::DetachCompactMember | DesktopAction::DissolveCompactGroup if !self.compact => Some("Focus an item in a compact group."),
            DesktopAction::AddCompactMembers if self.fullscreen => Some("Leave fullscreen before adding compact members."),
            DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers if !self.compact_candidates => Some("There are no other eligible items in this workspace."),
            DesktopAction::MoveCompactMember(Direction::Up | Direction::Down) => Some("Compact member order moves earlier or later."),
            DesktopAction::ToggleFloating if self.compact => Some("Remove the item from its compact group before floating it."),
            DesktopAction::PreviousWorkspace => self.adjacent[0],
            DesktopAction::NextWorkspace => self.adjacent[1],
            DesktopAction::MoveWorkspaceEarlier => self.reorder[0],
            DesktopAction::MoveWorkspaceLater => self.reorder[1],
            DesktopAction::External(_) if self.external.is_none() => {
                Some("External command configuration is unavailable.")
            }
            DesktopAction::External(external)
                if self
                    .external
                    .is_some_and(|mask| mask & (1 << external.index()) == 0) =>
            {
                Some("Configure this external command in Park settings.")
            }
            DesktopAction::Focus(_) | DesktopAction::ToggleFocus if !self.candidates => {
                Some("No eligible item is available in this workspace.")
            }
            DesktopAction::CycleItemWidth if self.fullscreen => {
                Some("Leave fullscreen before changing the window width.")
            }
            DesktopAction::CycleItemWidth if !self.focused => {
                Some("No eligible item is focused in this workspace.")
            }
            DesktopAction::Resize(_) | DesktopAction::CycleRowHeight if self.fullscreen => {
                Some("Leave fullscreen before changing item size.")
            }
            DesktopAction::Resize(_) | DesktopAction::CycleRowHeight if !self.focused => {
                Some("No eligible item is focused in this workspace.")
            }
            DesktopAction::CycleRowHeight if !self.row => {
                Some("Focus an item in a tiled row before changing row height.")
            }
            DesktopAction::ToggleFloating if self.workspace == Some(WorkspaceLayout::Free) => {
                Some("Individual floating is available while the workspace is tiled.")
            }
            DesktopAction::InspectWindowRecord => self.inspection,
            DesktopAction::ReapplyOpeningRules if !self.application => Some("Focus an application window before reapplying opening rules."),
            DesktopAction::MoveRowToOutput(_) if !self.row => {
                Some("Focus an item in a tiled row before moving the row to a monitor.")
            }
            DesktopAction::MoveToWorkspace(index) | DesktopAction::Workspace(index)
                if index == 0 || index > 9 && index as usize > self.workspaces =>
            {
                Some("That numbered workspace does not exist.")
            }
            DesktopAction::ToggleFullscreen if self.fullscreen => None,
            DesktopAction::Move(_)
            | DesktopAction::Close
            | DesktopAction::ToggleFloating
            | DesktopAction::ToggleFullscreen
            | DesktopAction::ToggleMaximized
            | DesktopAction::MoveToWorkspace(_)
            | DesktopAction::MoveToOutput(_)
                if !self.focused =>
            {
                Some("Focus an item before using this command.")
            }
            _ => None,
        }
    }

    pub(crate) fn windowed_fullscreen(self) -> Option<bool> {
        (self.park && self.application).then_some(self.windowed_fullscreen)
    }

    pub(crate) fn shortcut(
        self,
        shortcut: crate::native::shortcuts::Shortcut,
    ) -> Option<&'static str> {
        use crate::native::shortcuts::{Scope, Shortcut};
        if self.workspace.is_none() {
            return Some("The workspace is not ready.");
        }
        if let Some(action) = shortcut.desktop_action() {
            return self.unavailable(action);
        }
        if shortcut.scope() == Scope::Canvas3d && !self.spatial {
            return Some("Switch to the 3D view to use this command.");
        }
        if shortcut == Shortcut::DeleteSand && !self.selection {
            return Some("Select a Sand before deleting it.");
        }
        None
    }
}
