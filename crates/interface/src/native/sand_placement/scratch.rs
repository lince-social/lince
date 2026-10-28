use super::Placement;
use crate::layout::{Rules, desktop::Geometry};
use std::collections::BTreeMap;

pub(crate) struct Home {
    pub workspace: u64,
    pub geometry: Geometry,
    pub rules: Rules,
}

pub(crate) type Homes = BTreeMap<[u8; 16], Home>;

pub(crate) trait Saved {
    fn normalize_scratch(&mut self, homes: &Homes);
}

pub(crate) fn normalize(placement: &mut Placement, workspace: &mut u64, position: &mut [f64; 2], size: &mut [f32; 2], homes: &Homes) {
    let Some(layout) = &mut placement.layout else { return; };
    let Some(home) = homes.get(&layout.id) else { return; };
    *workspace = home.workspace;
    *position = std::array::from_fn(|axis| home.geometry.origin[axis] + home.geometry.size[axis] * 0.5);
    *size = home.geometry.size.map(|value| value as f32);
    layout.rules = home.rules;
}

pub(crate) fn normalize_all<T: Saved>(saved: &mut [T], homes: &Homes) {
    for saved in saved { saved.normalize_scratch(homes); }
}

macro_rules! saved {
    ($kind:ty) => {
        impl crate::native::sand_placement::scratch::Saved for $kind {
            fn normalize_scratch(&mut self, homes: &crate::native::sand_placement::scratch::Homes) {
                crate::native::sand_placement::scratch::normalize(&mut self.placement, &mut self.workspace, &mut self.position, &mut self.size, homes);
            }
        }
    };
}

pub(crate) use saved;
