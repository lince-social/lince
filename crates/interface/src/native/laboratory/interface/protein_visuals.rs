use bevy::{
    a11y::AccessibilityNode,
    prelude::*,
    text::TextLayoutInfo,
    ui::{
        CalculatedClip, ComputedStackIndex, ComputedUiRenderTargetInfo, ComputedUiTargetCamera,
        UiStack,
    },
};
use std::{
    fmt::Write,
    io,
    time::{Duration, Instant},
};

pub(super) fn caption(world: &World, control: Entity, expected: &str) -> bool {
    caption_entity(world, control, expected).is_some_and(|entity| readable(world, entity))
}

fn caption_entity(world: &World, control: Entity, expected: &str) -> Option<Entity> {
    world.get::<Children>(control)?.iter().find(|entity| {
        world
            .get::<Text>(*entity)
            .is_some_and(|text| text.0 == expected)
    })
}

pub(super) fn verify_paint(app: &mut App, control: Entity, expected: &str) -> io::Result<()> {
    use super::super::showoff::screenshot::capture_target;
    let world = app.world();
    let entity = caption_entity(world, control, expected)
        .ok_or_else(|| io::Error::other(format!("Missing Protein caption {expected:?}")))?;
    let region = PaintRegion::text(world, entity, expected)?;
    let target = paint_target(world)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let visible = capture_target(app, &target, deadline)?;
    verify_frame(app, visible, &[region], deadline)
}

pub(super) fn paint_target(world: &World) -> io::Result<Handle<Image>> {
    world
        .get_resource::<crate::native::app::OutputTarget>()
        .and_then(|target| match &target.0 {
            bevy::camera::RenderTarget::Image(target) => Some(target.handle.clone()),
            _ => None,
        })
        .ok_or_else(|| io::Error::other("The Protein paint check requires an offscreen target"))
}

pub(super) struct PaintRegion {
    entity: Entity,
    name: String,
    masks: Vec<Vec<UVec2>>,
}

impl PaintRegion {
    pub(super) fn text(world: &World, entity: Entity, name: &str) -> io::Result<Self> {
        let masks = glyph_masks(world, entity)
            .filter(|masks| !masks.is_empty())
            .ok_or_else(|| io::Error::other(format!("Protein text {name:?} has no glyph mask")))?;
        Ok(Self {
            entity,
            name: name.into(),
            masks,
        })
    }

    #[cfg(test)]
    pub(super) fn icon(world: &World, entity: Entity, name: &str) -> io::Result<Self> {
        let mask = icon_mask(world, entity)
            .filter(|mask| !mask.is_empty())
            .ok_or_else(|| io::Error::other(format!("Protein icon {name:?} has no atlas mask")))?;
        Ok(Self {
            entity,
            name: name.into(),
            masks: vec![mask],
        })
    }
}

pub(super) fn verify_frame(
    app: &mut App,
    visible: Image,
    regions: &[PaintRegion],
    deadline: Instant,
) -> io::Result<()> {
    use super::super::showoff::screenshot::capture_target;
    let target = paint_target(app.world())?;
    let mut visibility = Vec::with_capacity(regions.len());
    for region in regions {
        let before = app
            .world()
            .get::<Visibility>(region.entity)
            .copied()
            .unwrap_or_default();
        visibility.push((region.entity, before));
        app.world_mut()
            .entity_mut(region.entity)
            .insert(Visibility::Hidden);
    }
    let without_regions = capture_target(app, &target, deadline);
    for (entity, before) in visibility {
        app.world_mut().entity_mut(entity).insert(before);
    }
    let visible = visible
        .try_into_dynamic()
        .map_err(io::Error::other)?
        .into_rgba8();
    let without_regions = without_regions?
        .try_into_dynamic()
        .map_err(io::Error::other)?
        .into_rgba8();
    for region in regions {
        if region
            .masks
            .iter()
            .all(|mask| has_painted_pixels(&visible, &without_regions, mask))
        {
            continue;
        }
        let evidence = save_crop(&visible, &without_regions, &region.masks).map_or_else(
            |error| format!("cropped evidence failed: {error}"),
            |path| format!("cropped evidence: {}", path.display()),
        );
        return Err(io::Error::other(format!(
            "Protein visual {:?} did not paint its mask ({evidence})",
            region.name
        )));
    }
    app.update();
    Ok(())
}

