use super::*;
use crate::actions::Action;
use bevy::{
    asset::RenderAssetUsages,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub(super) fn size(world: &World, viewport: Entity) -> Vec2 {
    world
        .get::<ComputedNode>(viewport)
        .map(|node| node.size() * node.inverse_scale_factor())
        .filter(|size| size.min_element() > 1.0)
        .unwrap_or(Vec2::new(320.0, 300.0))
}

pub(super) fn samples(
    settings: &Settings,
    time: &nucleus::schedule::TimeRange,
    now: i64,
    size: Vec2,
    unwind: f32,
    budget: usize,
) -> Vec<Vec3> {
    let from = time.from_ms.max(now);
    let end = time.until_ms.unwrap_or(from);
    let count = if end == from {
        1
    } else {
        (((end - from) as f64 / settings.aperture_ms as f64 * 512.0).ceil() as usize)
            .clamp(2, budget.max(2))
    };
    (0..count)
        .map(|index| {
            let at = from
                + ((end - from) as f64 * index as f64 / (count - 1).max(1) as f64).round() as i64;
            Vec3::from_array(settings.position(at, now, size.to_array(), unwind))
        })
        .collect()
}

pub(super) fn occurrence_points(
    settings: &Settings,
    occurrence: &model::Occurrence,
    width: f32,
    now: i64,
    size: Vec2,
    unwind: f32,
) -> Vec<Vec3> {
    let mut points = samples(settings, &occurrence.time, now, size, unwind, 512);
    let radius = size.min_element() * 0.4;
    let samples = points.len().saturating_sub(1).max(1) as f64;
    for (index, point) in points.iter_mut().enumerate() {
        let at = occurrence.time.from_ms
            + ((occurrence.time.until_ms.unwrap_or(occurrence.time.from_ms)
                - occurrence.time.from_ms) as f64
                * index as f64
                / samples) as i64;
        let offset = occurrence.offset_at(at, settings.aperture_ms, radius, width);
        *point += Vec3::from_array(settings.transverse(at, now, unwind)) * offset;
    }
    points
}

fn summary_lines(title: &str, font: f32, width: f32) -> Vec<String> {
    let face = ttf_parser::Face::parse(
        include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf"),
        0,
    )
    .ok();
    let advance = |ch| {
        face.as_ref()
            .and_then(|face| {
                face.glyph_index(ch)
                    .and_then(|glyph| face.glyph_hor_advance(glyph))
            })
            .map_or(font, |advance| {
                f32::from(advance) / f32::from(face.as_ref().unwrap().units_per_em()) * font
            })
    };
    let measure = |text: &str| text.chars().map(advance).sum::<f32>();
    let mut lines = vec![String::new()];
    for word in title.split_whitespace() {
        let current = lines.last_mut().unwrap();
        if !current.is_empty() && measure(current) + advance(' ') + measure(word) > width {
            lines.push(String::new());
        }
        let current = lines.last_mut().unwrap();
        if !current.is_empty() {
            current.push(' ');
        }
        for ch in word.chars() {
            if measure(lines.last().unwrap()) + advance(ch) > width
                && !lines.last().unwrap().is_empty()
            {
                lines.push(String::new());
            }
            lines.last_mut().unwrap().push(ch);
        }
    }
    if lines.len() > 2 {
        lines.truncate(2);
        let last = lines.last_mut().unwrap();
        while !last.is_empty() && measure(last) + advance('…') > width {
            last.pop();
        }
        last.push('…');
    }
    lines
}

fn svg(
    settings: &Settings,
    view: &View,
    now: i64,
    size: Vec2,
    density: f32,
    palette: &palette::Palette,
    geometry: bool,
) -> String {
    let face = palette::css(palette.face);
    let ink = palette::css(palette.ink);
    let muted = palette::css(palette.muted);
    let track = palette::css(palette.track);
    let present = palette::css(palette.present);
    let duration = (settings.aperture_ms as f64
        + (settings.horizon_ms - settings.aperture_ms) as f64 * f64::from(view.unwind))
        as i64;
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' fill='none' width='{}' height='{}' viewBox='{} {} {} {}'>",
        size.x,
        size.y,
        -size.x / 2.0,
        -size.y / 2.0,
        size.x,
        size.y
    );
    let path = |points: &[Vec3]| -> String {
        points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                format!(
                    "{} {:.2} {:.2} ",
                    if index == 0 { "M" } else { "L" },
                    point.x,
                    point.z
                )
            })
            .collect()
    };
    let radius = size.min_element() * 0.4;
    if view.unwind < 0.95 {
        svg.push_str(&format!(
            "<circle cx='0' cy='0' r='{radius}' fill='{face}' fill-opacity='{}'/>",
            0.96 * (1.0 - view.unwind),
        ));
    }
    let base = samples(
        settings,
        &nucleus::schedule::TimeRange {
            from_ms: now,
            until_ms: Some(now + duration),
        },
        now,
        size,
        view.unwind,
        2048,
    );
    svg.push_str(&format!(
        "<path d='{}' fill='none' stroke='{track}' stroke-width='1' stroke-linecap='round'/>",
        path(&base)
    ));
    let pixels = if view.unwind > 0.9 {
        size.x * density
    } else {
        size.min_element() * std::f32::consts::TAU * 0.4 * density
    };
    let interval = model::tick_interval(duration, pixels, 12.0)
        .max(model::tick_interval(duration, 128.0, 1.0));
    let label_interval = model::tick_interval(duration, pixels, 85.0).max(interval);
    let ticks = if view.unwind < 0.001 {
        settings.rim_ticks(now)
    } else {
        ((now.div_euclid(interval) * interval + interval)..now + duration)
            .step_by(interval as usize)
            .take(128)
            .map(|at_ms| model::RimTick {
                at_ms,
                major: at_ms.rem_euclid(label_interval) == 0,
                label: settings.tick_label(at_ms, label_interval),
            })
            .collect()
    };
    for tick in ticks {
        let at = tick.at_ms;
        let point = Vec3::from_array(settings.position(at, now, size.to_array(), view.unwind));
        let cross = Vec3::from_array(settings.transverse(at, now, view.unwind));
        let major = tick.major;
        let start = point - cross * if major { 11.0 } else { 5.0 };
        let end = point;
        svg.push_str(&format!(
            "<path d='M {} {} L {} {}' stroke='{}' stroke-width='0.8'/>",
            start.x,
            start.z,
            end.x,
            end.z,
            if major { &muted } else { &track }
        ));
        if major {
            let label = tick.label;
            let label_point = point - cross * 23.0;
            svg.push_str(&format!("<text x='{}' y='{}' dominant-baseline='central' text-anchor='middle' fill='{muted}' font-size='10' font-family='Lato'>{label}</text>", label_point.x, label_point.z));
        }
    }
    let occurrences = model::occurrences(&view.entries, now, now + duration, &settings.timezone);
    for occurrence in occurrences.into_iter().filter(|_| geometry) {
        let entry = &view.entries[occurrence.index];
        let points =
            occurrence_points(settings, &occurrence, palette.width, now, size, view.unwind);
        let color = palette::css(palette.event(occurrence.lane, view.selected.contains(&entry.id)));
        if occurrence.time.until_ms.is_some() {
            svg.push_str(&format!(
                "<path d='{}' fill='none' stroke='{color}' stroke-width='{}' stroke-linecap='round'/>",
                path(&points), palette.width
            ));
        } else if let Some(point) = points.first() {
            let cross =
                Vec3::from_array(settings.transverse(occurrence.time.from_ms, now, view.unwind))
                    * 3.0;
            svg.push_str(&format!(
                "<path d='M {} {} L {} {}' stroke='{color}' stroke-width='1.5'/><circle cx='{}' cy='{}' r='2.4' fill='{color}'/>",
                point.x - cross.x,
                point.z - cross.z,
                point.x + cross.x,
                point.z + cross.z,
                point.x,
                point.z
            ));
        }
    }
    let now_point = Vec3::from_array(settings.position(now, now, size.to_array(), view.unwind));
    let direction = Vec3::from_array(settings.transverse(now, now, view.unwind));
    let inner = now_point
        - direction
            * if view.unwind < 0.001 {
                radius * 0.42
            } else {
                18.0
            };
    let end = now_point - direction * if view.unwind < 0.001 { 4.0 } else { -16.0 };
    if geometry {
        svg.push_str(&format!("<path d='M {} {} L {} {}' stroke='{present}' stroke-width='1.2' stroke-linecap='round'/>", inner.x, inner.z, end.x, end.z));
        if view.unwind < 0.001 {
            svg.push_str(&format!(
                "<circle cx='{}' cy='{}' r='2' fill='{present}'/>",
                end.x, end.z
            ));
        }
    }
    if view.unwind < 0.5 {
        let current = chrono::DateTime::from_timestamp_millis(now)
            .zip(settings.timezone.parse::<nucleus::schedule::Tz>().ok())
            .map(|(time, zone)| {
                time.with_timezone(&zone)
                    .format(if settings.aperture_ms <= 60_000 {
                        "%H:%M:%S"
                    } else {
                        "%H:%M"
                    })
                    .to_string()
            })
            .unwrap_or_else(|| "--:--".into());
        let aperture = settings.aperture_label();
        let precision = if settings.aperture_ms <= 60_000 {
            1000
        } else {
            60_000
        };
        let window = format!(
            "{} – {}",
            settings.tick_label(now, precision),
            settings.tick_label(now + settings.aperture_ms, precision)
        );
        let scale = (palette.font / 16.0).clamp(0.8, 1.25);
        svg.push_str(&format!("<g opacity='{}' font-family='Lato' text-anchor='middle'><text x='0' y='{}' fill='{ink}' font-size='{}'>{current}</text><text x='0' y='{}' fill='{ink}' font-size='12'>{aperture}</text><text x='0' y='{}' fill='{muted}' font-size='10'>{window}</text><g transform='translate(-13 {}) scale(0.8125)' stroke='{ink}' stroke-width='1.3' stroke-linejoin='round'><path d='M16 2C7 2 3 7 3 13C3 18 6 21 9 22V29H23V22C26 21 29 18 29 13C29 7 25 2 16 2Z'/><circle cx='10' cy='13' r='3'/><circle cx='22' cy='13' r='3'/><path d='M16 17L13 21H19Z M9 24H23 M12 24V29 M16 24V29 M20 24V29'/></g><text x='0' y='{}' fill='{ink}' font-size='11' letter-spacing='3'>memento</text><text x='0' y='{}' fill='{ink}' font-size='11' letter-spacing='4'>mori</text>", 1.0 - view.unwind * 2.0, -radius * 0.63, 26.0 * scale, -radius * 0.50, -radius * 0.40, radius * 0.44, radius * 0.68, radius * 0.79));
        for (row, (index, remaining)) in
            model::summaries(&view.entries, now, now + settings.aperture_ms)
                .into_iter()
                .enumerate()
        {
            let entry = &view.entries[index];
            let y = -radius * 0.25 + row as f32 * radius * 0.32;
            let title = entry
                .head
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Untitled event");
            let font = 12.5 * scale.min(1.12);
            let lines = summary_lines(title, font, radius * 1.12);
            for (index, line) in lines.iter().enumerate() {
                let line = line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                svg.push_str(&format!(
                    "<text x='0' y='{}' fill='{ink}' font-size='{font}'>{line}</text>",
                    y + index as f32 * (font + 1.0)
                ));
            }
            let timing = entry.time_label(settings, now);
            let amount = model::countdown(remaining);
            let baseline = y + lines.len() as f32 * (font + 1.0);
            svg.push_str(&format!("<text x='0' y='{baseline}' fill='{muted}' font-size='11'>{timing}</text><text x='0' y='{}' fill='{ink}' font-size='12'>{amount}</text>", baseline + 14.0));
        }
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    svg
}

