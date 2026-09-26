use super::*;
use bevy::{
    asset::RenderAssetUsages,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub(super) fn display_width(state: &DocumentViewer, view: &View) -> f32 {
    if !view.info.as_ref().is_some_and(|info| info.is_pdf) {
        return view.viewport_size.x;
    }
    let width = if state.position().mode == Mode::Pages {
        view.layout.map_or(view.viewport_size.x, |layout| {
            view.viewport_size
                .x
                .min(view.viewport_size.y * layout.width as f32 / layout.height as f32)
        })
    } else {
        view.viewport_size.x
    };
    width * state.zoom
}

pub(super) fn update(world: &mut World) {
    let replies: Vec<_> = world
        .resource::<worker::Worker>()
        .replies
        .lock()
        .unwrap()
        .try_iter()
        .collect();
    for reply in replies {
        match reply {
            worker::Reply::Pick { owner, path } => {
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.picking = false;
                    if let Some(path) = path {
                        ui::open(world, owner, path);
                    }
                }
            }
            worker::Reply::Render { owner, key, result } => {
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    continue;
                };
                view.busy = false;
                view.requested.clear();
                if view.key.as_ref() != Some(&key) {
                    continue;
                }
                match result {
                    Ok(rendered) => {
                        view.info = Some(rendered.info);
                        view.layout = Some(rendered.layout);
                        let content = view.content;
                        let count = view.info.as_ref().unwrap().sections.len();
                        let mut state = world.get_mut::<DocumentViewer>(owner).unwrap();
                        if state.position().section >= count {
                            state.position_mut().section = count - 1;
                            continue;
                        }
                        for (index, tile) in rendered.tiles {
                            let image = Image::new(
                                Extent3d {
                                    width: tile.width,
                                    height: tile.height,
                                    depth_or_array_layers: 1,
                                },
                                TextureDimension::D2,
                                tile.rgba,
                                TextureFormat::Rgba8UnormSrgb,
                                RenderAssetUsages::RENDER_WORLD,
                            );
                            let handle = world.resource_mut::<Assets<Image>>().add(image);
                            let entity = world
                                .spawn((
                                    ImageNode::new(handle.clone()),
                                    Visibility::Hidden,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        ..default()
                                    },
                                    ChildOf(content),
                                ))
                                .id();
                            if let Some((old, _)) = world
                                .get_mut::<View>(owner)
                                .unwrap()
                                .tiles
                                .insert(index, (entity, handle))
                            {
                                world.despawn(old);
                            }
                        }
                    }
                    Err(error) => {
                        view.failed = true;
                        status(world, owner, &error);
                    }
                }
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        refresh(world, owner);
    }
}