#[cfg(test)]
fn icon_mask(world: &World, entity: Entity) -> Option<Vec<UVec2>> {
    let icon = world.get::<ImageNode>(entity)?;
    let atlas = icon.texture_atlas.as_ref()?;
    let image = world.get_resource::<Assets<Image>>()?.get(&icon.image)?;
    let rectangle = world
        .get_resource::<Assets<TextureAtlasLayout>>()?
        .get(&atlas.layout)?
        .textures
        .get(atlas.index)?;
    let size = world.get::<ComputedNode>(entity)?.size();
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let viewport = world
        .get::<ComputedUiRenderTargetInfo>(entity)?
        .physical_size()
        .as_vec2();
    let mut mask = Vec::new();
    for y in rectangle.min.y..rectangle.max.y {
        for x in rectangle.min.x..rectangle.max.x {
            if image.get_color_at(x, y).ok()?.to_linear().alpha < 0.5 {
                continue;
            }
            let relative = UVec2::new(x, y) - rectangle.min;
            let uv = (relative.as_vec2() + Vec2::splat(0.5)) / rectangle.size().as_vec2();
            let point = transform.transform_point2((uv - Vec2::splat(0.5)) * size);
            if point.is_finite() && point.cmpge(Vec2::ZERO).all() && point.cmplt(viewport).all() {
                mask.push(point.floor().as_uvec2());
            }
        }
    }
    Some(mask)
}

fn glyph_masks(world: &World, entity: Entity) -> Option<Vec<Vec<UVec2>>> {
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let layout = world.get::<TextLayoutInfo>(entity)?;
    let viewport = world
        .get::<ComputedUiRenderTargetInfo>(entity)?
        .physical_size();
    let atlases = world.get_resource::<Assets<Image>>()?;
    let mut masks = Vec::new();
    for glyph in &layout.glyphs {
        let rect = glyph.atlas_info.rect;
        let atlas = atlases.get(glyph.atlas_info.texture)?;
        let mut mask = Vec::new();
        for y in rect.min.y as u32..rect.max.y as u32 {
            for x in rect.min.x as u32..rect.max.x as u32 {
                let color = atlas.get_color_at(x, y).ok()?.to_linear();
                if color.alpha < 0.25 {
                    continue;
                }
                let offset = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - rect.center();
                let point =
                    transform.transform_point2(node.content_box().min + glyph.position + offset);
                if point.is_finite()
                    && point.cmpge(Vec2::ZERO).all()
                    && point.cmplt(viewport.as_vec2()).all()
                {
                    mask.push(point.floor().as_uvec2());
                }
            }
        }
        if !mask.is_empty() {
            masks.push(mask);
        }
    }
    Some(masks)
}

fn has_painted_pixels(
    visible: &image::RgbaImage,
    hidden: &image::RgbaImage,
    mask: &[UVec2],
) -> bool {
    if mask.is_empty() || visible.dimensions() != hidden.dimensions() {
        return false;
    }
    let changed = mask
        .iter()
        .filter(|point| {
            visible
                .get_pixel_checked(point.x, point.y)
                .zip(hidden.get_pixel_checked(point.x, point.y))
                .is_some_and(|(visible, hidden)| {
                    (0..3).any(|channel| visible[channel].abs_diff(hidden[channel]) > 18)
                })
        })
        .count();
    changed * 3 >= mask.len()
}

fn save_crop(
    visible: &image::RgbaImage,
    hidden: &image::RgbaImage,
    masks: &[Vec<UVec2>],
) -> io::Result<std::path::PathBuf> {
    let (minimum, maximum) = masks
        .iter()
        .flatten()
        .fold((UVec2::MAX, UVec2::ZERO), |(minimum, maximum), point| {
            (minimum.min(*point), maximum.max(*point))
        });
    let minimum = minimum.saturating_sub(UVec2::splat(4));
    let maximum = (maximum + UVec2::splat(5)).min(UVec2::new(
        visible.width().min(hidden.width()),
        visible.height().min(hidden.height()),
    ));
    let size = maximum.saturating_sub(minimum).min(UVec2::new(256, 128));
    if size.min_element() == 0 {
        return Err(io::Error::other(
            "The caption glyph crop is outside the captured targets",
        ));
    }
    let mut directory = tempfile::Builder::new();
    directory.prefix("lince-protein-caption-");
    let directory = if let Some(parent) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&parent)?;
        directory.tempdir_in(parent)?
    } else {
        directory.tempdir()?
    };
    for (image, filename) in [(visible, "caption.png"), (hidden, "without-caption.png")] {
        image::imageops::crop_imm(image, minimum.x, minimum.y, size.x, size.y)
            .to_image()
            .save(directory.path().join(filename))
            .map_err(io::Error::other)?;
    }
    Ok(directory.keep())
}

pub(super) fn result_caption(world: &World, mut control: Entity, expected: &str) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if world
            .get::<crate::native::protein_castle::ProteinCastle>(control)
            .is_some()
        {
            return result_text(world, control, expected)
                .is_some_and(|entity| readable(world, entity));
        }
        let Some(parent) = world.get::<ChildOf>(control) else {
            return false;
        };
        control = parent.parent();
    }
    false
}

