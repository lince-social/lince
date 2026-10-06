use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug, PartialEq)]
pub struct BandLevel {
    pub from_ms: i64,
    pub next_ms: i64,
    pub lane: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub index: usize,
    pub time: TimeRange,
    pub lane: usize,
    pub profile: Vec<BandLevel>,
    pub historical: bool,
}

impl Occurrence {
    pub fn anchor_ms(&self) -> i64 {
        self.time.from_ms
            + (self.time.until_ms.unwrap_or(self.time.from_ms) - self.time.from_ms) / 2
    }

    pub fn level_at(&self, at: i64, smooth_ms: i64) -> f32 {
        let index = self
            .profile
            .partition_point(|level| level.from_ms <= at)
            .saturating_sub(1);
        let Some(level) = self.profile.get(index) else {
            return self.lane as f32;
        };
        if index == 0 {
            return level.lane as f32;
        }
        let duration = smooth_ms
            .min((level.next_ms - level.from_ms).max(1) / 2)
            .max(1);
        let fraction = ((at - level.from_ms) as f32 / duration as f32).clamp(0.0, 1.0);
        let fraction = fraction * fraction * (3.0 - 2.0 * fraction);
        self.profile[index - 1].lane as f32 * (1.0 - fraction) + level.lane as f32 * fraction
    }

    pub fn offset_at(&self, at: i64, aperture_ms: i64, radius: f32, width: f32) -> f32 {
        let smooth = (aperture_ms as f64 * 8.0
            / (f64::from(radius.max(1.0)) * std::f64::consts::TAU))
            .round() as i64;
        4.0 + width * 0.5 + self.level_at(at, smooth) * (width + 3.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub id: String,
    pub occurrence: Occurrence,
    pub anchor: [f32; 3],
    pub rect: [f32; 4],
    pub title: String,
    pub time: String,
}

pub fn occurrences(entries: &[Entry], now: i64, until: i64, timezone: &str) -> Vec<Occurrence> {
    let mut ordered: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            (entry.category_at(now, timezone) == Category::Timed).then(|| {
                let mut time = entry.time.clone()?;
                if time.until_ms == Some(time.from_ms) {
                    time.until_ms = None;
                }
                time.clipped(now, until).map(|time| (index, time))
            })?
        })
        .collect();
    ordered.sort_by(|(a, _), (b, _)| {
        let a_time = entries[*a].time.as_ref().unwrap();
        let b_time = entries[*b].time.as_ref().unwrap();
        a_time
            .from_ms
            .cmp(&b_time.from_ms)
            .then_with(|| {
                (b_time.until_ms.unwrap_or(b_time.from_ms) - b_time.from_ms)
                    .cmp(&(a_time.until_ms.unwrap_or(a_time.from_ms) - a_time.from_ms))
            })
            .then_with(|| entries[*a].id.cmp(&entries[*b].id))
    });
    let mut result: Vec<_> = ordered
        .into_iter()
        .map(|(index, time)| Occurrence {
            index,
            time,
            lane: 0,
            profile: Vec::new(),
            historical: false,
        })
        .collect();
    let mut boundaries = BTreeMap::<i64, (Vec<usize>, Vec<usize>, Vec<usize>)>::new();
    for (index, occurrence) in result.iter().enumerate() {
        if let Some(end) = occurrence.time.until_ms {
            boundaries
                .entry(occurrence.time.from_ms)
                .or_default()
                .0
                .push(index);
            boundaries.entry(end).or_default().1.push(index);
        } else {
            boundaries
                .entry(occurrence.time.from_ms)
                .or_default()
                .2
                .push(index);
        }
    }
    let mut active = BTreeSet::new();
    let times: Vec<_> = boundaries.keys().copied().collect();
    for (boundary, (at, (starts, ends, points))) in boundaries.into_iter().enumerate() {
        let next_ms = times.get(boundary + 1).copied().unwrap_or(until);
        for index in ends {
            active.remove(&index);
        }
        active.extend(starts);
        for (lane, index) in active.iter().copied().enumerate() {
            let occurrence = &mut result[index];
            if occurrence
                .profile
                .last()
                .is_none_or(|level| level.lane != lane)
            {
                occurrence.profile.push(BandLevel {
                    from_ms: at,
                    next_ms,
                    lane,
                });
            }
            occurrence.lane = occurrence.lane.max(lane);
        }
        for (lane, index) in points.into_iter().enumerate() {
            let occurrence = &mut result[index];
            occurrence.lane = active.len() + lane;
            occurrence.profile.push(BandLevel {
                from_ms: at,
                next_ms,
                lane: occurrence.lane,
            });
        }
    }
    result
}

