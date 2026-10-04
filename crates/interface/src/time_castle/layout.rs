use super::*;
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub index: usize,
    pub time: TimeRange,
    pub lane: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
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
    ordered.sort_by(|(a, a_time), (b, b_time)| {
        a_time
            .from_ms
            .cmp(&b_time.from_ms)
            .then_with(|| entries[*a].id.cmp(&entries[*b].id))
    });
    let mut occupied = BinaryHeap::<std::cmp::Reverse<(i64, usize)>>::new();
    let mut next = 0;
    ordered
        .into_iter()
        .map(|(index, time)| {
            let lane = if occupied.peek().is_some_and(|end| end.0.0 <= time.from_ms) {
                occupied.pop().unwrap().0.1
            } else {
                let lane = next;
                next += 1;
                lane
            };
            occupied.push(std::cmp::Reverse((
                time.until_ms
                    .unwrap_or_else(|| time.from_ms.saturating_add(1)),
                lane,
            )));
            Occurrence { index, time, lane }
        })
        .collect()
}

pub fn lane_offset(lane: usize, count: usize, radius: f32) -> f32 {
    -(lane as f32) * (radius * 0.22 / count.saturating_sub(1).max(1) as f32).min(7.0)
}

fn overlap(a: [f32; 4], b: [f32; 4], gap: f32) -> bool {
    a[0] < b[0] + b[2] + gap
        && a[0] + a[2] + gap > b[0]
        && a[1] < b[1] + b[3] + gap
        && a[1] + a[3] + gap > b[1]
}

fn wrap(title: &str, width: f32, font: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut occupied = 0.0;
    let advance = |ch: char| font * if ch as u32 >= 0x1100 { 1.0 } else { 0.62 };
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
    let occurrences = occurrences(
        entries,
        now,
        now.saturating_add(settings.aperture_ms),
        &settings.timezone,
    );
    let count = occurrences
        .iter()
        .map(|entry| entry.lane + 1)
        .max()
        .unwrap_or(1);
    let radius = size[0].min(size[1]) * 0.4;
    let width = 174.0 * (font / 13.0).clamp(0.85, 1.4);
    let mut groups =
        BTreeMap::<usize, Vec<(Occurrence, [f32; 3], [f32; 3], String, String, f32)>>::new();
    for occurrence in occurrences {
        let entry = &entries[occurrence.index];
        let title = entry
            .head
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("Untitled event");
        let lines = wrap(title, width - 16.0, font);
        let mut time = entry.time_label(settings, now);
        if entry.preview {
            time.push_str(" · projected");
        }
        let time = wrap(&time, width - 16.0, font * 0.9);
        let height =
            (lines.len().max(1) as f32 + time.len().max(1) as f32 * 0.9) * font * 1.35 + 17.0;
        let cross = settings.transverse(occurrence.time.from_ms, now, 0.0);
        let mut anchor = settings.position(occurrence.time.from_ms, now, size, 0.0);
        let offset = lane_offset(occurrence.lane, count, radius);
        anchor[0] += cross[0] * offset;
        anchor[2] += cross[2] * offset;
        let sector = (((cross[0].atan2(-cross[2]) + std::f32::consts::TAU) / std::f32::consts::TAU
            * 16.0)
            .floor() as usize)
            % 16;
        groups.entry(sector).or_default().push((
            occurrence,
            anchor,
            cross,
            lines.join("\n"),
            time.join("\n"),
            height,
        ));
    }
    let mut output = Vec::<Label>::new();
    let mut rectangles = Vec::<[f32; 4]>::new();
    for group in groups.into_values() {
        let direction = group.iter().fold([0.0_f32; 2], |mut sum, row| {
            sum[0] += row.2[0];
            sum[1] += row.2[2];
            sum
        });
        let length = direction[0].hypot(direction[1]).max(0.001);
        let cross = [direction[0] / length, direction[1] / length];
        let height =
            group.iter().map(|row| row.5).sum::<f32>() + gap * group.len().saturating_sub(1) as f32;
        let mut distance =
            radius + 40.0 + cross[0].abs() * width * 0.5 + cross[1].abs() * height * 0.5;
        let step = if cross[0].abs() > cross[1].abs() {
            width + gap
        } else {
            height + gap
        };
        let rect = loop {
            let rect = [
                cross[0] * distance - width * 0.5,
                cross[1] * distance - height * 0.5,
                width,
                height,
            ];
            let collides = rectangles
                .iter()
                .any(|previous| overlap(rect, *previous, gap));
            let nearest_x = 0.0_f32.clamp(rect[0], rect[0] + width);
            let nearest_y = 0.0_f32.clamp(rect[1], rect[1] + height);
            if !collides && nearest_x.hypot(nearest_y) >= radius + 24.0 {
                break rect;
            }
            distance += step;
        };
        rectangles.push(rect);
        let mut offset = if cross[1] < -0.55 { height } else { 0.0 };
        for (occurrence, anchor, _, title, time, height) in group {
            if cross[1] < -0.55 {
                offset -= height;
            }
            output.push(Label {
                occurrence,
                anchor,
                rect: [rect[0], rect[1] + offset, width, height],
                title,
                time,
            });
            offset += if cross[1] < -0.55 { -gap } else { height + gap };
        }
    }
    output.sort_by_key(|label| (label.occurrence.time.from_ms, label.occurrence.index));
    output
}
