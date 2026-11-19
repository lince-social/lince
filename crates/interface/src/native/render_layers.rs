use std::ops::Range;

#[derive(Clone, Copy)]
pub(crate) struct Slots {
    first: usize,
    pub len: usize,
}

impl Slots {
    const fn new(first: usize, len: usize) -> Self {
        Self { first, len }
    }

    pub fn get(self, index: usize) -> Option<usize> {
        (index < self.len).then(|| self.first + index)
    }

    pub fn indices(self) -> Range<usize> {
        self.first..self.first + self.len
    }
}

pub(crate) const OUTPUTS: Slots = Slots::new(32, 16);
pub(crate) const FIOTE: Slots = Slots::new(48, 16);
pub(crate) const SHADERS: Slots = Slots::new(64, 64);
pub(crate) const OVERVIEW: Slots = Slots::new(128, 8);
pub(crate) const MODELS: Slots = Slots::new(136, 16);

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::visibility::RenderLayers;

    #[test]
    fn reserved_slots_are_bounded_and_all_camera_masks_are_disjoint() {
        let groups = [OUTPUTS, FIOTE, SHADERS, OVERVIEW, MODELS];
        for (index, slots) in groups.iter().enumerate() {
            assert_eq!(slots.get(0), Some(slots.indices().start));
            assert_eq!(slots.get(slots.len - 1), Some(slots.indices().end - 1));
            assert_eq!(slots.get(slots.len), None);
            assert_eq!(slots.get(usize::MAX), None);
            let mask = RenderLayers::from_layers(&slots.indices().collect::<Vec<_>>());
            for layer in 0..32 {
                assert!(!mask.intersects(&RenderLayers::layer(layer)));
            }
            for other in groups.iter().skip(index + 1) {
                let other = RenderLayers::from_layers(&other.indices().collect::<Vec<_>>());
                assert!(!mask.intersects(&other));
            }
        }
    }
}
