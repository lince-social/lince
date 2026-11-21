//! Retain the authored value when a token changes the scale of a pixel measurement.

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct PixelScale(Option<Applied>);

#[derive(Clone, Copy, PartialEq)]
struct Applied {
    authored: f32,
    scaled: f32,
}

impl PixelScale {
    pub(crate) fn needed(&self, current: f32) -> bool {
        current != 0.0 || self.0.is_some_and(|previous| previous.authored != 0.0)
    }

    pub(crate) fn scale(&mut self, current: f32, ratio: f32) -> f32 {
        // A different value is a new authored measurement.
        let authored = self
            .0
            .filter(|previous| current == previous.scaled)
            .map_or(current, |previous| previous.authored);
        let scaled = authored * ratio;
        self.0 = Some(Applied { authored, scaled });
        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::PixelScale;

    #[test]
    fn rescales_from_the_authored_value_and_observes_edits() {
        let mut scale = PixelScale::default();
        assert!(!scale.needed(0.0));
        assert!(scale.needed(4.0));
        assert_eq!(scale.scale(4.0, 2.0), 8.0);
        assert_eq!(scale.scale(8.0, 3.0), 12.0);
        assert_eq!(scale.scale(6.0, 3.0), 18.0);
        assert_eq!(scale.scale(18.0, 0.0), 0.0);
        assert!(scale.needed(0.0));
        assert_eq!(scale.scale(0.0, 2.0), 12.0);
        assert_eq!(scale.scale(0.0, 2.0), 0.0);
        assert!(!scale.needed(0.0));
    }
}