pub fn clock_occurrences(
    settings: &Settings,
    entries: &[Entry],
    now: i64,
    until: i64,
) -> Vec<Occurrence> {
    let mut result = occurrences(entries, now, until, &settings.timezone);
    if settings.past_tasks {
        let mut outstanding = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            if entry.outstanding_at(now, &settings.timezone)
                && outstanding.insert(entry.record_uid.clone())
            {
                result.push(Occurrence {
                    index,
                    time: TimeRange {
                        from_ms: now,
                        until_ms: None,
                    },
                    lane: 0,
                    profile: Vec::new(),
                    historical: true,
                });
            }
        }
        let from = settings.history_from(now);
        for (index, entry) in entries.iter().enumerate() {
            if entry.preview
                || outstanding.contains(&entry.record_uid)
                || entry.origin["kind"] != "manual"
                || entry.category_at(now, &settings.timezone) != Category::Timed
            {
                continue;
            }
            let Some(time) = &entry.time else { continue };
            let end = time.until_ms.unwrap_or(time.from_ms);
            if end < from || end > now || time.from_ms >= now {
                continue;
            }
            result.push(Occurrence {
                index,
                time: TimeRange {
                    from_ms: now,
                    until_ms: None,
                },
                lane: 0,
                profile: Vec::new(),
                historical: true,
            });
        }
    }
    result
}

pub fn lane_offset(lane: usize, _count: usize, _radius: f32) -> f32 {
    6.0 + lane as f32 * 7.0
}

fn overlap(a: [f32; 4], b: [f32; 4], gap: f32) -> bool {
    a[0] < b[0] + b[2] + gap
        && a[0] + a[2] + gap > b[0]
        && a[1] < b[1] + b[3] + gap
        && a[1] + a[3] + gap > b[1]
}

