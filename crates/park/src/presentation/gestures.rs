use super::*;
use lince_interface::host::input::GestureDelivery;

impl Presentation {
    pub(crate) fn cancel_gesture(&mut self) {
        native::layout::desktop::gestures::cancel(self.app.world_mut());
    }

    pub(crate) fn gesture(&mut self, delivery: GestureDelivery) -> Result<(), ProbeFailure> {
        if self.suspended()
            || !self
                .app
                .world()
                .get_resource::<native::layout::desktop::gestures::GestureSupport>()
                .is_some_and(|support| support.0)
        {
            self.cancel_gesture();
            return Ok(());
        }
        let root = self.canvas_root()?;
        if let GestureDelivery::Begin(context) = delivery {
            if context.owner != root.to_bits()
                || native::shortcuts::keyboard_owned(self.app.world())
                    && !(context.fingers == 4
                        && native::layout::desktop::overview::active(self.app.world()))
            {
                return Ok(());
            }
            if !self.output_state.outputs.is_empty() {
                if !self
                    .output_state
                    .assignments
                    .outputs()
                    .iter()
                    .any(|output| output.id == context.output && output.active == context.workspace)
                {
                    return Ok(());
                }
                self.focus_output(context.output)?;
            }
        }
        if let Some(action) =
            native::layout::desktop::gestures::deliver(self.app.world_mut(), root, delivery)
        {
            self.desktop_action(action)?;
        }
        Ok(())
    }
}