pub(super) fn result_text(world: &World, editor: Entity, expected: &str) -> Option<Entity> {
    let mut pending = vec![editor];
    for _ in 0..512 {
        let entity = pending.pop()?;
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().rev());
        }
        if world
            .get::<Text>(entity)
            .is_some_and(|text| text.0 == expected)
        {
            return Some(entity);
        }
    }
    None
}

fn readable(world: &World, entity: Entity) -> bool {
    let (
        Some(node),
        Some(transform),
        Some(visibility),
        Some(viewport),
        Some(camera),
        Some(order),
        Some(layout),
        Some(ink),
    ) = (
        world.get::<ComputedNode>(entity),
        world.get::<UiGlobalTransform>(entity),
        world.get::<InheritedVisibility>(entity),
        world.get::<ComputedUiRenderTargetInfo>(entity),
        world
            .get::<ComputedUiTargetCamera>(entity)
            .and_then(ComputedUiTargetCamera::get),
        world.get::<ComputedStackIndex>(entity),
        world.get::<TextLayoutInfo>(entity),
        world.get::<TextColor>(entity),
    )
    else {
        return false;
    };
    let clip = world.get::<CalculatedClip>(entity);
    if !visibility.get()
        || node.size().min_element() <= 0.0
        || clip.is_some_and(CalculatedClip::is_fully_clipped)
    {
        return false;
    }
    let ink = ink.0.to_linear();
    let paper = background(world, entity);
    let viewport = Rect::from_corners(Vec2::ZERO, viewport.physical_size().as_vec2());
    let blockers: Vec<_> = world
        .resource::<UiStack>()
        .uinodes
        .iter()
        .filter_map(|&other| {
            let other = world.get_entity(other).ok()?;
            if other.get::<ComputedStackIndex>()?.0 <= order.0
                || other.get::<ComputedUiTargetCamera>()?.get() != Some(camera)
                || !other.get::<InheritedVisibility>()?.get()
                || other.get::<BackgroundColor>()?.0.to_linear().alpha < 1.0
            {
                return None;
            }
            Some((
                other.get::<UiGlobalTransform>()?.try_inverse()?,
                other.get::<ComputedNode>()?.content_box(),
                other.get::<CalculatedClip>(),
            ))
        })
        .collect();
    let atlases = world.resource::<Assets<Image>>();
    let mut drawn = false;
    for glyph in &layout.glyphs {
        let rect = glyph.atlas_info.rect;
        let Some(atlas) = atlases.get(glyph.atlas_info.texture) else {
            return false;
        };
        let mut has_ink = false;
        let mut readable_pixels = 0;
        let mut visible_pixels = 0;
        for y in rect.min.y as u32..rect.max.y as u32 {
            for x in rect.min.x as u32..rect.max.x as u32 {
                let Ok(pixel) = atlas.get_color_at(x, y) else {
                    return false;
                };
                let pixel = pixel.to_linear();
                if pixel.alpha <= 0.0 {
                    continue;
                }
                has_ink = true;
                let color = if glyph.atlas_info.is_alpha_mask {
                    LinearRgba {
                        alpha: ink.alpha * pixel.alpha,
                        ..ink
                    }
                } else {
                    pixel
                };
                if contrast(over(color, paper), paper) < 3.0 {
                    continue;
                }
                readable_pixels += 1;
                let pixel_position = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - rect.center();
                let local = node.content_box().min + glyph.position + pixel_position;
                let point = transform.transform_point2(local);
                if viewport.contains(point)
                    && clip.is_none_or(|clip| clip.contains_point(point))
                    && !blockers.iter().any(|(inverse, bounds, clip)| {
                        bounds.contains(inverse.transform_point2(point))
                            && clip.is_none_or(|clip| clip.contains_point(point))
                    })
                {
                    visible_pixels += 1;
                }
            }
        }
        if has_ink && (readable_pixels == 0 || visible_pixels * 4 < readable_pixels * 3) {
            return false;
        }
        drawn |= visible_pixels > 0;
    }
    drawn
}

