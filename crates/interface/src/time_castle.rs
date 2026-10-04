use nucleus::schedule::{MAX_DURATION_MS, TimeRange, Tz};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod layout;
pub use layout::{
    BandLevel, Label, LabelMetrics, Occurrence, labels, labels_with_metrics, lane_offset,
    occurrences,
};

pub const RECORD_SELECTED: &str = "Record selected";
pub const COVERAGE_PADDING_MS: i64 = 300_000;
pub const MAX_HORIZON_MS: i64 = MAX_DURATION_MS - 2 * COVERAGE_PADDING_MS;
pub const MAX_STACKS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Coiled,
    Straight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorMode {
    #[default]
    Moving,
    Fixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RimTick {
    pub at_ms: i64,
    pub major: bool,
    pub label: String,
}

pub fn countdown(milliseconds: i64) -> String {
    let seconds = milliseconds.max(0).saturating_add(999) / 1000;
    if seconds >= 3600 {
        format!(
            "{}h {:02}m {:02}s",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

pub fn summaries(entries: &[Entry], now: i64, until: i64) -> Vec<(usize, i64)> {
    let mut current = Vec::new();
    let mut future = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.category != Category::Timed {
            continue;
        }
        let Some(time) = &entry.time else { continue };
        if time.from_ms <= now && time.until_ms.is_some_and(|end| end > now) {
            current.push((index, time.from_ms, time.until_ms.unwrap() - now));
        } else if time.from_ms >= now && time.from_ms < until {
            future.push((index, time.from_ms, time.from_ms - now));
        }
    }
    let sort = |rows: &mut Vec<(usize, i64, i64)>| {
        rows.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| entries[a.0].id.cmp(&entries[b.0].id))
        })
    };
    sort(&mut current);
    sort(&mut future);
    current
        .into_iter()
        .chain(future)
        .map(|(index, _, remaining)| (index, remaining))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub aperture_ms: i64,
    pub horizon_ms: i64,
    pub timezone: String,
    pub area: Option<String>,
    pub mode: Mode,
    #[serde(default)]
    pub cursor: CursorMode,
    #[serde(default = "enabled")]
    pub floating_cards: bool,
    #[serde(default = "enabled")]
    pub card_physics: bool,
    #[serde(default)]
    pub sound: crate::sound::Settings,
}

fn enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            aperture_ms: 3_600_000,
            horizon_ms: 3_600_000,
            timezone: "UTC".into(),
            area: None,
            mode: Mode::Coiled,
            cursor: CursorMode::Moving,
            floating_cards: true,
            card_physics: true,
            sound: crate::sound::Settings::default(),
        }
    }
}

impl Settings {
    pub fn rim_ticks(&self, now: i64) -> Vec<RimTick> {
        let mut civil = self.clone();
        civil.cursor = CursorMode::Moving;
        let phase = civil.phase(now) / std::f64::consts::TAU;
        let base = now - (phase * self.aperture_ms as f64).round() as i64;
        let interval = (self.aperture_ms / 12).max(1);
        (0..60)
            .map(|index| {
                let mut at =
                    base + (self.aperture_ms as f64 * f64::from(index) / 60.0).round() as i64;
                if at < now {
                    at += self.aperture_ms;
                }
                let major = index % 5 == 0;
                let label = if !major {
                    String::new()
                } else if self.aperture_ms == 3_600_000 {
                    let full = self.tick_label(at, interval);
                    if full.len() == 5 {
                        full[3..].to_owned()
                    } else {
                        full
                    }
                } else if self.aperture_ms <= 60_000 {
                    chrono::DateTime::from_timestamp_millis(at)
                        .zip(self.timezone.parse::<Tz>().ok())
                        .map(|(time, zone)| {
                            time.with_timezone(&zone)
                                .format(if interval < 1000 { "%S%.3f" } else { "%S" })
                                .to_string()
                        })
                        .unwrap_or_default()
                } else {
                    self.tick_label(at, interval)
                };
                RimTick {
                    at_ms: at,
                    major,
                    label,
                }
            })
            .collect()
    }

