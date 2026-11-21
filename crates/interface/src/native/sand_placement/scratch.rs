use super::Placement;
use crate::layout::{Rules, desktop::Geometry};
use std::{array::from_fn, collections::BTreeMap};

pub(crate) struct Home {
    pub workspace: u64,
    pub geometry: Geometry,
    pub rules: Rules,
}

pub(crate) type Homes = BTreeMap<[u8; 16], Home>;

pub(crate) trait Saved {
    fn normalize_scratch(&mut self, homes: &Homes);
}

impl Placement {
    pub(crate) fn normalize_home(
        &mut self,
        workspace: &mut u64,
        position: &mut [f64; 2],
        size: &mut [f32; 2],
        homes: &Homes,
    ) {
        if let Some(layout) = &mut self.layout
            && let Some(home) = homes.get(&layout.id)
        {
            let geometry = home.geometry;
            *workspace = home.workspace;
            *position = from_fn(|axis| geometry.origin[axis] + geometry.size[axis] * 0.5);
            *size = geometry.size.map(|value| value as f32);
            layout.rules = home.rules;
        }
    }
}

pub(crate) fn normalize_all<T: Saved>(saved: &mut [T], homes: &Homes) {
    for saved in saved {
        saved.normalize_scratch(homes);
    }
}

macro_rules! saved {
    ($kind:ty) => {
        impl $crate::native::sand_placement::scratch::Saved for $kind {
            fn normalize_scratch(
                &mut self,
                homes: &$crate::native::sand_placement::scratch::Homes,
            ) {
                let Self {
                    placement,
                    workspace,
                    position,
                    size,
                    ..
                } = self;
                placement.normalize_home(workspace, position, size, homes);
            }
        }
    };
}

pub(crate) use saved;
