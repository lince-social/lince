use super::*;
use lince_interface::host::input::FocusTarget;
use std::sync::mpsc::TrySendError;
use bevy::input::keyboard::Key;
use smithay::reexports::winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode};

impl Desktop {
    pub(super) fn invalidate_pointer_policy(&mut self) -> io::Result<()> {
        self.pointer_epoch = self.pointer_epoch.checked_add(1)
            .ok_or_else(|| io::Error::other("Park pointer epochs exhausted"))?;
        self.pointer_focus_item = None;
        self.pointer_sent = None;
        Ok(())
    }

    fn focus_pointer_blocked(&self) -> bool {
        #[cfg(feature = "session")]
        if self.pointer_gesture_active() { return true; }
        !self.focused || self.server.locked() || self.server.dragging()
            || self.server.pointer_constrained() || self.server.popup_grab_active() || self.server.input_method_grabbed()
            || !self.buttons.is_empty() || self.grabbed.is_some() || self.manipulation.active()
            || self.input_pending() || self.pending_focus.is_some() || self.layer_focus.is_some()
            || self.popups.iter().any(|popup| popup.hit(self.pointer).is_some())
            || self.current.as_ref().is_none_or(|current| current.frame.input.modal
                || current.frame.input.keyboard_modal || current.frame.input.keyboard_capture || current.frame.input.text_composing)
            || self.layers.iter().any(|layer| layer.keyboard == KeyboardInteractivity::Exclusive
                && matches!(layer.layer, Layer::Top | Layer::Overlay))
    }

    fn typing_key_held(&self) -> bool {
        self.keys.iter().any(|(code, event)| {
            if self.consumed.contains(code) { return false; }
            let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(*code));
            let label = self.keyboard_snapshot.as_ref().and_then(|snapshot| snapshot.labels.get(&format!("{physical:?}")));
            !matches!(input::logical(label, None, event.symbol), Key::Control | Key::Shift | Key::Alt | Key::Super | Key::AltGraph)
        })
    }

    pub(super) fn follow_pointer(&mut self) -> io::Result<()> {
        if self.focus_pointer_blocked() || !self.keys.is_empty() {
            self.pointer_focus_item = None;
            return Ok(());
        }
        let target = (|| {
            let current = self.frame_at(self.pointer)?;
            if !current.frame.input.focus_policy.pointer_follow { return None; }
            let geometry = current.frame.geometry.as_ref()?;
            if current.frame.input.focus_blocks.iter().any(|bounds| bounds.contains(self.pointer)) { return None; }
            let item = match self.target() {
                input::Target::Application(id, _, output) => geometry.items.iter().find(|item| item.id == ItemId::Application(id) && item.output == output)?,
                input::Target::Native => geometry.items.iter().rev().find(|item| matches!(item.id, ItemId::Native(_))
                    && item.surface_point(self.pointer).is_some()
                    && geometry.outputs.iter().any(|output| output.id == item.output && output.bounds.contains(self.pointer)))?,
                _ => return None,
            };
            let workspace = geometry.outputs.iter().find(|output| output.id == item.output)?.workspace;
            Some(FocusTarget { owner: current.frame.input.workspaces.owner, workspace, output: item.output, item: item.id })
        })();
        if target == self.pointer_focus_item { return Ok(()); }
        let Some(target) = target else {
            self.pointer_focus_item = None;
            return Ok(());
        };
        match self.worker.input(Input::PointerFocus(target, self.pointer_epoch)) {
            Ok(_) => self.pointer_focus_item = Some(target),
            Err(TrySendError::Full(_)) => self.pointer_focus_item = None,
            Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("Park stopped while selecting a pointer focus target")),
        }
        Ok(())
    }

    pub(super) fn warp_pointer(&mut self, time: u32) -> io::Result<()> {
        let Some(request) = self.current.as_ref().and_then(|current| current.frame.input.cursor_warp) else { return Ok(()); };
        if request.sequence <= self.cursor_warp_sequence { return Ok(()); }
        self.cursor_warp_sequence = request.sequence;
        if self.focus_pointer_blocked() || self.typing_key_held()
            || request.pointer_epoch != self.pointer_epoch
        {
            return Ok(());
        }
        let point = (|| {
            let current = self.current.as_ref()?;
            if !current.frame.input.focus_policy.warp
                || current.frame.input.workspaces.owner != request.target.owner
                || current.frame.output != Some(request.target.output)
            { return None; }
            let geometry = current.frame.geometry.as_ref()?;
            if geometry.focused != Some(request.target.item) { return None; }
            let output = geometry.outputs.iter().find(|output| output.id == request.target.output && output.workspace == request.target.workspace)?;
            let usable = output.usable?;
            let item = geometry.items.iter().find(|item| item.id == request.target.item && item.output == output.id)?;
            let center = std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5);
            let point = item.mapping.canvas_point(center)?;
            let point = if usable.contains(point) { point } else {
                let low = std::array::from_fn::<_, 2, _>(|axis| usable.origin()[axis].max(item.mapping.canvas.origin()[axis]));
                let high = std::array::from_fn::<_, 2, _>(|axis| (usable.origin()[axis] + usable.size()[axis]).min(item.mapping.canvas.origin()[axis] + item.mapping.canvas.size()[axis]));
                if (0..2).any(|axis| high[axis] <= low[axis]) { return None; }
                std::array::from_fn(|axis| (low[axis] + high[axis]) * 0.5)
            };
            (usable.contains(point) && item.surface_point(point).is_some()).then_some(point)
        })();
        if let Some(point) = point {
            self.pointer = point;
            self.display_pointer = Some(self.displayed_point(point));
            self.pointer_sent = None;
            self.redraw = true;
            self.motion(time)?;
        }
        Ok(())
    }
}