    pub fn valid(&self) -> bool {
        if !self.sound.valid() {
            return false;
        }
        (1000..=MAX_HORIZON_MS).contains(&self.aperture_ms)
            && (self.aperture_ms..=MAX_HORIZON_MS).contains(&self.horizon_ms)
            && self.timezone.parse::<Tz>().is_ok()
            && self
                .area
                .as_ref()
                .is_none_or(|id| !id.is_empty() && id.len() <= 128)
    }

    pub fn set_aperture(&mut self, value: i64) {
        self.aperture_ms = value;
        self.horizon_ms = value;
    }

    pub fn set_horizon(&mut self, value: i64) {
        self.horizon_ms = value;
        self.aperture_ms = self.aperture_ms.min(value);
    }

    pub fn aperture_label(&self) -> String {
        let (value, unit) = if self.aperture_ms % 3_600_000 == 0 {
            (self.aperture_ms as f64 / 3_600_000.0, "hour")
        } else if self.aperture_ms % 60_000 == 0 {
            (self.aperture_ms as f64 / 60_000.0, "minute")
        } else {
            (self.aperture_ms as f64 / 1000.0, "second")
        };
        format!("Next {value} {unit}{}", if value == 1.0 { "" } else { "s" })
    }

    pub fn window(&self, now_ms: i64) -> Option<nucleus::projection::Window> {
        if !self.valid() {
            return None;
        }
        let from_ms = now_ms.div_euclid(COVERAGE_PADDING_MS) * COVERAGE_PADDING_MS;
        Some(nucleus::projection::Window {
            from_ms,
            until_ms: now_ms
                .checked_add(self.horizon_ms)?
                .checked_add(COVERAGE_PADDING_MS)?,
            timezone: self.timezone.clone(),
        })
    }

    pub fn phase(&self, now_ms: i64) -> f64 {
        if self.cursor == CursorMode::Fixed {
            return 0.0;
        }
        let Some(time) = chrono::DateTime::from_timestamp_millis(now_ms) else {
            return 0.0;
        };
        let Ok(zone) = self.timezone.parse::<Tz>() else {
            return 0.0;
        };
        let time = time.with_timezone(&zone);
        let local_ms = time.naive_local().and_utc().timestamp_millis();
        std::f64::consts::TAU * local_ms.rem_euclid(self.aperture_ms) as f64
            / self.aperture_ms as f64
    }

    pub fn position(&self, at_ms: i64, now_ms: i64, size: [f32; 2], unwind: f32) -> [f32; 3] {
        let radius = f64::from(size[0].min(size[1]).max(1.0)) * 0.40;
        let elapsed = at_ms.saturating_sub(now_ms).max(0) as f64;
        let turns = elapsed / self.aperture_ms as f64;
        let angle = self.phase(now_ms) + std::f64::consts::TAU * turns;
        let pitch = (radius * 0.45)
            .min(100_000.0 / (self.horizon_ms as f64 / self.aperture_ms as f64).max(1.0));
        let coiled = [radius * angle.sin(), -pitch * turns, -radius * angle.cos()];
        let straight = [
            f64::from(size[0]) * (elapsed / self.horizon_ms as f64 - 0.5) * 0.9,
            0.0,
            0.0,
        ];
        let unwind = f64::from(unwind.clamp(0.0, 1.0));
        std::array::from_fn(|index| {
            (coiled[index] * (1.0 - unwind) + straight[index] * unwind) as f32
        })
    }

    pub fn transverse(&self, at_ms: i64, now_ms: i64, unwind: f32) -> [f32; 3] {
        let angle = self.phase(now_ms)
            + std::f64::consts::TAU * at_ms.saturating_sub(now_ms).max(0) as f64
                / self.aperture_ms as f64;
        let value = [
            (angle.sin() as f32) * (1.0 - unwind),
            0.0,
            (-angle.cos() as f32) * (1.0 - unwind) + unwind,
        ];
        let length = value[0].hypot(value[2]);
        if length < 0.001 {
            return [0.0, 0.0, 1.0];
        }
        [value[0] / length, 0.0, value[2] / length]
    }

