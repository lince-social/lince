use bevy::{math::DVec3, prelude::*};

pub const PIXEL_BUDGET: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Request {
    pub size: Vec2,
    pub density: f32,
    pub previous: f32,
    pub visible: bool,
}

pub fn dimensions(size: Vec2, density: f32) -> (UVec2, f32) {
    let maximum = ((4096.0 / size.max_element()).log2() * 2.0).floor() * 0.5;
    let exponent = (density.max(1.0 / 65536.0).log2() * 2.0).ceil() * 0.5;
    let density = 2.0_f32.powf(exponent.min(maximum));
    ((size * density).ceil().max(Vec2::ONE).as_uvec2(), density)
}

pub fn plan(requests: &[Request], budget: u64) -> Vec<(UVec2, f32)> {
    let mut result: Vec<_> = requests
        .iter()
        .map(|request| {
            if !request.visible {
                return (UVec2::ONE, 1.0);
            }
            let density = if request.previous >= request.density
                && request.previous <= request.density * 2.0
            {
                request.previous
            } else {
                request.density
            };
            dimensions(request.size, density)
        })
        .collect();
    loop {
        let pixels: u64 = result
            .iter()
            .map(|(size, _)| u64::from(size.x) * u64::from(size.y))
            .sum();
        if pixels <= budget.max(requests.len() as u64) {
            break;
        }
        let reduction =
            ((budget as f64 / pixels as f64).sqrt() as f32).min(std::f32::consts::FRAC_1_SQRT_2);
        for (request, (pixels, density)) in requests.iter().zip(&mut result) {
            if request.visible {
                *density *= reduction;
                let exponent = (*density).log2().mul_add(2.0, -0.0001).floor() * 0.5;
                *density = 2.0_f32.powf(exponent);
                *pixels = (request.size * *density).ceil().max(Vec2::ONE).as_uvec2();
            }
        }
    }
    result
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
            };
            assert_eq!(plan(&[request], PIXEL_BUDGET)[0], (UVec2::splat(300), 1.0));
        }
    }
}
