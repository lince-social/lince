use super::*;
use bevy::{input::touch::{TouchInput, TouchPhase}, picking::pointer::{PointerAction, PointerButton, PointerId, PointerInput, PointerLocation}};
use crate::native_input::{absolute::{Contact, Contacts, Touch}, pointer};

impl Presentation {
    pub(crate) fn touch(&mut self, event: Touch) -> Result<bool, ProbeFailure> {
        if self.suspended() { self.cancel_touch(); return Ok(false); }
        if !event.current() || self.app.world().get_resource::<lince_interface::host::control::Status>().is_some_and(|status| status.available && (!status.active || status.locked)) {
            self.cancel_touch_id(event.id);
            return Ok(false);
        }
        self.app.world_mut().init_resource::<Contacts>();
        let geometry = self.frame_geometry();
        let views = self.accessibility_views(geometry.as_ref());
        let captured = if matches!(event.phase, TouchPhase::Started) {
            event.target.filter(|target| pointer::current(self.app.world(), &views, geometry.as_ref(), target))
        } else {
            self.app.world().resource::<Contacts>().0.get(&event.id).map(|contact| contact.target.clone())
        };
        let Some(captured) = captured else { return Ok(false); };
        let mapping_current = captured.output().and_then(|output| self.app.world().get_resource::<lince_interface::host::mapping::Snapshot>()?.get(output))
            .is_some_and(|mapping| mapping.revision() == event.mapping);
        let held = pointer::held_current(self.app.world(), &views, geometry.as_ref(), &captured);
        if !held || !mapping_current || matches!(event.phase, TouchPhase::Canceled) || event.point.iter().any(|value| !value.is_finite()) {
            self.cancel_touch_id(event.id);
            return Ok(false);
        }
        let Some(view) = views.iter().find(|view| Some(view.output) == captured.output()) else { self.cancel_touch_id(event.id); return Ok(false); };
        let touch_position = Vec2::from_array(view.raster.point(event.point).map(|value| (value / view.raster.scale[0]) as f32));
        let Some(location) = pointer::location(self.app.world_mut(), &views, geometry.as_ref(), &captured, event.point)
            else { self.cancel_touch_id(event.id); return Ok(false); };
        let window = self.input_window()?;
        let previous = self.app.world().resource::<Contacts>().0.get(&event.id).map(|contact| contact.location.position).unwrap_or(location.position);
        let world = self.app.world_mut();
        let pointer = PointerId::Touch(event.id);
        let entity = if matches!(event.phase, TouchPhase::Started) {
            if world.resource::<Contacts>().0.len() >= 64 || world.resource::<Contacts>().0.contains_key(&event.id) { return Ok(false); }
            world.spawn((pointer, PointerLocation::new(location.clone()))).id()
        } else {
            let Some(contact) = world.resource::<Contacts>().0.get(&event.id) else { return Ok(false); };
            contact.pointer
        };
        let action = match event.phase {
            TouchPhase::Started => PointerAction::Press(PointerButton::Primary),
            TouchPhase::Moved => PointerAction::Move { delta: location.position - previous },
            TouchPhase::Ended => PointerAction::Release(PointerButton::Primary),
            TouchPhase::Canceled => PointerAction::Cancel,
        };
        world.write_message(TouchInput { phase: event.phase, position: touch_position, window, force: None, id: event.id });
        world.write_message(PointerInput::new(pointer, location.clone(), action));
        if matches!(event.phase, TouchPhase::Ended) { world.resource_mut::<Contacts>().0.remove(&event.id); }
        else { world.resource_mut::<Contacts>().0.insert(event.id, Contact { target: captured, location, window, mapping: event.mapping, pointer: entity, lifetime: event.lifetime, epoch: event.epoch }); }
        if matches!(event.phase, TouchPhase::Started) { self.application_focus = None; }
        Ok(true)
    }

    #[cfg(feature = "session")]
    fn absolute_focus_permitted(&self, request: &crate::native_input::AbsoluteFocus) -> bool {
        request.current() && !self.suspended()
            && !self.app.world().get_resource::<lince_interface::host::control::Status>().is_some_and(|status| status.available && (!status.active || status.locked))
            && self.app.world().get_resource::<lince_interface::host::mapping::Snapshot>()
                .and_then(|snapshot| snapshot.get(request.output)).is_some_and(|mapping| mapping.revision() == request.mapping)
    }