    pub fn label(&self, at_ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(at_ms)
            .zip(self.timezone.parse::<Tz>().ok())
            .map(|(time, zone)| {
                time.with_timezone(&zone)
                    .format("%m-%d %H:%M:%S %:z")
                    .to_string()
            })
            .unwrap_or_else(|| "Invalid time".into())
    }

    pub fn tick_label(&self, at_ms: i64, interval_ms: i64) -> String {
        use chrono::TimeZone;
        let Some((at, zone)) =
            chrono::DateTime::from_timestamp_millis(at_ms).zip(self.timezone.parse::<Tz>().ok())
        else {
            return "Invalid time".into();
        };
        let local = at.with_timezone(&zone);
        let format = if interval_ms < 60_000 {
            "%H:%M:%S"
        } else if self.aperture_ms >= 86_400_000 {
            "%m-%d %H:%M"
        } else {
            "%H:%M"
        };
        let mut label = local.format(format).to_string();
        if matches!(
            zone.from_local_datetime(&local.naive_local()),
            chrono::LocalResult::Ambiguous(_, _)
        ) {
            label.push_str(&local.format(" %:z").to_string());
        }
        label
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Timed,
    AllDay,
    Overdue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "uid")]
    pub id: String,
    pub record_uid: String,
    pub head: String,
    pub quantity: String,
    pub category: Category,
    pub time: Option<TimeRange>,
    pub origin: Value,
    pub preview: bool,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

impl Entry {
    pub fn sound_cue(
        &self,
        scope: u64,
        settings: &crate::sound::Settings,
    ) -> Option<crate::sound::Cue> {
        if self.category == Category::AllDay {
            return None;
        }
        Some(crate::sound::Cue {
            scope,
            key: self.cue_key("")?,
            at_ms: self.time.as_ref()?.from_ms,
            title: self
                .head
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Untitled event")
                .into(),
            projected: self.preview || self.origin["kind"] == "projection",
            settings: settings.clone(),
        })
    }

    pub fn cue_key(&self, source: &str) -> Option<String> {
        let time = self.time.as_ref()?;
        let linked = self.origin.get("occurrence").and_then(|value| {
            serde_json::from_value::<nucleus::projection::OccurrenceLink>(value.clone()).ok()
        });
        let cause = self.origin.get("cause").and_then(|value| {
            serde_json::from_value::<nucleus::simulation::Cause>(value.clone()).ok()
        });
        let identity = if let Some(link) = linked {
            serde_json::json!([
                link.record.as_str(),
                link.occurrence.rule_uid,
                link.occurrence.revision,
                link.occurrence.event_id
            ])
        } else if let Some(nucleus::simulation::Cause::Rule { occurrence, .. }) = cause {
            serde_json::json!([
                self.record_uid,
                occurrence.rule_uid,
                occurrence.revision,
                occurrence.event_id
            ])
        } else {
            serde_json::json!([self.record_uid, time.from_ms])
        };
        Some(serde_json::json!([source, identity]).to_string())
    }

    pub fn time_label(&self, settings: &Settings, now: i64) -> String {
        let Some(time) = &self.time else {
            return [self.start_date.as_deref(), self.due_date.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" – ");
        };
        let Ok(zone) = settings.timezone.parse::<Tz>() else {
            return "Invalid timezone".into();
        };
        let date = |at| {
            chrono::DateTime::from_timestamp_millis(at)
                .map(|at| at.with_timezone(&zone).date_naive())
        };
        let interval = if settings.aperture_ms <= 60_000 {
            1000
        } else {
            60_000
        };
        let label = |at, compare| {
            let prefix = date(at)
                .filter(|date| Some(*date) != compare)
                .map(|date| date.format("%m-%d ").to_string())
                .unwrap_or_default();
            format!("{prefix}{}", settings.tick_label(at, interval))
        };
        let start = label(time.from_ms, date(now));
        time.until_ms.map_or_else(
            || start.clone(),
            |end| format!("{start} – {}", label(end, date(time.from_ms))),
        )
    }

