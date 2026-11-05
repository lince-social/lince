use super::*;

impl Presentation {
    pub(crate) fn inspect_window(
        &mut self,
        target: lince_interface::host::WindowInspection,
    ) -> Result<(), ProbeFailure> {
        if self.suspended() {
            return Ok(());
        }
        if let Err(error) = self.try_inspect_window(target) {
            native::notifications::report(
                self.app.world(),
                "Window properties",
                &error.to_string(),
            );
        }
        Ok(())
    }

    pub(in crate::presentation) fn try_inspect_window(
        &mut self,
        target: lince_interface::host::WindowInspection,
    ) -> Result<(), ProbeFailure> {
        if self.suspended() {
            return Err(input_failure("The Park view is inactive."));
        }
        let root = self.canvas_root()?;
        if !target.valid()
            || root.to_bits() != target.owner
            || self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .is_none_or(|spaces| spaces.active != target.workspace)
        {
            return Err(input_failure("The original workspace is unavailable."));
        }
        let window = self
            .windows
            .get(&target.surface)
            .filter(|_| {
                self.identities.get(&target.surface) == Some(&PlacementId(target.placement))
            })
            .ok_or_else(|| input_failure("The selected window is no longer available."))?;
        let entity = window.entity;
        native::item_properties::open(self.app.world_mut(), root, entity)
            .map(|_| ())
            .ok_or_else(|| input_failure("The selected item is unavailable."))
    }
}
