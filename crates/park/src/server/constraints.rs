use super::*;
use smithay::{
    input::pointer::PointerHandle,
    utils::Point,
    wayland::pointer_constraints::{PointerConstraint, PointerConstraintsHandler, with_pointer_constraint},
};

impl State {
    fn constraint_eligible(&self, surface: &WlSurface) -> bool {
        if self.lock.owner.is_some() || self.controls_suppressed.as_ref().is_some_and(|suppressed| self.input_root(suppressed) == self.input_root(surface)) {
            return false;
        }
        let keyboard = self.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus());
        let root = self.input_root(surface);
        surface.is_alive() && root.is_some()
            && root == keyboard.as_ref().and_then(|surface| self.input_root(surface))
    }

    pub(super) fn refresh_constraints(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else { return };
        let Some(surface) = pointer.current_focus() else { return };
        let eligible = self.constraint_eligible(&surface);
        let position = pointer.current_location();
        with_pointer_constraint(&surface, &pointer, |constraint| {
            if let Some(constraint) = constraint {
                if !eligible && constraint.is_active() {
                    constraint.deactivate();
                } else if eligible && !constraint.is_active()
                    && constraint.region().is_none_or(|region| input_contains(region, [position.x, position.y])) {
                    constraint.activate();
                }
            }
        });
    }
}

impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, _: &WlSurface, _: &PointerHandle<Self>) {
        self.refresh_constraints();
    }

    fn cursor_position_hint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>, location: Point<f64, Logical>) {
        if !location.x.is_finite() || !location.y.is_finite() || !surface.is_alive() {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(constraint) = constraint.filter(|constraint| constraint.is_active()) {
                    constraint.deactivate();
                }
            });
        }
    }
}

impl Server {
    pub fn release_client_controls(&mut self) {
        self.state.controls_suppressed = self.state.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus());
        for inhibitor in &self.state.shortcut_inhibitors {
            if inhibitor.is_active() { inhibitor.inactivate(); }
        }
        if let Some(pointer) = self.state.seat.get_pointer()
            && let Some(surface) = pointer.current_focus() {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                if let Some(constraint) = constraint.filter(|constraint| constraint.is_active()) {
                    constraint.deactivate();
                }
            });
        }
    }

    pub fn pointer_locked(&self) -> bool {
        if self.locked() { return false; }
        let Some(pointer) = self.state.seat.get_pointer() else { return false };
        let Some(surface) = pointer.current_focus() else { return false };
        with_pointer_constraint(&surface, &pointer, |constraint| {
            constraint.is_some_and(|constraint| constraint.is_active() && matches!(&*constraint, PointerConstraint::Locked(_)))
        })
    }

    pub fn constrain_pointer(&self, surface: &WlSurface, previous: [f64; 2], proposed: [f64; 2], size: [f64; 2]) -> [f64; 2] {
        if !self.state.constraint_eligible(surface) { return proposed; }
        let Some(pointer) = self.state.seat.get_pointer() else { return proposed };
        if pointer.current_focus().as_ref() != Some(surface) { return proposed; }
        let input = with_states(surface, |states| states.cached_state.get::<SurfaceAttributes>().current().input_region.clone());
        with_pointer_constraint(surface, &pointer, |constraint| {
            let Some(constraint) = constraint.filter(|constraint| constraint.is_active()) else { return proposed };
            if matches!(&*constraint, PointerConstraint::Locked(_)) { return previous; }
            clip_segment(previous, proposed, size, constraint.region(), input.as_ref())
        })
    }
}

fn clip_segment(from: [f64; 2], to: [f64; 2], size: [f64; 2], region: Option<&RegionAttributes>, input: Option<&RegionAttributes>) -> [f64; 2] {
    if from.iter().chain(&to).chain(&size).any(|value| !value.is_finite()) || size.iter().any(|size| *size <= 0.0) {
        return from;
    }
    let allowed = |point: [f64; 2]| (0..2).all(|axis| point[axis] >= 0.0 && point[axis] < size[axis])
        && region.is_none_or(|region| input_contains(region, point))
        && input.is_none_or(|region| input_contains(region, point));
    if !allowed(from) { return from; }
    let delta: [f64; 2] = std::array::from_fn(|axis| to[axis] - from[axis]);
    let mut stops = vec![0.0, 1.0];
    let mut edge = |axis: usize, value: f64| {
        if delta[axis] != 0.0 {
            let time = (value - from[axis]) / delta[axis];
            if time > 0.0 && time < 1.0 { stops.push(time); }
        }
    };
    for axis in 0..2 { edge(axis, 0.0); edge(axis, size[axis]); }
    for region in [region, input].into_iter().flatten() {
        for (_, rectangle) in &region.rects {
            edge(0, f64::from(rectangle.loc.x));
            edge(0, f64::from(rectangle.loc.x) + f64::from(rectangle.size.w));
            edge(1, f64::from(rectangle.loc.y));
            edge(1, f64::from(rectangle.loc.y) + f64::from(rectangle.size.h));
        }
    }
    stops.sort_by(f64::total_cmp);
    let point = |time: f64| std::array::from_fn(|axis| from[axis] + delta[axis] * time);
    for span in stops.windows(2) {
        if !allowed(point((span[0] + span[1]) * 0.5)) {
            return point((span[0] - 0.000001).max(0.0));
        }
    }
    if allowed(to) { to } else { point((1.0_f64 - 0.000001).max(0.0)) }
}

smithay::delegate_pointer_constraints!(State);

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::wayland::compositor::RectangleKind;

    #[test]
    fn confinement_does_not_jump_across_excluded_regions() {
        let region = RegionAttributes { rects: vec![
            (RectangleKind::Add, Rectangle::new((0, 0).into(), (100, 100).into())),
            (RectangleKind::Subtract, Rectangle::new((40, 0).into(), (20, 100).into())),
        ] };
        let clipped = clip_segment([10.0, 20.0], [90.0, 20.0], [100.0; 2], Some(&region), None);
        assert!(clipped[0] < 40.0 && clipped[0] > 39.0);
        assert_eq!(clip_segment([10.0, 20.0], [30.0, 20.0], [100.0; 2], Some(&region), None), [30.0, 20.0]);
    }
}
