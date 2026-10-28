use super::*;
use crate::native_input::{absolute::{Pen, Pens, Tablet, tablet_pointer as pointer_id}, pointer};
use bevy::picking::pointer::{PointerAction, PointerButton, PointerInput, PointerLocation};
use lince_interface::host::input::{TabletInput, TabletPhase};
use std::sync::atomic::Ordering;

fn button(code: u32) -> Option<PointerButton> {
    match code { 272 => Some(PointerButton::Primary), 273 | 331 => Some(PointerButton::Secondary), 274 | 332 => Some(PointerButton::Middle), _ => None }
}

fn pressed(pen: &Pen, target: PointerButton) -> bool {
    pen.tip && target == PointerButton::Primary || pen.buttons.iter().any(|code| button(*code) == Some(target))
}

impl Presentation {
    pub(crate) fn tablet(&mut self, event: Tablet) -> Result<bool, ProbeFailure> {
        let id = event.event.tool;
        let same = self.app.world().resource::<Pens>().tools.get(&id).is_some_and(|pen| Arc::ptr_eq(&pen.lifetime, &event.lifetime) && pen.epoch == event.epoch);
        if matches!(event.event.phase, TabletPhase::Leave | TabletPhase::Cancel) {
            if same { self.cancel_tablet_id(id, if event.current() { event.event.phase } else { TabletPhase::Cancel }); }
            return Ok(false);
        }
        if !event.current() { if same { self.cancel_tablet_id(id, TabletPhase::Cancel); } return Ok(false); }
        if self.suspended() || self.app.world().get_resource::<lince_interface::host::control::Status>().is_some_and(|status| status.available && (!status.active || status.locked)) {
            self.cancel_tablet();
            return Ok(false);
        }
        let geometry = self.frame_geometry();
        let views = self.accessibility_views(geometry.as_ref());
        let current = self.app.world().resource::<Pens>().tools.get(&id);
        let exists = current.is_some();
        let held = !matches!(event.event.phase, TabletPhase::Enter) && current.is_some_and(|pen| pen.tip || !pen.buttons.is_empty());
        let captured = if !held { event.target.clone().filter(|target| pointer::current(self.app.world(), &views, geometry.as_ref(), target)) }
            else { current.map(|pen| pen.target.clone()) };
        let Some(captured) = captured else { if same { self.cancel_tablet_id(id, TabletPhase::Cancel); } return Ok(false); };
        let mapping = self.app.world().get_resource::<lince_interface::host::mapping::Snapshot>().and_then(|snapshot| snapshot.get(event.event.output));
        if !event.event.axes.valid() || event.event.point.iter().any(|value| !value.is_finite()) || captured.output() != Some(event.event.output)
            || mapping.is_none_or(|mapping| mapping.revision() != event.mapping)
            || !pointer::held_current(self.app.world(), &views, geometry.as_ref(), &captured) {
            if same { self.cancel_tablet_id(id, TabletPhase::Cancel); }
            return Ok(false);
        }
        let Some(location) = pointer::location(self.app.world_mut(), &views, geometry.as_ref(), &captured, event.event.point) else { return Ok(false); };
        let acquire = matches!(event.event.phase, TabletPhase::Enter)
            || !same && !exists && (event.fresh_press || !event.held && matches!(event.event.phase, TabletPhase::Motion));
        if acquire {
            let previous_pointer = self.app.world().resource::<Pens>().tools.get(&id).map(|pen| pen.pointer);
            self.cancel_tablet_id(id, TabletPhase::Cancel);
            if self.app.world().resource::<Pens>().tools.len() >= 128 { return Ok(false); }
            let world = self.app.world_mut();
            let entity = if let Some(entity) = previous_pointer.filter(|entity| world.get_entity(*entity).is_ok()) {
                world.resource_mut::<Pens>().retired.retain(|retired| *retired != entity);
                world.entity_mut(entity).insert((PointerLocation::new(location.clone()), event.event.clone()));
                entity
            } else { world.spawn((pointer_id(id), PointerLocation::new(location.clone()), event.event.clone())).id() };
            self.app.world_mut().resource_mut::<Pens>().tools.insert(id, Pen { target: captured.clone(), location: location.clone(), pointer: entity,
                mapping: event.mapping, lifetime: event.lifetime.clone(), epoch: event.epoch, scope: event.scope.clone(), scope_epoch: event.scope_epoch,
                tip: false, buttons: Default::default(), last: event.event.clone() });
            if event.event.phase != TabletPhase::Enter {
                let mut enter = event.event.clone(); enter.phase = TabletPhase::Enter; enter.axes.wheel = None;
                self.app.world_mut().write_message::<TabletInput>(enter);
            }
        } else if !same { return Ok(false); }
        let world = self.app.world_mut();
        let mut pens = world.resource_mut::<Pens>();
        let Some(pen) = pens.tools.get_mut(&id) else { return Ok(false); };
        let previous = pen.location.position;
        let before = [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle].map(|button| pressed(pen, button));
        match event.event.phase {
            TabletPhase::Tip(down) => pen.tip = down,
            TabletPhase::Button { code, pressed } => { if pressed { pen.buttons.insert(code); } else { pen.buttons.remove(&code); } }
            _ => {}
        }
        let after = [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle].map(|button| pressed(pen, button));
        let pointer = pen.pointer;
        pen.target = captured;
        pen.location = location.clone();
        pen.last = event.event.clone();
        drop(pens);
        world.entity_mut(pointer).insert(event.event.clone());
        world.write_message(PointerInput::new(pointer_id(id), location.clone(), PointerAction::Move { delta: location.position - previous }));
        for (index, button) in [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle].into_iter().enumerate() {
            if before[index] != after[index] { world.write_message(PointerInput::new(pointer_id(id), location.clone(), if after[index] { PointerAction::Press(button) } else { PointerAction::Release(button) })); }
        }
        world.write_message::<TabletInput>(event.event.clone());
        if matches!(event.event.phase, TabletPhase::Tip(true) | TabletPhase::Button { pressed: true, .. }) { self.application_focus = None; }
        Ok(true)
    }