fn fonts(cjk: bool) -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static BASE: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    static CJK: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    let base = BASE.get_or_init(|| {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        for data in [
            include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").as_slice(),
            include_bytes!("../../../../institute/assets/fonts/DejaVuSans/DejaVuSans.ttf")
                .as_slice(),
            include_bytes!(
                "../../../../institute/assets/fonts/NotoSansSymbols2/NotoSansSymbols2-Regular.ttf"
            )
            .as_slice(),
        ] {
            fonts.load_font_data(data.to_vec());
        }
        std::sync::Arc::new(fonts)
    });
    if cjk {
        CJK.get_or_init(|| {
            let mut fonts = base.as_ref().clone();
            fonts.load_font_data(include_bytes!("../../../../institute/assets/fonts/NotoSansMonoCJK/NotoSansMonoCJKjp-Regular.otf").to_vec());
            std::sync::Arc::new(fonts)
        }).clone()
    } else {
        base.clone()
    }
}

fn rasterize(svg: &str, size: Vec2, density: f32) -> Option<Image> {
    let mut options = resvg::usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    options.fontdb = fonts(svg.chars().any(|ch| {
        (0x2e80..=0x9fff).contains(&(ch as u32)) || (0xac00..=0xd7ff).contains(&(ch as u32))
    }));
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let density = density.max(0.01).min(4096.0 / size.max_element());
    let width = (size.x * density).ceil().max(1.0) as u32;
    let height = (size.y * density).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(density, density),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    for rgba in pixels.chunks_exact_mut(4) {
        if rgba[3] > 0 {
            for index in 0..3 {
                rgba[index] = ((u32::from(rgba[index]) * 255) / u32::from(rgba[3])).min(255) as u8;
            }
        }
    }
    Some(Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

pub(super) fn preview(world: &World, owner: Entity) -> Option<(Entity, Image)> {
    let view = world.get::<View>(owner)?;
    let settings = &world.get::<TimeSettings>(owner)?.0;
    let size = world.get::<crate::canvas::CanvasItem>(owner)?.size;
    let palette = palette::Palette::resolve(world, owner);
    let svg = svg(
        settings,
        view,
        chrono::Utc::now().timestamp_millis(),
        size,
        1.0,
        &palette,
        true,
    );
    Some((view.viewport, rasterize(&svg, size, 1.0)?))
}

pub(super) fn select_at(world: &mut World, owner: Entity, point: Vec2) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let settings = &world.get::<TimeSettings>(owner).unwrap().0;
    let now = chrono::Utc::now().timestamp_millis();
    let size = size(world, view.viewport);
    if let Some(motion) = world.get::<motion::Motion>(owner) {
        let width = view.palette.as_ref().map_or(4.0, |palette| palette.width);
        let selected = motion
            .bands
            .values()
            .filter(|band| !band.retiring)
            .filter_map(|band| {
                let points = motion::points(band, settings, now, size, view.unwind, width);
                let distance = if points.len() == 1 {
                    point.distance(points[0].xz())
                } else {
                    points
                        .windows(2)
                        .map(|pair| segment_distance(point, pair[0].xz(), pair[1].xz()))
                        .fold(f32::INFINITY, f32::min)
                };
                (distance <= 12.0).then_some((distance, band.entry.id.clone()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id);
        if let Some(id) = selected {
            ui::Select(vec![id]).apply(world, owner);
        }
        return;
    }
    let duration = (settings.aperture_ms as f64
        + (settings.horizon_ms - settings.aperture_ms) as f64 * f64::from(view.unwind))
        as i64;
    let occurrences = model::occurrences(&view.entries, now, now + duration, &settings.timezone);
    let width = view.palette.as_ref().map_or(4.0, |palette| palette.width);
    let distance = |occurrence: &model::Occurrence| -> f32 {
        let points = occurrence_points(settings, occurrence, width, now, size, view.unwind);
        if points.len() == 1 {
            point.distance(points[0].xz())
        } else {
            points
                .windows(2)
                .map(|pair| segment_distance(point, pair[0].xz(), pair[1].xz()))
                .fold(f32::INFINITY, f32::min)
        }
    };
    if let Some(occurrence) = occurrences
        .iter()
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        && distance(occurrence) <= 12.0
    {
        let id = view.entries[occurrence.index].id.clone();
        ui::Select(vec![id]).apply(world, owner);
    }
}

fn segment_distance(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let length = (b - a).length_squared();
    let fraction = if length > 0.0 {
        ((point - a).dot(b - a) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    point.distance(a.lerp(b, fraction))
}

pub(super) fn update(world: &mut World, mut wake_at: Local<Option<std::time::Instant>>) {
    if crate::laboratory::active(world) {
        return;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    let mut animated = false;
    let mut active = false;
    for owner in owners {
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            continue;
        };
        if world
            .get::<crate::workspace::Workspaces>(root)
            .zip(world.get::<crate::workspace::WorkspaceMember>(owner))
            .is_some_and(|(spaces, member)| spaces.active != member.0)
        {
            continue;
        }
        active = true;
        let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        if !settings.valid() {
            continue;
        }
        let palette = palette::Palette::resolve(world, owner);
        if world.get::<View>(owner).unwrap().palette.as_ref() != Some(&palette) {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.palette = Some(palette.clone());
            view.revision = view.revision.wrapping_add(1);
        }
        chrome::colors(world, owner, &palette);
        let target = if settings.mode == Mode::Straight {
            1.0
        } else {
            0.0
        };
        let mut view = world.get_mut::<View>(owner).unwrap();
        let delta = view.animation_at.elapsed().as_secs_f32().min(0.1) / 0.3;
        view.animation_at = std::time::Instant::now();
        let change = (target - view.unwind).clamp(-delta, delta);
        view.unwind += change;
        animated |= (view.unwind - target).abs() > 0.001;
        let (viewport, label, fallback, revision, unwind) = (
            view.viewport,
            view.present,
            view.fallback,
            view.revision,
            view.unwind,
        );
        let size = size(world, viewport);
        let spatial = world
            .get::<crate::topology::view::View>(root)
            .is_some_and(|view| view.spatial)
            && world
                .get::<crate::topology::presentation::Surface>(owner)
                .is_some();
        chrome::presentation(world, owner, !spatial && settings.mode == Mode::Coiled);
        let source = settings
            .area
            .as_ref()
            .map(|id| {
                world
                    .query::<&crate::area::InfluenceArea>()
                    .iter(world)
                    .find(|area| area.id == *id)
                    .map(|area| ui::headline(&area.name))
                    .unwrap_or_else(|| "Custom schedule".into())
            })
            .unwrap_or_else(|| "Local schedule".into());
        let text = format!(
            "Now {} · {} · {}{}",
            settings.label(now),
            settings.timezone,
            source,
            if fallback {
                " · device zone unavailable; UTC fallback"
            } else {
                ""
            }
        );
        if let Some(mut label) = world.get_mut::<Text>(label)
            && label.0 != text
        {
            label.0 = text;
        }
        let density = world
            .get::<ComputedNode>(viewport)
            .map_or(1.0, |node| node.inverse_scale_factor().recip());
        let stamp = (
            settings.clone(),
            revision,
            now / (settings.aperture_ms / 12).clamp(16, 1000),
            [
                (size.x * density).round() as u32,
                (size.y * density).round() as u32,
            ],
            unwind.to_bits(),
            spatial,
        );
        let changed = world.get::<View>(owner).unwrap().rendered.as_ref() != Some(&stamp);
        let duration = if spatial {
            settings.horizon_ms
        } else {
            (settings.aperture_ms as f64
                + (settings.horizon_ms - settings.aperture_ms) as f64 * f64::from(unwind))
                as i64
        };
        let moving = motion::update(world, owner, now, duration, changed);
        animated |= moving;
        if changed {
            if spatial {
                world.entity_mut(viewport).remove::<ImageNode>();
            } else {
                let view = world.get::<View>(owner).unwrap();
                let svg = svg(
                    &settings,
                    view,
                    now,
                    size,
                    density,
                    &palette,
                    world
                        .get::<crate::topology::presentation::Surface>(owner)
                        .is_none(),
                );
                if let Some(image) = rasterize(&svg, size, density) {
                    world.init_resource::<Assets<Image>>();
                    let previous = world.get::<View>(owner).unwrap().image.clone();
                    let handle = if let Some(handle) = previous {
                        let _ = world
                            .resource_mut::<Assets<Image>>()
                            .insert(handle.id(), image);
                        handle
                    } else {
                        world.resource_mut::<Assets<Image>>().add(image)
                    };
                    world
                        .entity_mut(viewport)
                        .insert(ImageNode::new(handle.clone()));
                    world.get_mut::<View>(owner).unwrap().image = Some(handle);
                }
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.rendered = Some(stamp);
            view.detail_revision = u64::MAX;
        }
        let round = !spatial && settings.mode == Mode::Coiled && unwind < 0.001;
        let labels = annotations::update(world, owner, now, size, round, changed, &palette);
        let following = world
            .get::<motion::Motion>(owner)
            .is_some_and(|motion| motion.active);
        animated |= following;
        if changed || moving || following {
            scene::update(world, owner, &settings, now, size, spatial);
            scene::annotations(world, owner, &labels, &palette);
        }
        let hand = round && !chrome::controls_open(world, owner);
        scene::hand(world, owner, &settings, now, size, hand, &palette);
        animated |= hand;
        scene::placement(world, owner, viewport, spatial);
    }
    let delay = if animated { 16 } else { 1000 };
    if active && (animated || wake_at.is_none_or(|at| at.elapsed().as_millis() >= 900)) {
        *wake_at = Some(std::time::Instant::now());
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                wake.ring();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_density_tracks_zoom_and_transparent_edges_keep_their_color() {
        let source = "<svg xmlns='http://www.w3.org/2000/svg' width='420' height='420'><circle cx='210' cy='210' r='100.3' fill='white'/></svg>";
        let image = rasterize(source, Vec2::splat(420.0), 4.0).unwrap();
        assert_eq!(image.size(), UVec2::splat(1680));
        let pixels = image.data.as_ref().unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        assert!(
            pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel[..3] == [255, 255, 255])
        );
        let maximum = rasterize(source, Vec2::splat(420.0), 20.0).unwrap();
        assert!(maximum.size().max_element() <= 4096);
    }

    #[test]
    fn round_face_has_one_rim_and_keeps_diagnostics_inside_configuration() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((Node::default(), TimeSettings(Settings::default())))
            .id();
        populate(&mut world, owner);
        let palette = palette::Palette::resolve(&world, owner);
        let svg = svg(
            &Settings::default(),
            world.get::<View>(owner).unwrap(),
            1_800_000,
            Vec2::splat(420.0),
            1.0,
            &palette,
            false,
        );
        assert_eq!(svg.matches("<circle cx='0' cy='0'").count(), 1);
        assert_eq!(svg.matches("dominant-baseline='central'").count(), 12);
        assert!(!svg.contains("simulation"));
        assert!(!svg.contains("Scheduled work"));
        assert!(svg.contains("memento"));
        assert!(svg.contains("mori"));
    }

    #[test]
    fn interior_titles_wrap_and_limit_long_unicode_without_losing_card_data() {
        let lines = summary_lines(
            "Prepare a thoughtful response to the design review",
            14.0,
            100.0,
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
        let unicode = summary_lines("日程 🕰 café & work", 14.0, 80.0);
        assert!(unicode.len() <= 2);
        assert!(!unicode[0].is_empty());
    }

    #[test]
    fn clock_text_fallbacks_cover_arrows_symbols_and_cjk_and_reuse_font_data() {
        let database = fonts(true);
        assert!(std::sync::Arc::ptr_eq(&database, &fonts(true)));
        for symbol in ['→', '✓', '日'] {
            assert!(database.faces().any(|font| {
                database
                    .with_face_data(font.id, |bytes, index| {
                        ttf_parser::Face::parse(bytes, index)
                            .ok()
                            .and_then(|face| face.glyph_index(symbol))
                            .is_some()
                    })
                    .unwrap_or(false)
            }));
        }
    }

    #[test]
    fn equal_time_points_select_their_own_visible_lane() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let settings = Settings {
            cursor: CursorMode::Fixed,
            ..default()
        };
        let owner = world
            .spawn((Node::default(), TimeSettings(settings.clone())))
            .id();
        populate(&mut world, owner);
        let now = chrono::Utc::now().timestamp_millis();
        let entries: Vec<_> = (0..4)
            .map(|index| Entry {
                id: format!("event-{index}"),
                record_uid: format!("r:{index}"),
                head: format!("Task {index}"),
                quantity: "-1".into(),
                category: model::Category::Timed,
                time: Some(nucleus::schedule::TimeRange {
                    from_ms: now + 60_000,
                    until_ms: None,
                }),
                origin: serde_json::json!({"kind":"manual"}),
                preview: false,
                start_date: None,
                due_date: None,
            })
            .collect();
        world.get_mut::<View>(owner).unwrap().entries = entries.clone();
        let size = size(&world, world.get::<View>(owner).unwrap().viewport);
        for occurrence in model::occurrences(&entries, now, now + settings.aperture_ms, "UTC") {
            let point = occurrence_points(&settings, &occurrence, 4.0, now, size, 0.0)[0];
            select_at(&mut world, owner, point.xz());
            assert_eq!(
                world.get::<View>(owner).unwrap().selected,
                [entries[occurrence.index].id.clone()]
            );
        }
    }
}
