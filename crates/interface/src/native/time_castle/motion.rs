use super::*;
use lince_interface::motion::Spring;
use std::time::Instant;

#[derive(Clone)]
pub(super) struct Band {
    pub entry: Entry,
    pub occurrence: model::Occurrence,
    previous: model::Occurrence,
    blend: Spring<1>,
    fall: Spring<1>,
    pub retiring: bool,
    retired_at: Option<Instant>,
}

impl Band {
    pub fn settled(entry: Entry, occurrence: model::Occurrence) -> Self {
        Self {
            entry,
            previous: occurrence.clone(),
            occurrence,
            blend: Spring::new([1.0]),
            fall: Spring::new([0.0]),
            retiring: false,
            retired_at: None,
        }
    }
    pub fn opacity(&self) -> f32 {
        (1.0 - self.fall.position[0].max(0.0) / 72.0).clamp(0.0, 1.0)
            * if self.occurrence.historical { 0.7 } else { 1.0 }
    }

    pub fn offset(&self, at: i64, settings: &Settings, size: Vec2, width: f32) -> f32 {
        let radius = size.min_element() * 0.4;
        let previous = self
            .previous
            .offset_at(at, settings.aperture_ms, radius, width);
        let next = self
            .occurrence
            .offset_at(at, settings.aperture_ms, radius, width);
        previous + (next - previous) * self.blend.position[0] + self.fall.position[0]
    }

    #[cfg(test)]
    pub fn anchor(
        &self,
        settings: &Settings,
        now: i64,
        size: Vec2,
        width: f32,
        unwind: f32,
    ) -> [f32; 3] {
        let curve = settings.geometry(now, size.to_array(), unwind);
        self.anchor_in(&curve, settings, now, size, width)
    }

    pub fn anchor_in(
        &self,
        curve: &model::Geometry,
        settings: &Settings,
        now: i64,
        size: Vec2,
        width: f32,
    ) -> [f32; 3] {
        let time = self.visible_time(now);
        let at = time.from_ms + (time.until_ms.unwrap_or(time.from_ms) - time.from_ms) / 2;
        let mut point = Vec3::from_array(curve.position(at));
        point += Vec3::from_array(curve.transverse(at)) * self.offset(at, settings, size, width);
        point.to_array()
    }

    fn visible_time(&self, now: i64) -> nucleus::schedule::TimeRange {
        let time = if self.entry.follows_cursor() {
            let mut time = self.entry.effective_time(now).unwrap();
            time.until_ms = self.occurrence.time.until_ms.map(|end| {
                time.from_ms
                    .saturating_add(end - self.occurrence.time.from_ms)
            });
            time
        } else {
            self.occurrence.time.clone()
        };
        time.clipped(now, i64::MAX)
            .unwrap_or(nucleus::schedule::TimeRange {
                from_ms: now,
                until_ms: None,
            })
    }
}

#[derive(Component)]
pub(super) struct Motion {
    pub bands: HashMap<String, Band>,
    pub seconds: f32,
    pub active: bool,
    pub tick: i64,
    last: Instant,
}