fn wrap(title: &str, width: f32, advance: &dyn Fn(char) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut occupied = 0.0;
    for word in title.split_whitespace() {
        let length = word.chars().map(advance).sum::<f32>();
        if !line.is_empty() && occupied + advance(' ') + length > width {
            lines.push(std::mem::take(&mut line));
            occupied = 0.0;
        }
        if !line.is_empty() {
            line.push(' ');
            occupied += advance(' ');
        }
        for ch in word.chars() {
            if occupied + advance(ch) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                occupied = 0.0;
            }
            line.push(ch);
            occupied += advance(ch);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn labels(
    settings: &Settings,
    entries: &[Entry],
    now: i64,
    size: [f32; 2],
    font: f32,
    gap: f32,
) -> Vec<Label> {
    labels_with_metrics(
        settings,
        entries,
        now,
        size,
        font,
        gap,
        &LabelMetrics {
            band_width: 4.0,
            advance: &|ch| font * if ch as u32 >= 0x1100 { 1.0 } else { 0.62 },
        },
    )
}

pub struct LabelMetrics<'a> {
    pub band_width: f32,
    pub advance: &'a dyn Fn(char) -> f32,
}

pub fn labels_with_metrics(
    settings: &Settings,
    entries: &[Entry],
    now: i64,
    size: [f32; 2],
    font: f32,
    gap: f32,
    metrics: &LabelMetrics<'_>,
) -> Vec<Label> {
    let occurrences = clock_occurrences(
        settings,
        entries,
        now,
        now.saturating_add(settings.aperture_ms),
    );
    let radius = size[0].min(size[1]) * 0.4;
    let width = 200.0 * (font / 14.0).clamp(0.85, 1.4);
    let mut output = Vec::<Label>::new();
    let mut cells = HashMap::<(i32, i32), Vec<[f32; 4]>>::new();
    let mut searches = HashMap::<(i32, i32), usize>::new();
    let cell_size = width + gap;
    for occurrence in occurrences {
        let entry = &entries[occurrence.index];
        let title = entry
            .head
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("Untitled event");
        let lines = wrap(title, width - 16.0, metrics.advance);
        let mut time = entry.time_label(settings, now);
        if entry.preview {
            time.push_str(" · projected");
        }
        if occurrence.historical && !entry.outstanding_at(now, &settings.timezone) {
            time.push_str(" · past");
        }
        let time = wrap(&time, width - 16.0, metrics.advance);
        let height = (lines.len().max(1) as f32 + time.len().max(1) as f32) * font * 1.35 + 17.0;
        let at = occurrence.anchor_ms();
        let cross = settings.transverse(at, now, 0.0);
        let mut anchor = settings.position(at, now, size, 0.0);
        let offset = occurrence.offset_at(at, settings.aperture_ms, radius, metrics.band_width);
        anchor[0] += cross[0] * offset;
        anchor[2] += cross[2] * offset;
        let clearance = cross[0].abs() * width * 0.5 + cross[2].abs() * height * 0.5 + 30.0;
        let center = [
            anchor[0] + cross[0] * clearance,
            anchor[2] + cross[2] * clearance,
        ];
        let key = (
            (center[0] / cell_size).floor() as i32,
            (center[1] / cell_size).floor() as i32,
        );
        let mut attempt = searches.get(&key).copied().unwrap_or_default();
        let rect = loop {
            let angle = attempt as f32 * 2.399_963_1;
            let distance = (attempt as f32).sqrt() * (height + gap).max(32.0);
            let rect = [
                center[0] + angle.cos() * distance - width * 0.5,
                center[1] + angle.sin() * distance - height * 0.5,
                width,
                height,
            ];
            let occupied = cell_keys(rect, cell_size, gap);
            let collides = occupied.into_iter().any(|key| {
                cells.get(&key).is_some_and(|neighbors| {
                    neighbors.iter().any(|other| overlap(rect, *other, gap))
                })
            });
            let nearest_x = 0.0_f32.clamp(rect[0], rect[0] + width);
            let nearest_y = 0.0_f32.clamp(rect[1], rect[1] + height);
            if !collides && nearest_x.hypot(nearest_y) >= radius + 18.0 {
                break rect;
            }
            attempt += 1;
        };
        searches.insert(key, attempt + 1);
        for key in cell_keys(rect, cell_size, gap) {
            cells.entry(key).or_default().push(rect);
        }
        output.push(Label {
            id: entry.id.clone(),
            occurrence,
            anchor,
            rect,
            title: lines.join("\n"),
            time: time.join("\n"),
        });
    }
    output.sort_by_key(|label| (label.occurrence.time.from_ms, label.occurrence.index));
    output
}

fn cell_keys(rect: [f32; 4], size: f32, gap: f32) -> Vec<(i32, i32)> {
    let min = [
        ((rect[0] - gap) / size).floor() as i32,
        ((rect[1] - gap) / size).floor() as i32,
    ];
    let max = [
        ((rect[0] + rect[2] + gap) / size).floor() as i32,
        ((rect[1] + rect[3] + gap) / size).floor() as i32,
    ];
    (min[0]..=max[0])
        .flat_map(|x| (min[1]..=max[1]).map(move |y| (x, y)))
        .collect()
}