    pub fn category_at(&self, now_ms: i64, timezone: &str) -> Category {
        if self.origin["kind"] == "manual"
            && nucleus::DecimalValue::parse_inferred(&self.quantity)
                .is_ok_and(|quantity| !quantity.is_zero())
            && let Some(due) = self.due_date.as_deref()
            && let Ok(due) = nucleus::schedule::TimeValue::parse(due)
        {
            let overdue = match due {
                nucleus::schedule::TimeValue::Instant(time) => time.timestamp_millis() < now_ms,
                nucleus::schedule::TimeValue::Date(date) => {
                    chrono::DateTime::from_timestamp_millis(now_ms)
                        .zip(timezone.parse::<Tz>().ok())
                        .is_some_and(|(now, zone)| date < now.with_timezone(&zone).date_naive())
                }
            };
            if overdue {
                return Category::Overdue;
            }
        }
        self.category
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    pub record_uid: String,
    pub source: crate::records::Source,
    pub entry: Entry,
    pub timezone: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub ids: Vec<String>,
    pub time: TimeRange,
}

pub fn stacks(
    entries: &[Entry],
    now_ms: i64,
    until_ms: i64,
    pixels: f32,
    timezone: &str,
) -> Vec<Stack> {
    let collision_ms = ((until_ms - now_ms) as f64 * 8.0 / f64::from(pixels.max(8.0)))
        .ceil()
        .max(1.0) as i64;
    let mut ordered: Vec<_> = entries
        .iter()
        .filter(|entry| entry.category_at(now_ms, timezone) == Category::Timed)
        .filter_map(|entry| Some((entry, entry.time.as_ref()?.clipped(now_ms, until_ms)?)))
        .collect();
    ordered.sort_by(|(a, a_time), (b, b_time)| {
        a_time
            .from_ms
            .cmp(&b_time.from_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut output: Vec<Stack> = Vec::new();
    for (entry, time) in ordered {
        if let Some(previous) = output.last_mut()
            && time.from_ms
                <= previous
                    .time
                    .until_ms
                    .unwrap_or(previous.time.from_ms)
                    .saturating_add(collision_ms)
        {
            previous.ids.push(entry.id.clone());
            if let Some(end) = time.until_ms {
                previous.time.until_ms = Some(
                    previous
                        .time
                        .until_ms
                        .unwrap_or(previous.time.from_ms)
                        .max(end),
                );
            }
        } else {
            output.push(Stack {
                ids: vec![entry.id.clone()],
                time,
            });
        }
    }
    if output.len() > MAX_STACKS {
        let batch = output.len().div_ceil(MAX_STACKS);
        output = output
            .chunks(batch)
            .map(|group| Stack {
                ids: group.iter().flat_map(|stack| stack.ids.clone()).collect(),
                time: TimeRange {
                    from_ms: group[0].time.from_ms,
                    until_ms: group.iter().filter_map(|stack| stack.time.until_ms).max(),
                },
            })
            .collect();
    }
    output
}

pub fn tick_interval(duration_ms: i64, pixels: f32, minimum_pixels: f32) -> i64 {
    let target =
        duration_ms as f64 * f64::from(minimum_pixels.max(1.0)) / f64::from(pixels.max(1.0));
    [
        1000,
        2000,
        5000,
        10_000,
        15_000,
        30_000,
        60_000,
        120_000,
        300_000,
        600_000,
        900_000,
        1_800_000,
        3_600_000,
        7_200_000,
        10_800_000,
        21_600_000,
        43_200_000,
        86_400_000,
        172_800_000,
        604_800_000,
        2_592_000_000,
        MAX_DURATION_MS,
    ]
    .into_iter()
    .find(|step| *step as f64 >= target)
    .unwrap_or(MAX_DURATION_MS)
}