    #[cfg(feature = "session")]
    pub(crate) fn prepare_absolute_focus(&mut self, request: &crate::native_input::AbsoluteFocus) -> Result<bool, ProbeFailure> {
        use crate::native_input::absolute::FocusTarget;
        if self.output_state.assignments.focused() == Some(request.output) || !self.absolute_focus_permitted(request) { return Ok(false); }
        let eligible = match &request.target {
            FocusTarget::Touch(event) => event.current() && event.target.as_ref().is_some_and(|target| target.owner_current(self.app.world())),
            FocusTarget::Tablet(event) => event.current() && event.target.as_ref().is_some_and(|target| target.owner_current(self.app.world())),
            FocusTarget::Application(surface) => self.frame_geometry().is_some_and(|geometry| geometry.items.iter()
                .any(|item| item.id == lince_interface::host::ItemId::Application(*surface) && item.output == request.output)),
            FocusTarget::Layer => true,
        };
        #[cfg(test)]
        eprintln!("Preparing absolute output focus {:?} -> {:?}: owner_current={eligible}", self.output_state.assignments.focused(), request.output);
        if !eligible { return Ok(false); }
        self.focus_output(request.output)?;
        Ok(true)
    }

    #[cfg(feature = "session")]
    pub(crate) fn absolute_focus(&mut self, request: crate::native_input::AbsoluteFocus) -> Result<(), ProbeFailure> {
        use crate::native_input::absolute::FocusTarget;
        let permitted = self.absolute_focus_permitted(&request);
        let eligible = permitted && match &request.target {
            FocusTarget::Touch(event) => event.current() && event.target.as_ref().is_some_and(|target| self.native_pointer_current(target)),
            FocusTarget::Tablet(event) => event.current(),
            FocusTarget::Application(surface) => self.frame_geometry().is_some_and(|geometry| geometry.items.iter()
                .any(|item| item.id == lince_interface::host::ItemId::Application(*surface) && item.output == request.output)),
            FocusTarget::Layer => true,
        };
        if !eligible { request.receipt.complete(false); return Ok(()); }
        self.focus_output(request.output)?;
        if !request.current() { request.receipt.complete(false); return Ok(()); }
        let accepted = match request.target {
            FocusTarget::Touch(event) => { self.focus(true)?; self.touch(event)? }
            FocusTarget::Tablet(event) => { self.focus(true)?; self.tablet(event)? }
            FocusTarget::Application(surface) => {
                self.focus(false)?;
                self.focus_application(surface);
                self.application_focus == Some(surface)
            }
            FocusTarget::Layer => { self.focus(false)?; true }
        };
        request.receipt.complete(accepted);
        Ok(())
    }

    fn cancel_touch_id(&mut self, id: u64) {
        let world = self.app.world_mut();
        let Some(contact) = world.get_resource_mut::<Contacts>().and_then(|mut contacts| contacts.0.remove(&id)) else { return; };
        world.write_message(TouchInput { phase: TouchPhase::Canceled, position: contact.location.position, window: contact.window, force: None, id });
        world.write_message(PointerInput::new(PointerId::Touch(id), contact.location, PointerAction::Cancel));
    }

    pub(crate) fn cancel_touch(&mut self) {
        let ids = self.app.world().get_resource::<Contacts>().map(|contacts| contacts.0.keys().copied().collect::<Vec<_>>()).unwrap_or_default();
        for id in ids { self.cancel_touch_id(id); }
    }

    pub(super) fn reconcile_touch(&mut self) {
        let geometry = self.frame_geometry();
        let views = self.accessibility_views(geometry.as_ref());
        let invalid = self.app.world().get_resource::<Contacts>().map(|contacts| contacts.0.iter().filter_map(|(id, contact)|
            (!pointer::held_current(self.app.world(), &views, geometry.as_ref(), &contact.target)
                || contact.lifetime.load(std::sync::atomic::Ordering::Acquire) != contact.epoch
                || contact.target.output().and_then(|output| self.app.world().get_resource::<lince_interface::host::mapping::Snapshot>()?.get(output))
                    .is_none_or(|mapping| mapping.revision() != contact.mapping)).then_some(*id)).collect::<Vec<_>>()).unwrap_or_default();
        for id in invalid { self.cancel_touch_id(id); }
    }
}