#[derive(Resource)]
struct MotionQuery(
    bevy::ecs::query::QueryState<(&'static View, &'static TimeSettings, &'static mut Motion)>,
);

impl FromWorld for MotionQuery {
    fn from_world(world: &mut World) -> Self {
        Self(world.query())
    }
}

pub(super) fn update(
    world: &mut World,
    owner: Entity,
    now: i64,
    duration: i64,
    changed: bool,
) -> bool {
    if world.get::<Motion>(owner).is_none() {
        world.entity_mut(owner).insert(Motion {
            bands: HashMap::new(),
            seconds: 1.0 / 60.0,
            active: false,
            tick: i64::MIN,
            last: Instant::now(),
        });
    }
    world.init_resource::<MotionQuery>();
    world.resource_scope(|world, mut query: Mut<MotionQuery>| {
        let (view, settings, mut motion) = query.0.get_mut(world, owner).unwrap();
        advance(view, &settings.0, &mut motion, now, duration, changed)
    })
}

fn advance(
    view: &View,
    settings: &Settings,
    motion: &mut Motion,
    now: i64,
    duration: i64,
    changed: bool,
) -> bool {
    let _span = nucleus::performance::Span::new("time-motion-update");
    let tick = now / 1000;
    let changed = changed || motion.tick != tick;
    let entries = &view.entries;
    let occurrences =
        changed.then(|| model::clock_occurrences(settings, entries, now, now + duration));
    let physics = settings.card_physics;
    motion.seconds = motion.last.elapsed().as_secs_f32();
    motion.last = Instant::now();
    motion.tick = tick;
    if let Some(occurrences) = occurrences {
        let ids: std::collections::HashSet<_> = occurrences
            .iter()
            .map(|occurrence| entries[occurrence.index].id.as_str())
            .collect();
        for band in motion.bands.values_mut() {
            if !ids.contains(band.entry.id.as_str()) && !band.retiring {
                band.retiring = true;
                band.retired_at = Some(Instant::now());
            }
        }
        for occurrence in occurrences {
            let entry = &entries[occurrence.index];
            if let Some(band) = motion.bands.get_mut(&entry.id) {
                let same_profile = band.occurrence.profile.len() == occurrence.profile.len()
                    && band
                        .occurrence
                        .profile
                        .iter()
                        .zip(&occurrence.profile)
                        .enumerate()
                        .all(|(index, (a, b))| {
                            a.lane == b.lane
                                && (index == 0
                                    || if entry.follows_cursor() {
                                        a.from_ms - band.occurrence.time.from_ms
                                            == b.from_ms - occurrence.time.from_ms
                                            && a.next_ms - band.occurrence.time.from_ms
                                                == b.next_ms - occurrence.time.from_ms
                                    } else {
                                        a.from_ms == b.from_ms && a.next_ms == b.next_ms
                                    })
                        });
                if !same_profile || band.entry.time != entry.time {
                    band.previous = band.occurrence.clone();
                    band.blend.position[0] = 0.0;
                }
                band.occurrence = occurrence;
                if band.entry != *entry {
                    band.entry.clone_from(entry);
                    crate::host::performance::count(
                        crate::host::performance::Count::TimeMotionEntriesCopied,
                    );
                }
                band.retiring = false;
                band.retired_at = None;
            } else {
                crate::host::performance::count(
                    crate::host::performance::Count::TimeMotionEntriesCopied,
                );
                motion.bands.insert(
                    entry.id.clone(),
                    Band {
                        entry: entry.clone(),
                        previous: occurrence.clone(),
                        occurrence,
                        blend: Spring::new([1.0]),
                        fall: Spring::new([72.0]),
                        retiring: false,
                        retired_at: None,
                    },
                );
            }
        }
    }
    let seconds = motion.seconds;
    let mut active = false;
    let previous_count = motion.bands.len();
    motion.bands.retain(|_, band| {
        if band
            .retired_at
            .is_some_and(|at| at.elapsed().as_millis() >= 700)
        {
            return false;
        }
        let target = [if band.retiring { 72.0 } else { 0.0 }];
        if physics {
            if band.blend.position != [1.0] || band.blend.velocity != [0.0] {
                active |= band.blend.advance([1.0], seconds);
            }
            if band.fall.position != target || band.fall.velocity != [0.0] {
                active |= band.fall.advance(target, seconds);
            }
        } else {
            band.blend = Spring::new([1.0]);
            band.fall = Spring::new(target);
        }
        true
    });
    active |= previous_count != motion.bands.len();
    motion.active = active;
    active
}

#[cfg(test)]
pub(super) fn points(
    band: &Band,
    settings: &Settings,
    now: i64,
    size: Vec2,
    unwind: f32,
    width: f32,
) -> Vec<Vec3> {
    let curve = settings.geometry(now, size.to_array(), unwind);
    points_in(band, &curve, settings, now, size, width)
}

pub(super) fn points_in(
    band: &Band,
    curve: &model::Geometry,
    settings: &Settings,
    now: i64,
    size: Vec2,
    width: f32,
) -> Vec<Vec3> {
    let time = band.visible_time(now);
    let mut points: Vec<_> = curve.samples(&time, 512).map(Vec3::from_array).collect();
    let count = points.len().saturating_sub(1).max(1) as f64;
    for (index, point) in points.iter_mut().enumerate() {
        let at = time.from_ms
            + ((time.until_ms.unwrap_or(time.from_ms) - time.from_ms) as f64 * index as f64 / count)
                .round() as i64;
        *point += Vec3::from_array(curve.transverse(at)) * band.offset(at, settings, size, width);
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            id: "range".into(),
            record_uid: "record".into(),
            head: "Ongoing range".into(),
            quantity: "-1".into(),
            category: model::Category::Timed,
            time: Some(nucleus::schedule::TimeRange {
                from_ms: 1000,
                until_ms: Some(60_000),
            }),
            origin: serde_json::json!({"kind":"manual"}),
            preview: false,
            start_date: None,
            due_date: None,
            estimate_min: None,
        }
    }

    #[test]
    fn incoming_bands_settle_then_retire_without_past_geometry() {
        let mut world = World::new();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(&mut world, owner);
        world.get_mut::<View>(owner).unwrap().entries = vec![entry()];
        update(&mut world, owner, 2000, 3_600_000, true);
        assert!(
            world.get::<Motion>(owner).unwrap().bands["range"]
                .fall
                .position[0]
                > 60.0
        );
        for _ in 0..120 {
            world.get_mut::<Motion>(owner).unwrap().last =
                Instant::now() - std::time::Duration::from_secs_f32(1.0 / 60.0);
            update(&mut world, owner, 2000, 3_600_000, false);
        }
        let motion = world.get::<Motion>(owner).unwrap();
        assert!(!motion.active);
        assert_eq!(motion.bands["range"].opacity(), 1.0);
        assert_eq!(motion.bands["range"].occurrence.time.from_ms, 2000);
        world.get_mut::<View>(owner).unwrap().entries.clear();
        update(&mut world, owner, 3000, 3_600_000, true);
        let band = &world.get::<Motion>(owner).unwrap().bands["range"];
        assert!(band.retiring);
        let geometry = points(
            band,
            &Settings::default(),
            3000,
            Vec2::splat(420.0),
            0.0,
            4.0,
        );
        assert!(geometry.len() > 1);
        let expired = points(
            band,
            &Settings::default(),
            61_000,
            Vec2::splat(420.0),
            0.0,
            4.0,
        );
        assert_eq!(expired.len(), 1);
        world
            .get_mut::<Motion>(owner)
            .unwrap()
            .bands
            .get_mut("range")
            .unwrap()
            .retired_at = Some(Instant::now() - std::time::Duration::from_millis(701));
        update(&mut world, owner, 3000, 3_600_000, false);
        assert!(world.get::<Motion>(owner).unwrap().bands.is_empty());
    }

    #[test]
    fn past_work_stays_attached_to_the_current_cursor() {
        for cursor in [CursorMode::Moving, CursorMode::Fixed] {
            let settings = Settings {
                cursor,
                ..default()
            };
            let entry = entry();
            let occurrence = model::clock_occurrences(
                &settings,
                std::slice::from_ref(&entry),
                61_000,
                3_600_000,
            )
            .remove(0);
            assert!(occurrence.historical);
            let band = Band::settled(entry, occurrence);
            for now in [61_000, 63_000] {
                let size = Vec2::splat(420.0);
                let expected = Vec3::from_array(settings.position(now, now, size.to_array(), 0.0))
                    + Vec3::from_array(settings.transverse(now, now, 0.0)) * 6.0;
                let anchor = Vec3::from_array(band.anchor(&settings, now, size, 4.0, 0.0));
                assert!((anchor - expected).length() < 0.001);
            }
        }
    }

    #[test]
    fn leaders_use_visible_band_midpoints_and_outside_rim_offsets() {
        let entry = entry();
        let occurrence =
            model::occurrences(std::slice::from_ref(&entry), 2000, 3_600_000, "UTC").remove(0);
        let band = Band::settled(entry, occurrence);
        let settings = Settings::default();
        let size = Vec2::splat(420.0);
        let anchor = Vec3::from_array(band.anchor(&settings, 2000, size, 4.0, 0.0)).with_y(0.0);
        let midpoint =
            Vec3::from_array(settings.position(31_000, 2000, size.to_array(), 0.0)).with_y(0.0);
        assert!((anchor.normalize() - midpoint.normalize()).length() < 0.0001);
        assert!(anchor.length() > size.x * 0.4);
    }
    #[test]
    fn parallel_cursor_ranges_do_not_restart_band_motion_each_second() {
        let mut world = World::new();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(&mut world, owner);
        let mut short = entry();
        short.origin = serde_json::json!({"kind":"need","allocation":{"cursor_offset_ms":0,"duration_ms":60000}});
        let mut long = short.clone();
        long.id = "long".into();
        long.record_uid = "long-record".into();
        long.origin["allocation"]["duration_ms"] = serde_json::json!(120000);
        world.get_mut::<View>(owner).unwrap().entries = vec![short, long];
        update(&mut world, owner, 1000, 3_600_000, true);
        for band in world.get_mut::<Motion>(owner).unwrap().bands.values_mut() {
            band.fall = Spring::new([0.0]);
            band.blend = Spring::new([1.0]);
        }
        for now in [2000, 60000, 120000, 3_600_000] {
            assert!(!update(&mut world, owner, now, 3_600_000, false));
            let motion = world.get::<Motion>(owner).unwrap();
            assert_eq!(motion.bands.len(), 2);
            for band in motion.bands.values() {
                assert_eq!(band.blend.position, [1.0]);
                assert_eq!(band.visible_time(now).from_ms, now);
            }
        }
    }
}
