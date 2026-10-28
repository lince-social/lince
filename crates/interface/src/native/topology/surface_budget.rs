use bevy::{math::DVec3, prelude::*};

mod allocation;
pub(crate) mod residency;
mod ui;
pub(crate) use allocation::{Entry, Grant, allocate, bytes};
pub(crate) use ui::{panel, reduction};

pub const PIXEL_BUDGET: u64 = 32 * 1024 * 1024;
pub(crate) const BYTE_BUDGET: u64 = PIXEL_BUDGET * 8;

pub(crate) struct BudgetPlugin;

impl Plugin for BudgetPlugin {
    fn build(&self, app: &mut App) {
        residency::install(app);
        app.add_systems(
            Update,
            ui::update.after(crate::native::actions::ApplyActions),
        );
        app.add_systems(
            PostUpdate,
            super::presentation::synchronize
                .after(crate::native::layout::ResolveFollow)
                .before(bevy::camera::CameraUpdateSystems)
                .before(bevy::ui::UiSystems::Prepare),
        );
    }
}

#[derive(Resource, Default)]
pub(crate) struct Diagnostics {
    pub granted_bytes: u64,
    pub limited_targets: usize,
    pub maximum_extent: u32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeRaster {
    pub pixels: UVec2,
    pub requested: UVec2,
    pub device_limited: bool,
    pub budget_limited: bool,
}

#[derive(Clone, Copy)]
pub struct Request {
    pub size: Vec2,
    pub density: f32,
    pub previous: f32,
    pub visible: bool,
    pub text: bool,
    pub exact: bool,
}

pub fn dimensions(size: Vec2, density: f32) -> (UVec2, f32) {
    let maximum = ((4096.0 / size.max_element()).log2() * 2.0).floor() * 0.5;
    let exponent = (density.max(1.0 / 65536.0).log2() * 2.0).ceil() * 0.5;
    let density = 2.0_f32.powf(exponent.min(maximum));
    ((size * density).ceil().max(Vec2::ONE).as_uvec2(), density)
}

#[cfg(test)]
pub fn plan(requests: &[Request], budget: u64) -> Vec<(UVec2, f32)> {
    let entries: Vec<_> = requests
        .iter()
        .map(|request| Entry {
            request: *request,
            priority: u8::from(request.text),
            bytes_per_pixel: 1,
        })
        .collect();
    allocate(&entries, budget, 4096)
        .into_iter()
        .map(|grant| (grant.pixels, grant.density))
        .collect()
}

pub fn perspective_visible(points: &[DVec3; 8], aspect: f64, tangent: f64) -> bool {
    let horizontal = tangent * aspect;
    let planes: [fn(DVec3, f64, f64) -> f64; 6] = [
        |p, _, _| -p.z - 0.5,
        |p, _, _| p.z + 1_000_000.0,
        |p, h, _| p.x - p.z * h,
        |p, h, _| -p.x - p.z * h,
        |p, _, v| p.y - p.z * v,
        |p, _, v| -p.y - p.z * v,
    ];
    planes.iter().all(|plane| {
        points
            .iter()
            .any(|point| plane(*point, horizontal, tangent) >= 0.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(center: DVec3, radius: f64) -> [DVec3; 8] {
        std::array::from_fn(|index| {
            center
                + DVec3::new(
                    if index & 1 == 0 { -radius } else { radius },
                    if index & 2 == 0 { -radius } else { radius },
                    if index & 4 == 0 { -radius } else { radius },
                )
        })
    }

    #[test]
    fn camera_planes_reject_hidden_objects_but_keep_intersections() {
        let visible = |points| perspective_visible(&points, 16.0 / 9.0, 0.5);
        assert!(visible(bounds(DVec3::new(0.0, 0.0, -10.0), 1.0)));
        assert!(!visible(bounds(DVec3::new(0.0, 0.0, 10.0), 1.0)));
        assert!(!visible(bounds(DVec3::new(100.0, 0.0, -10.0), 1.0)));
        assert!(!visible(bounds(DVec3::new(0.0, 100.0, -10.0), 1.0)));
        assert!(visible(bounds(DVec3::ZERO, 10.0)));
        assert!(visible(bounds(DVec3::new(9.0, 0.0, -10.0), 2.0)));
    }

    #[test]
    fn a_thousand_surfaces_stay_in_budget_even_near_the_camera() {
        let requests: Vec<_> = (0..1000)
            .map(|index| Request {
                size: if index % 2 == 0 {
                    Vec2::splat(300.0)
                } else {
                    Vec2::new(340.0, 640.0)
                },
                density: 1000.0,
                previous: 0.0,
                visible: index % 3 != 0,
                text: index % 5 == 0,
                exact: false,
            })
            .collect();
        let result = plan(&requests, PIXEL_BUDGET);
        assert!(
            result
                .iter()
                .map(|(p, _)| u64::from(p.x) * u64::from(p.y))
                .sum::<u64>()
                <= PIXEL_BUDGET
        );
        for (request, (pixels, density)) in requests.iter().zip(result) {
            assert!(pixels.min_element() > 0 && pixels.max_element() <= 4096);
            if request.visible {
                assert!(
                    (pixels.as_vec2() / density - request.size)
                        .abs()
                        .max_element()
                        <= 1.0 / density
                );
            } else {
                assert_eq!(pixels, UVec2::ONE);
            }
        }
    }

    #[test]
    fn distant_surfaces_shrink_and_small_camera_moves_reuse_resolution() {
        assert_eq!(dimensions(Vec2::splat(300.0), 0.125).0, UVec2::splat(38));
        for density in [0.51, 0.71, 0.99, 1.0] {
            let request = Request {
                size: Vec2::splat(300.0),
                density,
                previous: 1.0,
                visible: true,
                text: true,
                exact: false,
            };
            assert_eq!(plan(&[request], PIXEL_BUDGET)[0], (UVec2::splat(300), 1.0));
        }
    }

    #[test]
    fn exact_pixel_consumers_do_not_inherit_bucket_blur_or_previous_output_density() {
        for density in [1.0, 1.25, 1.5, 2.0] {
            let request = Request {
                size: Vec2::new(160.0, 120.0),
                density,
                previous: 2.0,
                visible: true,
                text: false,
                exact: true,
            };
            assert_eq!(
                plan(&[request], PIXEL_BUDGET)[0],
                ((request.size * density).as_uvec2(), density)
            );
            let reduced = plan(&[request], 1000);
            assert!(u64::from(reduced[0].0.x) * u64::from(reduced[0].0.y) <= 1000);
        }
    }

    #[test]
    fn text_keeps_its_physical_resolution_before_media_is_reduced() {
        let text = Request {
            size: Vec2::new(200.0, 80.0),
            density: 2.0,
            previous: 0.0,
            visible: true,
            text: true,
            exact: false,
        };
        let media = Request {
            size: Vec2::splat(1000.0),
            text: false,
            ..text
        };
        let allocations = plan(&[text, media], 1_000_000);
        assert_eq!(allocations[0], (UVec2::new(400, 160), 2.0));
        assert!(allocations[1].1 < 2.0);
        assert!(
            allocations
                .iter()
                .map(|(size, _)| u64::from(size.x) * u64::from(size.y))
                .sum::<u64>()
                <= 1_000_000
        );
    }
}
