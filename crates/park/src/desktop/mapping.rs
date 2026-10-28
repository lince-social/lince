use super::*;
use lince_interface::host::mapping::Mapping;

impl Desktop {
    pub(super) fn input_mapping_stamp(&self, point: [f64; 2]) -> Option<(lince_interface::host::OutputId, u64)> {
        let current = self.frame_at(point)?;
        let output = current.frame.geometry.as_ref()?.outputs.iter().find(|output| output.bounds.contains(point))?;
        Some((output.id, current.frame.input.mappings.get(output.id)?.revision()))
    }

    fn displayed_mapping(&self, point: [f64; 2]) -> Option<&Mapping> {
        self.output_frames.iter().find_map(|(id, (frame, _))| {
            frame.frame.geometry.as_ref()?.outputs.iter().find(|output| output.id == *id && output.bounds.contains(point))?;
            frame.frame.input.mappings.get(*id)
        }).or_else(|| {
            let current = self.current.as_ref()?;
            let output = current.frame.geometry.as_ref()?.outputs.iter().find(|output| output.bounds.contains(point))?;
            current.frame.input.mappings.get(output.id)
        })
    }

    pub(super) fn logical_point(&self, point: [f64; 2]) -> [f64; 2] {
        self.displayed_mapping(point).and_then(|mapping| mapping.displayed_to_logical(point)).unwrap_or(point)
    }

    pub(super) fn displayed_point(&self, point: [f64; 2]) -> [f64; 2] {
        self.frame_at(point).and_then(|frame| {
            let output = frame.frame.geometry.as_ref()?.outputs.iter().find(|output| output.bounds.contains(point))?;
            frame.frame.input.mappings.get(output.id)?.logical_to_displayed(point)
        }).unwrap_or(point)
    }

    pub(crate) fn displayed_pointer(&self) -> [f64; 2] { self.display_pointer.unwrap_or_else(|| self.displayed_point(self.pointer)) }

    pub(super) fn refresh_pointer_mapping(&mut self) {
        let displayed = self.display_pointer.unwrap_or(self.pointer);
        let point = self.logical_point(displayed);
        if self.pointer != point { self.pointer = point; self.pointer_sent = None; }
        self.display_pointer = Some(displayed);
    }

    #[cfg(any(feature = "session", all(test, feature = "server-tests")))]
    pub(super) fn logical_delta(&self, delta: [f64; 2]) -> [f64; 2] {
        let point = self.displayed_pointer();
        let Some(mapping) = self.displayed_mapping(point) else { return delta; };
        let Some(first) = mapping.displayed_to_logical(point) else { return delta; };
        let Some(last) = mapping.displayed_to_logical(std::array::from_fn(|axis| point[axis] + delta[axis])) else { return delta; };
        std::array::from_fn(|axis| last[axis] - first[axis])
    }
}
