use super::*;

pub(super) struct Face {
    pub time: Rect,
    pub font: f32,
    pub upcoming: Option<Rect>,
    pub memento: Option<Rect>,
}

impl Face {
    pub fn new(size: Vec2, font: f32, seconds: bool, rows: usize) -> Self {
        let radius = size.min_element() * 0.4;
        let height = radius * 1.55;
        let gap = 16.0;
        let row_height = 2.4 * 14.0 + 10.0;
        let font = (26.0 * (font / 16.0).clamp(0.8, 1.25))
            .min(radius * 1.5 / if seconds { 4.3 } else { 2.7 });
        let time_height = font * 1.2;
        let rows = rows.max(1) as f32;
        let minimum = row_height * rows.min(2.0) + 6.0 * (rows.min(2.0) - 1.0);
        let motto_height = motto_arc().0 + 17.0;
        let memento = height >= time_height + minimum + motto_height + gap * 2.0;
        let upcoming = height >= time_height + minimum + gap;
        let list_height = if upcoming {
            (row_height * rows + 6.0 * (rows - 1.0))
                .min(height - time_height - gap - if memento { motto_height + gap } else { 0.0 })
        } else {
            0.0
        };
        let total = time_height
            + if upcoming { list_height + gap } else { 0.0 }
            + if memento { motto_height + gap } else { 0.0 };
        let top = -total * 0.5;
        let time = Rect::from_center_size(
            Vec2::new(0.0, top + time_height * 0.5),
            Vec2::new(radius * 1.5, time_height),
        );
        let upcoming = upcoming.then(|| {
            Rect::from_center_size(
                Vec2::new(0.0, time.max.y + gap + list_height * 0.5),
                Vec2::new(radius * 1.45, list_height),
            )
        });
        let memento = memento.then(|| {
            Rect::from_center_size(
                Vec2::new(0.0, total * 0.5 - motto_height * 0.5),
                Vec2::new(motto_arc().0 * 2.0 + 6.0, motto_height),
            )
        });
        Self {
            time,
            font,
            upcoming,
            memento,
        }
    }

    pub fn clock(world: &World, owner: Entity, now: i64, size: Vec2, font: f32) -> Self {
        let settings = &world.get::<TimeSettings>(owner).unwrap().0;
        let rows = Self::rows(settings, &world.get::<View>(owner).unwrap().entries, now);
        Self::new(size, font, settings.aperture_ms <= 60_000, rows)
    }

    pub fn rows(settings: &Settings, entries: &[Entry], now: i64) -> usize {
        let until = now.saturating_add(settings.aperture_ms);
        entries
            .iter()
            .filter(|entry| {
                entry.category == model::Category::Timed
                    && entry.time.as_ref().is_some_and(|time| {
                        time.from_ms < until
                            && (time.from_ms >= now || time.until_ms.is_some_and(|end| end > now))
                    })
            })
            .count()
    }
}

pub(super) fn motto_arc() -> (f32, f32, f32) {
    static RADIUS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    let radius = *RADIUS.get_or_init(|| {
        let face = ttf_parser::Face::parse(
            include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf"),
            0,
        )
        .unwrap();
        let width = "memento mori"
            .chars()
            .filter_map(|ch| face.glyph_index(ch))
            .filter_map(|glyph| face.glyph_hor_advance(glyph))
            .map(f32::from)
            .sum::<f32>()
            * 11.0
            / f32::from(face.units_per_em())
            + 22.0;
        width / std::f32::consts::PI
    });
    (radius, radius, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_hides_the_memento_then_tasks_and_keeps_fixed_gaps_and_centering() {
        for font in [12.0, 16.0, 20.0] {
            for rows in [0, 1, 5, 30] {
                let mut had_memento = false;
                let mut had_tasks = false;
                for size in 64..=900 {
                    let face = Face::new(
                        Vec2::new(size as f32, size as f32 + 100.0),
                        font,
                        false,
                        rows,
                    );
                    assert!(!had_memento || face.memento.is_some());
                    assert!(!had_tasks || face.upcoming.is_some());
                    had_memento = face.memento.is_some();
                    had_tasks = face.upcoming.is_some();
                    assert!(face.memento.is_none() || face.upcoming.is_some());
                    let mut bottom = face.time.max.y;
                    for rect in [face.upcoming, face.memento].into_iter().flatten() {
                        assert!((rect.min.y - bottom - 16.0).abs() < 0.001);
                        assert_eq!(rect.center().x, 0.0);
                        bottom = rect.max.y;
                    }
                    assert!((face.time.min.y + bottom).abs() < 0.001);
                    assert!(bottom <= size as f32 * 0.4 * 0.775 + 0.001);
                }
            }
        }
        assert!(
            Face::new(Vec2::splat(420.0), 16.0, false, 5)
                .memento
                .is_some()
        );
        let medium = Face::new(Vec2::splat(260.0), 16.0, false, 5);
        assert!(medium.memento.is_none() && medium.upcoming.is_some());
        let small = Face::new(Vec2::splat(120.0), 16.0, false, 5);
        assert!(small.memento.is_none() && small.upcoming.is_none());
    }
}
