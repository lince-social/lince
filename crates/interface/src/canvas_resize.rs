use crate::canvas::CanvasItem;
use bevy::{
    prelude::*,
    window::{CursorIcon, SystemCursorIcon},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edges {
    pub x: i8,
    pub y: i8,
}

impl Edges {
    pub fn at(point: Vec2, bounds: Rect) -> Self {
        if !point.is_finite()
            || !bounds.min.is_finite()
            || !bounds.max.is_finite()
            || !bounds.contains(point)
        {
            return Self::default();
        }
        let tolerance = Vec2::splat(12.0).min(bounds.size() * 0.25);
        Self {
            x: if point.x - bounds.min.x <= tolerance.x {
                -1
            } else if bounds.max.x - point.x <= tolerance.x {
                1
            } else {
                0
            },
            y: if point.y - bounds.min.y <= tolerance.y {
                -1
            } else if bounds.max.y - point.y <= tolerance.y {
                1
            } else {
                0
            },
        }
    }

    pub fn cursor(self) -> Option<SystemCursorIcon> {
        match (self.x, self.y) {
            (0, 0) => None,
            (_, 0) => Some(SystemCursorIcon::EwResize),
            (0, _) => Some(SystemCursorIcon::NsResize),
            (x, y) if x == y => Some(SystemCursorIcon::NwseResize),
            _ => Some(SystemCursorIcon::NeswResize),
        }
    }
}

pub(crate) struct Resize {
    pub edges: Edges,
    pub zoom: f64,
    pub start: Vec2,
    pub original: CanvasItem,
    pub minimum: Vec2,
}

impl Resize {
    pub fn apply_square(&self, point: Vec2) -> Option<CanvasItem> {
        let mut item = self.apply(point)?;
        let axis = if self.edges.x == 0 {
            1
        } else if self.edges.y == 0 {
            0
        } else if (item.size.x - self.original.size.x).abs()
            >= (item.size.y - self.original.size.y).abs()
        {
            0
        } else {
            1
        };
        item.size = Vec2::splat(item.size[axis]);
        item.position = self.original.position
            + (item.size - self.original.size).as_dvec2()
                * bevy::math::DVec2::new(f64::from(self.edges.x), f64::from(self.edges.y))
                * 0.5;
        Some(item)
    }

    pub fn apply(&self, point: Vec2) -> Option<CanvasItem> {
        if !point.is_finite()
            || !self.start.is_finite()
            || !self.zoom.is_finite()
            || self.zoom <= 0.0
            || !self.original.position.is_finite()
            || !self.original.size.is_finite()
        {
            return None;
        }
        let delta = (point - self.start).as_dvec2() / self.zoom;
        let original = self.original.size.as_dvec2();
        let mut size = original;
        let mut center = self.original.position;
        for (axis, edge) in [self.edges.x, self.edges.y].into_iter().enumerate() {
            if edge != 0 {
                let minimum = f64::from(self.minimum[axis]).clamp(48.0, 100_000.0);
                size[axis] =
                    (original[axis] + delta[axis] * f64::from(edge)).clamp(minimum, 100_000.0);
                center[axis] += (size[axis] - original[axis]) * f64::from(edge) * 0.5;
            }
        }
        (center.is_finite() && size.is_finite()).then_some(CanvasItem {
            position: center,
            size: size.as_vec2(),
        })
    }
}

#[derive(Resource, Default)]
pub(crate) struct ResizeCursor(Option<(Entity, Option<CursorIcon>, SystemCursorIcon)>);

impl ResizeCursor {
    pub fn update(
        &mut self,
        next: Option<(Entity, SystemCursorIcon)>,
        windows: &Query<(Entity, &Window, Option<&CursorIcon>)>,
        commands: &mut Commands,
    ) {
        if let Some((entity, previous, applied)) = &self.0
            && next.map(|(entity, _)| entity) != Some(*entity)
        {
            if let Ok((_, _, current)) = windows.get(*entity)
                && current == Some(&CursorIcon::System(*applied))
            {
                if let Some(previous) = previous {
                    commands.entity(*entity).insert(previous.clone());
                } else {
                    commands.entity(*entity).remove::<CursorIcon>();
                }
            }
            self.0 = None;
        }
        if let Some((entity, icon)) = next
            && let Ok((_, _, current)) = windows.get(entity)
        {
            if self.0.is_none() {
                self.0 = Some((entity, current.cloned(), icon));
            }
            if current != Some(&CursorIcon::System(icon)) {
                commands.entity(entity).insert(CursorIcon::System(icon));
            }
            if let Some(state) = &mut self.0 {
                state.2 = icon;
            }
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::math::DVec2;

    #[cfg_attr(test, test)]
    fn regular_areas_keep_proportions_and_opposite_edges_at_every_zoom() {
        for zoom in [0.1, 1.0, 3.0] {
            for edges in [
                Edges { x: 1, y: 0 },
                Edges { x: -1, y: -1 },
                Edges { x: 0, y: 1 },
            ] {
                let resize = Resize {
                    edges,
                    zoom,
                    start: Vec2::ZERO,
                    original: CanvasItem {
                        position: bevy::math::DVec2::splat(1e12),
                        size: Vec2::splat(200.0),
                    },
                    minimum: Vec2::splat(48.0),
                };
                let item = resize.apply_square(Vec2::new(40.0, 70.0)).unwrap();
                assert_eq!(item.size.x, item.size.y);
                for (axis, edge) in [edges.x, edges.y].into_iter().enumerate() {
                    if edge != 0 {
                        assert_eq!(
                            item.position[axis]
                                - f64::from(item.size[axis]) * f64::from(edge) * 0.5,
                            resize.original.position[axis] - 100.0 * f64::from(edge)
                        );
                    }
                }
            }
        }
    }

    #[cfg_attr(test, test)]
    fn resize_hotspots_show_the_matching_cursor_on_every_side_and_corner() {
        let bounds = Rect::from_corners(Vec2::ZERO, Vec2::splat(200.0));
        for (point, icon) in [
            (Vec2::new(10.0, 100.0), SystemCursorIcon::EwResize),
            (Vec2::new(190.0, 100.0), SystemCursorIcon::EwResize),
            (Vec2::new(100.0, 10.0), SystemCursorIcon::NsResize),
            (Vec2::new(100.0, 190.0), SystemCursorIcon::NsResize),
            (Vec2::splat(10.0), SystemCursorIcon::NwseResize),
            (Vec2::splat(190.0), SystemCursorIcon::NwseResize),
            (Vec2::new(10.0, 190.0), SystemCursorIcon::NeswResize),
            (Vec2::new(190.0, 10.0), SystemCursorIcon::NeswResize),
        ] {
            assert_eq!(Edges::at(point, bounds).cursor(), Some(icon));
        }
        assert_eq!(Edges::at(Vec2::splat(100.0), bounds).cursor(), None);
        assert_eq!(Edges::at(Vec2::splat(-1.0), bounds).cursor(), None);
    }

    #[cfg_attr(test, test)]
    fn every_edge_and_corner_keeps_the_opposite_side_fixed_at_each_zoom() {
        for zoom in [0.1, 1.0, 3.0] {
            for x in -1..=1 {
                for y in -1..=1 {
                    let original = CanvasItem {
                        position: DVec2::new(40.0, -30.0),
                        size: Vec2::new(200.0, 160.0),
                    };
                    let resize = Resize {
                        edges: Edges { x, y },
                        zoom,
                        start: Vec2::ZERO,
                        original,
                        minimum: Vec2::splat(48.0),
                    };
                    let next = resize
                        .apply(Vec2::new(30.0 * x as f32, 20.0 * y as f32) * zoom as f32)
                        .unwrap();
                    for (axis, edge) in [x, y].into_iter().enumerate() {
                        let old_fixed = original.position[axis]
                            - f64::from(original.size[axis]) * f64::from(edge) * 0.5;
                        let new_fixed = next.position[axis]
                            - f64::from(next.size[axis]) * f64::from(edge) * 0.5;
                        assert!((old_fixed - new_fixed).abs() < 0.0001);
                        assert!(
                            (next.size[axis]
                                - original.size[axis]
                                - if edge == 0 {
                                    0.0
                                } else if axis == 0 {
                                    30.0
                                } else {
                                    20.0
                                })
                            .abs()
                                < 0.001
                        );
                    }
                }
            }
        }
    }

    #[cfg_attr(test, test)]
    fn resize_limits_do_not_flip_or_lose_pointer_alignment() {
        let resize = Resize {
            edges: Edges { x: -1, y: -1 },
            zoom: 1.0,
            start: Vec2::ZERO,
            original: CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(200.0),
            },
            minimum: Vec2::splat(48.0),
        };
        assert_eq!(
            resize.apply(Vec2::splat(1000.0)).unwrap().size,
            Vec2::splat(48.0)
        );
        assert_eq!(
            resize.apply(Vec2::splat(-1e8)).unwrap().size,
            Vec2::splat(100_000.0)
        );
        assert_eq!(resize.apply(Vec2::ZERO).unwrap().size, resize.original.size);
        assert!(resize.apply(Vec2::splat(f32::NAN)).is_none());
        let bounds = Rect::from_corners(Vec2::ZERO, Vec2::splat(100.0));
        assert_eq!(
            Edges::at(Vec2::new(2.0, 98.0), bounds).cursor(),
            Some(SystemCursorIcon::NeswResize)
        );
        assert_eq!(Edges::at(Vec2::splat(50.0), bounds).cursor(), None);
    }

    crate::laboratory_cases! {
        regular_areas_keep_proportions_and_opposite_edges_at_every_zoom,
        resize_hotspots_show_the_matching_cursor_on_every_side_and_corner,
        every_edge_and_corner_keeps_the_opposite_side_fixed_at_each_zoom,
        resize_limits_do_not_flip_or_lose_pointer_alignment,
    }
}
