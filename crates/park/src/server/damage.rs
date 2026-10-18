use super::*;
use smithay::{
    backend::renderer::utils::{CommitCounter, DamageBag},
    utils::Buffer,
    wayland::compositor::{Damage, RectangleKind},
};

pub(super) struct Tracked {
    pub(super) damage: DamageBag<i32, Buffer>,
    layout: SurfaceLayout,
    size: [i32; 2],
}

impl State {
    pub(super) fn update_damage(&mut self, surface: &WlSurface, changes: Vec<Damage>) {
        let Some(content) = self.buffers.get(surface) else {
            self.damage.remove(surface);
            return;
        };
        let size = content.size;
        let layout = with_states(surface, |states| {
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            let attributes = cached.current();
            let viewport = *states.cached_state.get::<ViewportCachedState>().current();
            SurfaceLayout::new(size, attributes.buffer_scale, attributes.buffer_transform.into(), viewport)
        });
        let Some(layout) = layout else { return };
        let tracked = self.damage.entry(surface.clone()).or_insert_with(|| Tracked { damage: DamageBag::new(16), layout, size });
        if tracked.layout != layout || tracked.size != size {
            tracked.damage.reset();
            tracked.layout = layout;
            tracked.size = size;
        }
        let bounds = Rectangle::from_size((size[0], size[1]).into());
        tracked.damage.add(changes.into_iter().filter_map(|damage| {
            match damage {
                Damage::Buffer(rectangle) => rectangle.intersection(bounds),
                Damage::Surface(rectangle) => surface_damage(layout, size, rectangle),
            }
        }));
    }
}

fn surface_damage(layout: SurfaceLayout, size: [i32; 2], rectangle: Rectangle<i32, Logical>) -> Option<Rectangle<i32, Buffer>> {
    let rectangle = rectangle.to_f64().intersection(Rectangle::from_size((layout.size[0], layout.size[1]).into()))?;
    let corners = [rectangle.loc, (rectangle.loc.x + rectangle.size.w, rectangle.loc.y).into(),
        (rectangle.loc.x, rectangle.loc.y + rectangle.size.h).into(),
        (rectangle.loc.x + rectangle.size.w, rectangle.loc.y + rectangle.size.h).into()];
    let points = corners.map(|point| {
        let local = [point.x / layout.size[0], point.y / layout.size[1]];
        std::array::from_fn::<_, 2, _>(|axis| {
            (f64::from(layout.uv_origin[axis]) + f64::from(layout.uv_x[axis]) * local[0] + f64::from(layout.uv_y[axis]) * local[1]) * f64::from(size[axis])
        })
    });
    let low: [i32; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min).floor().max(0.0) as i32);
    let high: [i32; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max).ceil().min(f64::from(size[axis])) as i32);
    if (0..2).any(|axis| high[axis] <= low[axis]) { return None; }
    Some(Rectangle::new((low[0], low[1]).into(), (high[0] - low[0], high[1] - low[1]).into()))
}

impl SurfaceNode {
    pub fn buffer_damage_since(&self, previous: Option<CommitCounter>) -> Vec<Rectangle<i32, Buffer>> {
        self.damage.damage_since(previous).map(|damage| damage.iter().copied().collect())
            .unwrap_or_else(|| vec![Rectangle::from_size((self.content.size[0], self.content.size[1]).into())])
    }

    pub fn opaque_rectangles(&self) -> Vec<Rectangle<f64, Logical>> {
        let Some(region) = &self.opaque else { return Vec::new() };
        opaque_rectangles(region, self.layout.size)
    }

    pub fn fully_opaque(&self) -> bool {
        Rectangle::from_size((self.layout.size[0], self.layout.size[1]).into())
            .subtract_rects(self.opaque_rectangles()).is_empty()
    }
}

fn opaque_rectangles(region: &RegionAttributes, size: [f64; 2]) -> Vec<Rectangle<f64, Logical>> {
    let bounds = Rectangle::from_size((size[0], size[1]).into());
    let mut result = Vec::new();
    for (kind, rectangle) in &region.rects {
        if let Some(rectangle) = rectangle.to_f64().intersection(bounds) {
            match kind {
                RectangleKind::Add => result.push(rectangle),
                RectangleKind::Subtract => {
                    result = Rectangle::subtract_rects_many(result, [rectangle]);
                }
            }
            if result.len() > 4096 { return Vec::new(); }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_damage_obeys_rotated_viewport_mapping() {
        let layout = SurfaceLayout { size: [50.0, 100.0], uv_origin: [0.0, 1.0], uv_x: [0.0, -1.0], uv_y: [1.0, 0.0] };
        let damage = surface_damage(layout, [200, 100], Rectangle::new((0, 0).into(), (10, 20).into())).unwrap();
        assert_eq!(damage, Rectangle::new((0, 80).into(), (40, 20).into()));
    }

    #[test]
    fn opaque_regions_preserve_subtracted_holes() {
        let region = RegionAttributes { rects: vec![
            (RectangleKind::Add, Rectangle::new((0, 0).into(), (100, 100).into())),
            (RectangleKind::Subtract, Rectangle::new((40, 40).into(), (20, 20).into())),
        ] };
        let opaque = opaque_rectangles(&region, [100.0; 2]);
        assert!(!opaque.iter().any(|rectangle| rectangle.contains((50.0, 50.0))));
        assert!(opaque.iter().any(|rectangle| rectangle.contains((10.0, 10.0))));
    }
}