fn refresh(world: &mut World, owner: Entity) {
    let state = world.get::<DocumentViewer>(owner).unwrap().clone();
    if state.path.is_empty() {
        return;
    }
    let view = world.get::<View>(owner).unwrap();
    let viewport = view.viewport;
    let Some(node) = world.get::<ComputedNode>(viewport) else {
        return;
    };
    let size = node.size() * node.inverse_scale_factor;
    if size.x < 128.0 || size.y < 64.0 {
        return;
    }
    let pdf = view.info.as_ref().map_or_else(
        || state.path.to_ascii_lowercase().ends_with(".pdf"),
        |info| info.is_pdf,
    );
    let width = if pdf {
        size.x * state.zoom * 1.5
    } else {
        size.x / state.zoom
    };
    let width = ((width as u32).div_ceil(32) * 32).clamp(128, lince_document::MAX_WIDTH);
    let key = worker::Key {
        path: state.path.clone(),
        section: state.position().section,
        width,
        epoch: view.epoch,
    };
    let changed = view.key.as_ref() != Some(&key);
    if view
        .key
        .as_ref()
        .is_none_or(|old| old.path != key.path || old.section != key.section)
    {
        let input = view.section_input;
        world
            .get_mut::<bevy::text::EditableText>(input)
            .unwrap()
            .editor
            .set_text(&(key.section + 1).to_string());
    }
    let view = world.get::<View>(owner).unwrap();
    if changed {
        let entities: Vec<_> = view.tiles.values().map(|(entity, _)| *entity).collect();
        for entity in entities {
            world.despawn(entity);
        }
        let mut view = world.get_mut::<View>(owner).unwrap();
        if view
            .key
            .as_ref()
            .is_none_or(|old| old.path != key.path || old.epoch != key.epoch)
        {
            view.info = None;
        }
        view.key = Some(key.clone());
        view.layout = None;
        view.tiles.clear();
        view.requested.clear();
        view.restore = true;
        view.failed = false;
        status(world, owner, "Opening document…");
    }
    let mut view = world.get_mut::<View>(owner).unwrap();
    if view.viewport_size != size {
        view.restore = true;
    }
    view.viewport_size = size;
    if view.failed {
        return;
    }
    let Some(layout) = view.layout else {
        request(world, owner, key, Vec::new());
        return;
    };
    let view = world.get::<View>(owner).unwrap();
    let display_width = display_width(&state, view);
    let scale = display_width / layout.width as f32;
    let height = layout.height as f32 * scale;
    let content = view.content;
    let mode_label = view.mode_label;
    let restore = view.restore;
    let node = world.get::<Node>(content).unwrap();
    if node.width != px(display_width) || node.height != px(height) {
        let mut node = world.get_mut::<Node>(content).unwrap();
        node.width = px(display_width);
        node.height = px(height);
    }
    let overflow = if pdf && state.zoom > 1.0 {
        Overflow::scroll()
    } else {
        Overflow::scroll_y()
    };
    if world.get::<Node>(viewport).unwrap().overflow != overflow {
        world.get_mut::<Node>(viewport).unwrap().overflow = overflow;
    }
    let offset = if restore {
        let offset = state.position().offset(height, size.y);
        world.get_mut::<ScrollPosition>(viewport).unwrap().0.y = offset;
        world.get_mut::<View>(owner).unwrap().restore = false;
        offset
    } else {
        let offset = world
            .get::<ScrollPosition>(viewport)
            .unwrap()
            .0
            .y
            .clamp(0.0, (height - size.y).max(0.0));
        world
            .get_mut::<DocumentViewer>(owner)
            .unwrap()
            .position_mut()
            .set_offset(offset, height, size.y);
        offset
    };
    let position = world.get::<DocumentViewer>(owner).unwrap().position();
    if let Some(mut text) = world.get_mut::<Text>(mode_label) {
        let label = if position.mode == Mode::Pages {
            "Page mode"
        } else {
            "Scroll mode"
        };
        if text.0 != label {
            text.0 = label.into();
        }
    }
    let first = (offset / scale / lince_document::TILE_HEIGHT as f32) as u32;
    let end = (((offset + size.y) / scale / lince_document::TILE_HEIGHT as f32).ceil() as u32 + 1)
        .min(layout.height.div_ceil(lince_document::TILE_HEIGHT));
    let first = first.saturating_sub(1);
    let end = end.min(first + 10);
    let view = world.get::<View>(owner).unwrap();
    let evict: Vec<_> = view
        .tiles
        .iter()
        .filter(|(index, _)| **index < first || **index >= end)
        .map(|(index, (entity, _))| (*index, *entity))
        .collect();
    let missing: Vec<_> = (first..end)
        .filter(|index| !view.tiles.contains_key(index))
        .collect();
    for (index, entity) in evict {
        world.despawn(entity);
        world.get_mut::<View>(owner).unwrap().tiles.remove(&index);
    }
    let tiles: Vec<_> = world
        .get::<View>(owner)
        .unwrap()
        .tiles
        .iter()
        .map(|(index, (entity, image))| (*index, *entity, image.id()))
        .collect();
    for (index, entity, image) in tiles {
        if crate::castle::image_ready(world, image) {
            world
                .get_mut::<Visibility>(entity)
                .unwrap()
                .set_if_neq(Visibility::Inherited);
        }
        let top = index * lince_document::TILE_HEIGHT;
        let tile_height =
            (lince_document::TILE_HEIGHT + lince_document::TILE_OVERLAP).min(layout.height - top);
        let node = world.get::<Node>(entity).unwrap();
        if node.top != px(top as f32 * scale)
            || node.width != px(display_width)
            || node.height != px(tile_height as f32 * scale)
        {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            node.top = px(top as f32 * scale);
            node.left = px(0);
            node.width = px(display_width);
            node.height = px(tile_height as f32 * scale);
        }
    }
    let view = world.get::<View>(owner).unwrap();
    let info = view.info.as_ref().unwrap();
    let section = position.section.min(info.sections.len() - 1);
    let noun = if info.is_pdf { "Page" } else { "Chapter" };
    let message = format!(
        "{} · {noun} {} / {} · {:.0}% in {} · {:.0}% zoom{}",
        info.title,
        section + 1,
        info.sections.len(),
        position.fraction * 100.0,
        noun.to_lowercase(),
        state.zoom * 100.0,
        if missing.is_empty() {
            ""
        } else {
            " · Rendering…"
        }
    );
    status(world, owner, &message);
    if !missing.is_empty() {
        request(world, owner, key, missing);
    }
}

fn request(world: &mut World, owner: Entity, key: worker::Key, tiles: Vec<u32>) {
    let view = world.get::<View>(owner).unwrap();
    if view.busy || view.failed {
        return;
    }
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let requested = tiles.iter().copied().collect();
    match world
        .resource::<worker::Worker>()
        .sender
        .try_send(worker::Job::Render(worker::Request {
            owner,
            key,
            tiles,
            wake: wake.clone(),
        })) {
        Ok(()) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.busy = true;
            view.requested = requested;
        }
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            if let Some(wake) = wake {
                wake.ring();
            }
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
            world.get_mut::<View>(owner).unwrap().failed = true;
            status(
                world,
                owner,
                "The document reader stopped. Restart the interface to reopen it.",
            );
        }
    }
}
