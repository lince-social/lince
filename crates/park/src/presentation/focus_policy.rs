use super::*;
use lince_interface::{host::{ItemId, input::FocusTarget}, native::layout::desktop::focus};

impl Presentation {
    pub(crate) fn pointer_focus(&mut self, target: FocusTarget, epoch: u64) -> Result<(), ProbeFailure> {
        if self.suspended() || !focus::matches_pointer(self.app.world(), epoch) { return Ok(()); }
        let root = self.canvas_root()?;
        if root.to_bits() != target.owner
            || !focus::can_hover(self.app.world(), root, target.workspace)
            || !self.output_state.assignments.outputs().iter().any(|output| output.id == target.output && output.active == target.workspace)
        {
            return Ok(());
        }
        let world = self.app.world();
        let entity = world.get::<Children>(root).and_then(|children| children.iter().find(|entity| {
            world.get::<native::workspace::WorkspaceMember>(*entity).is_some_and(|member| member.0 == target.workspace)
                && match target.item {
                    ItemId::Application(surface) => world.get::<native::layout::desktop::ApplicationWindow>(*entity).is_some_and(|window| window.surface == surface),
                    ItemId::Native(id) => world.get::<native::layout::desktop::ApplicationWindow>(*entity).is_none()
                        && world.get::<native::layout::LayoutBox>(*entity).is_some_and(|layout| layout.id == id),
                }
        }));
        let Some(entity) = entity else { return Ok(()); };
        if !focus::hover_eligible(world, root, target.workspace, entity) {
            return Ok(());
        }
        self.focus_output(target.output)?;
        if focus::hover(self.app.world_mut(), root, target.workspace, entity) {
            self.application_focus = match target.item { ItemId::Application(surface) => Some(surface), _ => None };
        }
        Ok(())
    }
}