fn background(world: &World, entity: Entity) -> LinearRgba {
    let mut colors = Vec::new();
    let mut current = Some(entity);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = current else {
            break;
        };
        if let Some(color) = world.get::<BackgroundColor>(entity) {
            let color = color.0.to_linear();
            if color.alpha > 0.0 {
                colors.push(color);
            }
            if color.alpha >= 1.0 {
                break;
            }
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    let paper = crate::native::theme::PAPER.to_linear();
    colors
        .into_iter()
        .rev()
        .fold(paper, |back, front| over(front, back))
}

fn over(front: LinearRgba, back: LinearRgba) -> LinearRgba {
    LinearRgba::rgb(
        front.red * front.alpha + back.red * (1.0 - front.alpha),
        front.green * front.alpha + back.green * (1.0 - front.alpha),
        front.blue * front.alpha + back.blue * (1.0 - front.alpha),
    )
}

fn contrast(first: LinearRgba, second: LinearRgba) -> f32 {
    let luminance =
        |color: LinearRgba| color.red * 0.2126 + color.green * 0.7152 + color.blue * 0.0722;
    let (first, second) = (luminance(first), luminance(second));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

pub(super) fn describe(world: &World, editor: Entity, label: &str) -> String {
    let mut output = String::from("Private Protein fixture controls:\n");
    let mut pending = vec![editor];
    let mut observed = 0;
    for _ in 0..384 {
        let Some(entity) = pending.pop() else {
            break;
        };
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter().rev().take(24));
        }
        let name = world
            .get::<AccessibilityNode>(entity)
            .and_then(|node| node.label());
        let text = world.get::<Text>(entity).map(|text| text.0.as_str());
        if (name.is_none() && text.is_none()) || observed >= 24 && name != Some(label) {
            continue;
        }
        observed += 1;
        let displayed = world.get::<Node>(entity).map(|node| node.display);
        let size = world.get::<ComputedNode>(entity).map(ComputedNode::size);
        let glyphs = world
            .get::<TextLayoutInfo>(entity)
            .map_or(0, |info| info.glyphs.len());
        let _ = writeln!(
            output,
            "{entity} {name:?} {text:?} {displayed:?} size={size:?} glyphs={glyphs}"
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_requires_changed_pixels_inside_the_glyph_mask() {
        let background = image::Rgba([25, 25, 25, 255]);
        let ink = image::Rgba([225, 225, 225, 255]);
        let hidden = image::RgbaImage::from_pixel(8, 8, background);
        let mut visible = hidden.clone();
        let mask = [UVec2::new(3, 4), UVec2::new(4, 4)];
        visible.put_pixel(1, 1, ink);
        assert!(!has_painted_pixels(&visible, &hidden, &mask));
        visible.put_pixel(3, 4, ink);
        assert!(has_painted_pixels(&visible, &hidden, &mask));
        assert!(!has_painted_pixels(&visible, &visible, &mask));
    }

    #[test]
    fn protein_caption_oracle_rejects_missing_transparent_moved_clipped_and_covered_text() {
        let (mut app, root) = super::super::fixture(UVec2::new(1000, 800), 1.0);
        let toggle = crate::native::dropdown::spawn(
            app.world_mut(),
            root,
            root,
            "Condition",
            "Kind",
            Vec::new(),
        );
        super::super::settle(&mut app);
        let label = app
            .world()
            .get::<Children>(toggle)
            .unwrap()
            .iter()
            .find(|entity| {
                app.world()
                    .get::<Text>(*entity)
                    .is_some_and(|text| text.0 == "Kind")
            })
            .unwrap();
        assert!(caption(app.world(), toggle, "Kind"));
        assert!(!caption(app.world(), toggle, "Missing"));

        let ink = *app.world().get::<TextColor>(label).unwrap();
        app.world_mut()
            .entity_mut(label)
            .insert(TextColor(Color::NONE));
        assert!(!caption(app.world(), toggle, "Kind"));
        let paper = background(app.world(), label);
        app.world_mut()
            .entity_mut(label)
            .insert(TextColor(Color::LinearRgba(paper)));
        assert!(!caption(app.world(), toggle, "Kind"));

        let transform = *app.world().get::<UiGlobalTransform>(label).unwrap();
        app.world_mut()
            .entity_mut(label)
            .insert((ink, UiGlobalTransform::from_xy(-1000.0, -1000.0)));
        assert!(!caption(app.world(), toggle, "Kind"));
        let clip = app.world().get::<CalculatedClip>(label).cloned();
        app.world_mut()
            .entity_mut(label)
            .insert((transform, CalculatedClip::FullyClipped));
        assert!(!caption(app.world(), toggle, "Kind"));
        if let Some(clip) = clip {
            app.world_mut().entity_mut(label).insert(clip);
        } else {
            app.world_mut().entity_mut(label).remove::<CalculatedClip>();
        }
        assert!(caption(app.world(), toggle, "Kind"));

        app.world_mut().spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            GlobalZIndex(1000),
            BackgroundColor(Color::BLACK),
            ChildOf(root),
        ));
        app.update();
        assert!(!caption(app.world(), toggle, "Kind"));
    }
}