    fn cancel_tablet_id(&mut self, id: u64, phase: TabletPhase) {
        let world = self.app.world_mut();
        let Some(mut pen) = world.resource_mut::<Pens>().tools.remove(&id) else { return; };
        pen.last.phase = phase;
        pen.last.axes.wheel = None;
        world.write_message::<TabletInput>(pen.last);
        world.write_message(PointerInput::new(pointer_id(id), pen.location, PointerAction::Cancel));
        world.resource_mut::<Pens>().retired.push(pen.pointer);
    }

    pub(crate) fn cancel_tablet(&mut self) {
        let ids = self.app.world().resource::<Pens>().tools.keys().copied().collect::<Vec<_>>();
        for id in ids { self.cancel_tablet_id(id, TabletPhase::Cancel); }
    }

    pub(super) fn reconcile_tablet(&mut self) {
        let geometry = self.frame_geometry();
        let views = self.accessibility_views(geometry.as_ref());
        let ids = self.app.world().resource::<Pens>().tools.iter().filter_map(|(id, pen)|
            (pen.lifetime.load(Ordering::Acquire) != pen.epoch || !pointer::held_current(self.app.world(), &views, geometry.as_ref(), &pen.target)
                || pen.scope.load(Ordering::Acquire) != pen.scope_epoch
                || self.app.world().get_resource::<lince_interface::host::mapping::Snapshot>().and_then(|snapshot| snapshot.get(pen.last.output))
                    .is_none_or(|mapping| mapping.revision() != pen.mapping)).then_some(*id)).collect::<Vec<_>>();
        for id in ids { self.cancel_tablet_id(id, TabletPhase::Cancel); }
    }
}
