use super::*;
use lince_interface::host::{GeometrySnapshot, ItemId, OutputViewport, Rectangle, magnification, mapping::{Mapping, Snapshot}};

impl Presentation {
    pub(crate) fn magnification_caret(&mut self, caret: Option<magnification::Caret>) { self.magnification_caret = caret; }

    pub(super) fn finish_input(&mut self, geometry: Option<&GeometrySnapshot>) -> Result<InputSnapshot, ProbeFailure> {
        let mut input = self.input_snapshot(geometry);
        input.mappings = Arc::new(self.output_mappings(geometry, &input)?);
        if input.drop_target.as_ref().is_some_and(|target| target.mapping_revision != input.mappings.get(target.output).map(Mapping::revision)) {
            input.drop_target = None;
        }
        self.publish_accessibility(geometry);
        input.accessibility = self.accessibility.as_ref().and_then(accessibility::Bridge::receipt);
        Ok(input)
    }

    fn output_mappings(&mut self, geometry: Option<&GeometrySnapshot>, input: &InputSnapshot) -> Result<Snapshot, ProbeFailure> {
        let previous = self.app.world().get_resource::<Snapshot>().cloned().unwrap_or_default();
        let revision = previous.revision().checked_add(1).ok_or_else(|| input_failure("Output mapping revisions are exhausted."))?;
        let reveal = native::accessibility::reveal_target(self.app.world_mut());
        let reveal_bounds = reveal.and_then(|entity| {
            let context = native::accessibility::context(self.app.world(), entity, true)?;
            let view = self.accessibility_views(geometry).into_iter().find(|view| view.root == Some(context.root) && context.output().is_none_or(|output| output == view.output))?;
            let bounds = crate::native_input::projection::accessibility_bounds(self.app.world(), &view, geometry, entity, None)?;
            let output = geometry?.outputs.iter().find(|output| output.id == view.output)?;
            Some((view.output, bounds.intersection(output.bounds)?))
        });
        if reveal_bounds.is_some() && let Some(entity) = reveal { native::accessibility::revealed(self.app.world_mut(), entity); }
        let pointer_epoch = native::layout::desktop::focus::pointer_generation(self.app.world());
        let pointer_changed = self.magnification_pointer_epoch != Some(pointer_epoch);
        self.magnification_pointer_epoch = Some(pointer_epoch);
        let pointer = self.input_window().ok().and_then(|window| {
            let window = self.app.world().get::<Window>(window)?;
            let point = window.cursor_position()?.as_dvec2() * f64::from(window.resolution.scale_factor());
            Some(self.input_mapping().logical_point(point.to_array()))
        });
        let native_caret = input.editor.as_ref().filter(|_| input.keyboard).map(|editor| (
            self.output_state.assignments.focused().or_else(|| self.output_view.as_ref().map(|output| output.id)).unwrap_or(lince_interface::host::OutputId(1)),
            [f64::from(editor.caret[0]) + f64::from(editor.caret[2]) * 0.5, f64::from(editor.caret[1]) + f64::from(editor.caret[3]) * 0.5],
        ));
        let application_caret = self.magnification_caret.filter(|_| !input.keyboard).and_then(|caret| {
            let geometry = geometry?;
            match caret {
                magnification::Caret::Application { surface, rectangle } => {
                    if geometry.focused != Some(ItemId::Application(surface)) { return None; }
                    let item = geometry.items.iter().find(|item| item.id == ItemId::Application(surface))?;
                    Some((item.output, item.mapping.canvas_point(std::array::from_fn(|axis| rectangle.origin()[axis] + rectangle.size()[axis] * 0.5))?))
                }
                magnification::Caret::Layer { output, rectangle } => {
                    geometry.outputs.iter().find(|viewport| viewport.id == output)?;
                    Some((output, std::array::from_fn(|axis| rectangle.origin()[axis] + rectangle.size()[axis] * 0.5)))
                }
            }
        });
        let warp = input.cursor_warp.and_then(|warp| {
            let geometry = geometry?;
            let item = geometry.items.iter().find(|item| item.id == warp.target.item && item.output == warp.target.output)?;
            Some((item.output, item.mapping.canvas_point(std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5))?))
        });
        let settings = self.app.world().get_resource::<magnification::Snapshot>().filter(|settings| settings.available);
        let mut mappings = Vec::new();
        for output in geometry.into_iter().flat_map(|geometry| geometry.outputs.iter()) {
            let factor = settings.and_then(|settings| settings.head(output.id)).map_or(1.0, |head| head.requested);
            if let Some((_, bounds)) = reveal_bounds.filter(|(id, _)| *id == output.id) {
                let old = previous.get(output.id).filter(|old| compatible(output, factor, old));
                if let Some(center) = reveal_anchor(output, factor, old, bounds) {
                    let mapping = Mapping::new(output, output.rotation, factor, center, revision).map_err(input_failure)?;
                    mappings.push(old.filter(|old| old.same_mapping(&mapping)).cloned().unwrap_or(mapping));
                } else if let Some(old) = old { mappings.push(old.clone()); }
                continue;
            }
            let target = native_caret.or(application_caret).or(warp).filter(|(id, point)| *id == output.id && output.bounds.contains(*point)).map(|(_, point)| point)
                .or_else(|| pointer.filter(|point| output.bounds.contains(*point)
                    && (pointer_changed || previous.get(output.id).is_none_or(|old| old.magnification() != factor))));
            if let Some(old) = previous.get(output.id).filter(|old| retain(output, factor, old, target)) {
                mappings.push(old.clone());
                continue;
            }
            let anchor = anchor(output, factor, previous.get(output.id), target);
            let mapping = Mapping::new(output, output.rotation, factor, anchor, revision).map_err(input_failure)?;
            mappings.push(previous.get(output.id).filter(|old| old.same_mapping(&mapping)).cloned().unwrap_or(mapping));
        }
        if mappings.len() == previous.outputs().len()
            && mappings.iter().all(|mapping| previous.get(mapping.output()).is_some_and(|old| old.same_mapping(mapping)))
        { return Ok(previous); }
        let snapshot = Snapshot::new(revision, mappings).map_err(input_failure)?;
        self.app.world_mut().insert_resource(snapshot.clone());
        Ok(snapshot)
    }
}

