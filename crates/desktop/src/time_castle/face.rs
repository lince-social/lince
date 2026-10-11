use super::*;

pub(super) struct Face {
    pub time: Rect,
    pub font: f32,
    pub upcoming: Option<Rect>,
    pub memento: Option<Rect>,
}

pub(super) fn text_width(text: &str, size: f32) -> f32 {
    static FONT: std::sync::OnceLock<ttf_parser::Face<'static>> = std::sync::OnceLock::new();
    let font = FONT.get_or_init(|| {
        ttf_parser::Face::parse(
            include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf"),
            0,
        )
        .unwrap()
    });
    text.chars()
        .map(|ch| {
            font.glyph_index(ch)
                .and_then(|glyph| font.glyph_hor_advance(glyph))
                .map(f32::from)
                .unwrap_or(f32::from(font.units_per_em()))
        })
        .sum::<f32>()
        * size
        / f32::from(font.units_per_em())
}

pub(super) struct Ring {
    pub outer: f32,
    pub inner: f32,
}

impl Ring {
    pub fn new(settings: &Settings, now: i64, size: Vec2) -> Self {
        let outer = size.min_element() * 0.4;
        let width = if settings.aperture_ms == 3_600_000 || settings.aperture_ms <= 60_000 {
            text_width(
                if settings.aperture_ms < 12_000 {
                    "00.000"
                } else {
                    "00"
                },
                10.0,
            ) + 16.0
        } else {
            [now, now.saturating_add(settings.aperture_ms)]
                .into_iter()
                .map(|at| text_width(&settings.tick_label(at, settings.aperture_ms / 12), 10.0))
                .fold(0.0, f32::max)
                + 16.0
        };
        Self {
            outer,
            inner: outer - width.min(outer * 0.45),
        }
    }

    pub fn thickness(&self) -> f32 {
        self.outer - self.inner
    }
}

impl Face {
    #[cfg(test)]
    pub fn new(size: Vec2, font: f32, seconds: bool, rows: usize) -> Self {
        Self::inside(
            Ring::new(&Settings::default(), 0, size).inner,
            font,
            seconds,
            rows,
        )
    }

    fn inside(radius: f32, font: f32, seconds: bool, rows: usize) -> Self {
        let height = radius * 1.4;
        let gap = 16.0;
        let row_height = 2.4 * 14.0 + 10.0;
        let font = (26.0 * (font / 16.0).clamp(0.8, 1.25))
            .min(radius * 1.5 / if seconds { 4.3 } else { 2.7 });
        let time_height = font * 1.2;
        let rows = rows as f32;
        let minimum = row_height * rows.min(1.0) + 6.0 * (rows.min(1.0) - 1.0);
        let motto_height = motto_arc().0 + 17.0;
        let memento = height
            >= time_height + motto_height + gap + if rows > 0.0 { minimum + gap } else { 0.0 };
        let upcoming = rows > 0.0 && height >= time_height + minimum + gap;
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
            Vec2::new(
                text_width(if seconds { "00:00:00" } else { "00:00" }, font),
                time_height,
            ),
        );
        let upcoming = upcoming.then(|| {
            Rect::from_center_size(
                Vec2::new(0.0, time.max.y + gap + list_height * 0.5),
                Vec2::new(radius * 1.4, list_height),
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
        Self::content(
            settings,
            &world.get::<View>(owner).unwrap().entries,
            now,
            size,
            font,
        )
    }

    pub fn content(
        settings: &Settings,
        entries: &[Entry],
        now: i64,
        size: Vec2,
        font: f32,
    ) -> Self {
        Self::inside(
            Ring::new(settings, now, size).inner,
            font,
            settings.aperture_ms <= 60_000,
            Self::rows(settings, entries, now),
        )
    }

    pub fn rows(settings: &Settings, entries: &[Entry], now: i64) -> usize {
        let until = now.saturating_add(settings.aperture_ms);
        entries
            .iter()
            .filter(|entry| {
                entry.category == model::Category::Timed
                    && (settings.past_tasks || entry.origin["allocation"].is_null())
                    && entry.effective_time(now).is_some_and(|time| {
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
    fn labels_fit_the_ring_and_long_dates_make_it_wider() {
        let short = Settings::default();
        let mut long = short.clone();
        long.aperture_ms = 172_800_000;
        let short = Ring::new(&short, 1_780_000_000_000, Vec2::splat(800.0));
        let long = Ring::new(&long, 1_780_000_000_000, Vec2::splat(800.0));
        assert!(long.thickness() > short.thickness());
        assert!(
            Face::new(Vec2::splat(420.0), 16.0, false, 0)
                .upcoming
                .is_none()
        );
        assert!(
            Face::new(Vec2::splat(420.0), 16.0, false, 0)
                .memento
                .is_some()
        );
    }

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
                    assert!(rows == 0 || face.memento.is_none() || face.upcoming.is_some());
                    let mut bottom = face.time.max.y;
                    for rect in [face.upcoming, face.memento].into_iter().flatten() {
                        assert!((rect.min.y - bottom - 16.0).abs() < 0.001);
                        assert_eq!(rect.center().x, 0.0);
                        bottom = rect.max.y;
                    }
                    let inner = Ring::new(&Settings::default(), 0, Vec2::splat(size as f32)).inner;
                    for rect in [Some(face.time), face.upcoming, face.memento]
                        .into_iter()
                        .flatten()
                    {
                        for corner in [
                            rect.min,
                            rect.max,
                            Vec2::new(rect.min.x, rect.max.y),
                            Vec2::new(rect.max.x, rect.min.y),
                        ] {
                            assert!(corner.length() <= inner + 0.001);
                        }
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
