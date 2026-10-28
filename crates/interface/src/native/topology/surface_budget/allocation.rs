use super::Request;
use bevy::prelude::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub request: Request,
    pub priority: u8,
    pub bytes_per_pixel: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Grant {
    pub pixels: UVec2,
    pub density: f32,
    pub requested: UVec2,
    pub device_limited: bool,
    pub budget_limited: bool,
}

fn extent(size: Vec2, density: f64, maximum: u32) -> UVec2 {
    (size.as_dvec2() * density)
        .ceil()
        .clamp(
            bevy::math::DVec2::ONE,
            bevy::math::DVec2::splat(f64::from(maximum.max(1))),
        )
        .as_uvec2()
}

pub(crate) fn bytes(entry: Entry, pixels: UVec2) -> u64 {
    u64::from(pixels.x)
        .saturating_mul(u64::from(pixels.y))
        .saturating_mul(u64::from(entry.bytes_per_pixel.max(1)))
}

fn initial(entry: Entry, maximum: u32) -> Grant {
    let request = entry.request;
    if !request.visible
        || !request.size.is_finite()
        || request.size.min_element() <= 0.0
        || !request.density.is_finite()
        || request.density <= 0.0
    {
        return Grant {
            pixels: UVec2::ONE,
            density: 1.0,
            requested: UVec2::ONE,
            device_limited: false,
            budget_limited: false,
        };
    }
    let cap = f64::from(maximum.max(1)) / f64::from(request.size.max_element());
    let mut density = if !request.exact
        && request.previous.is_finite()
        && request.previous >= request.density
        && request.previous <= request.density * 2.0
    {
        f64::from(request.previous)
    } else {
        f64::from(request.density)
    };
    if !request.exact {
        density = 2.0_f64.powf((density.min(cap).log2() * 2.0).ceil() * 0.5);
    }
    density = density.min(cap);
    Grant {
        pixels: extent(request.size, density, maximum),
        density: density as f32,
        requested: extent(request.size, f64::from(request.density), u32::MAX),
        device_limited: f64::from(request.density) > cap,
        budget_limited: false,
    }
}

pub(crate) fn allocate(entries: &[Entry], budget: u64, maximum: u32) -> Vec<Grant> {
    let mut grants: Vec<_> = entries
        .iter()
        .map(|entry| initial(*entry, maximum))
        .collect();
    for priority in entries
        .iter()
        .map(|entry| entry.priority)
        .collect::<BTreeSet<_>>()
    {
        let total = entries
            .iter()
            .zip(&grants)
            .fold(0_u64, |sum, (entry, grant)| {
                sum.saturating_add(bytes(*entry, grant.pixels))
            });
        if total <= budget {
            break;
        }
        let indices: Vec<_> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.priority == priority
                    && entry.request.visible
                    && entry.request.size.is_finite()
                    && entry.request.size.min_element() > 0.0
                    && entry.request.density.is_finite()
                    && entry.request.density > 0.0
            })
            .map(|(index, _)| index)
            .collect();
        let group = indices.iter().fold(0_u64, |sum, index| {
            sum.saturating_add(bytes(entries[*index], grants[*index].pixels))
        });
        let available = budget.saturating_sub(total.saturating_sub(group));
        let cost = |factor: f32| {
            indices.iter().fold(0_u64, |sum, index| {
                let entry = entries[*index];
                let pixels = extent(
                    entry.request.size,
                    f64::from((grants[*index].density * factor).max(f32::MIN_POSITIVE)),
                    maximum,
                );
                sum.saturating_add(bytes(entry, pixels))
            })
        };
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..32 {
            let middle = (low + high) * 0.5;
            if cost(middle) <= available {
                low = middle;
            } else {
                high = middle;
            }
        }
        for index in indices {
            let grant = &mut grants[index];
            grant.density = (grant.density * low).max(f32::MIN_POSITIVE);
            grant.pixels = extent(
                entries[index].request.size,
                f64::from(grant.density),
                maximum,
            );
            grant.budget_limited = grant.pixels.cmplt(grant.requested).any();
        }
    }
    let mut total = entries
        .iter()
        .zip(&grants)
        .fold(0_u64, |sum, (entry, grant)| {
            sum.saturating_add(bytes(*entry, grant.pixels))
        });
    if total > budget {
        let mut indices: Vec<_> = (0..entries.len()).collect();
        indices.sort_by_key(|index| {
            (
                entries[*index].priority,
                entries[*index].request.visible,
                std::cmp::Reverse(*index),
            )
        });
        for index in indices {
            if total <= budget {
                break;
            }
            total = total.saturating_sub(bytes(entries[index], grants[index].pixels));
            grants[index].pixels = UVec2::ZERO;
            grants[index].density = 0.0;
            grants[index].budget_limited = true;
        }
    }
    grants
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(size: Vec2, priority: u8, bytes_per_pixel: u32) -> Entry {
        Entry {
            request: Request {
                size,
                density: 1.0,
                previous: 0.0,
                visible: true,
                text: false,
                exact: true,
            },
            priority,
            bytes_per_pixel,
        }
    }

    #[test]
    fn the_focused_source_keeps_its_pixels_before_background_targets_and_depth_costs_count() {
        let entries = [
            entry(Vec2::splat(512.0), 3, 20),
            entry(Vec2::splat(2048.0), 0, 8),
        ];
        let budget = 6 * 1024 * 1024;
        let grants = allocate(&entries, budget, 8192);
        assert_eq!(grants[0].pixels, UVec2::splat(512));
        assert!(!grants[0].budget_limited);
        assert!(grants[1].budget_limited);
        assert!(
            entries
                .iter()
                .zip(grants)
                .map(|(entry, grant)| bytes(*entry, grant.pixels))
                .sum::<u64>()
                <= budget
        );
    }

    #[test]
    fn device_caps_preserve_aspect_and_do_not_silently_use_the_old_4096_policy() {
        let entries = [entry(Vec2::new(6000.0, 3000.0), 3, 4)];
        let full = allocate(&entries, 128 * 1024 * 1024, 8192);
        assert_eq!(full[0].pixels, UVec2::new(6000, 3000));
        assert!(!full[0].device_limited);
        let capped = allocate(&entries, 128 * 1024 * 1024, 1024);
        assert_eq!(capped[0].pixels, UVec2::new(1024, 512));
        assert!(capped[0].device_limited);
    }

    #[test]
    fn empty_and_insufficient_budgets_terminate_without_allocating_past_the_limit() {
        let entries = [
            entry(Vec2::splat(1000.0), 0, 8),
            entry(Vec2::splat(500.0), 3, 20),
        ];
        for budget in [0, 1, 20, 4096, 1_000_000] {
            let grants = allocate(&entries, budget, 4096);
            assert!(
                entries
                    .iter()
                    .zip(grants)
                    .map(|(entry, grant)| bytes(*entry, grant.pixels))
                    .sum::<u64>()
                    <= budget
            );
        }
        assert!(allocate(&[], 0, 0).is_empty());
    }
}