impl Frame {
    #[cfg(feature = "nested")]
    pub(crate) fn has_output_image(&self, output: lince_interface::host::OutputId) -> bool {
        self.output == Some(output) && self.image.is_some() || self.outputs.iter().any(|candidate| candidate.id == output)
    }
}

fn retain(output: &OutputViewport, factor: f64, previous: &Mapping, target: Option<[f64; 2]>) -> bool {
    if !compatible(output, factor, previous) { return false; }
    if factor == 1.0 || target.is_none() { return true; }
    let Some(point) = target.and_then(|target| previous.logical_to_physical(target)) else { return false; };
    (0..2).all(|axis| point[axis] >= f64::from(output.physical_size[axis]) * 0.15 - 1e-6
        && point[axis] <= f64::from(output.physical_size[axis]) * 0.85 + 1e-6)
}

fn compatible(output: &OutputViewport, factor: f64, previous: &Mapping) -> bool {
    previous.magnification() == factor && previous.logical_bounds() == output.bounds && previous.physical_size() == output.physical_size
        && previous.raster() == output.raster && previous.rotation() == output.rotation
}

fn reveal_anchor(output: &OutputViewport, factor: f64, previous: Option<&Mapping>, target: Rectangle) -> Option<[f64; 2]> {
    let visible = previous.and_then(|mapping| mapping.logical_rectangle(Rectangle::new([0.0; 2], output.physical_size.map(f64::from))?));
    let visible_size = output.bounds.size().map(|size| size / factor);
    let current: Option<[f64; 2]> = visible.map(|visible| std::array::from_fn(|axis| visible.origin()[axis] + visible.size()[axis] * 0.5));
    let next: [f64; 2] = std::array::from_fn(|axis| {
        let margin = visible_size[axis] * 0.05;
        let size = target.size()[axis];
        let start = target.origin()[axis];
        let end = start + size;
        let center = current.map(|center| center[axis]).unwrap_or(start + size * 0.5);
        let low = center - visible_size[axis] * 0.5 + margin;
        let high = center + visible_size[axis] * 0.5 - margin;
        if size > high - low || start < low { start + visible_size[axis] * 0.5 - margin }
        else if end > high { end - visible_size[axis] * 0.5 + margin } else { center }
    });
    if current.is_some_and(|current| (0..2).all(|axis| (next[axis] - current[axis]).abs() <= 1e-9)) { None } else { Some(next) }
}

fn anchor(output: &OutputViewport, factor: f64, previous: Option<&Mapping>, target: Option<[f64; 2]>) -> [f64; 2] {
    let center = std::array::from_fn(|axis| output.bounds.origin()[axis] + output.bounds.size()[axis] * 0.5);
    let Some(previous) = previous.filter(|previous| compatible(output, factor, previous))
    else { return target.unwrap_or(center); };
    let Some(physical) = Rectangle::new([0.0; 2], output.physical_size.map(f64::from)) else { return center; };
    let Some(visible) = previous.logical_rectangle(physical) else { return center; };
    let center = std::array::from_fn(|axis| visible.origin()[axis] + visible.size()[axis] * 0.5);
    let Some(target) = target else { return center; };
    std::array::from_fn(|axis| {
        let margin = visible.size()[axis] * 0.15;
        let low = visible.origin()[axis] + margin;
        let high = visible.origin()[axis] + visible.size()[axis] - margin;
        center[axis] + if target[axis] < low { target[axis] - low } else if target[axis] > high { target[axis] - high } else { 0.0 }
    })
}

#[cfg(test)]
mod tests;
