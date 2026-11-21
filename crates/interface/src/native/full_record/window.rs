// non-lean-yet
// Scope: mod creation, struct Metadata, struct CaptureSource, fn caption_text.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 73, 80.

use super::*;

mod creation;
pub(crate) use creation::restore;
pub use creation::{Creation, CreationState, WindowRecord};

#[derive(Component, Clone, Debug, PartialEq)]
pub struct Metadata {
    pub placement: [u8; 16],
    pub uid: String,
    pub title: String,
    pub app_id: String,
    pub live: bool,
    pub state: String,
}

#[derive(Component, Clone, PartialEq, Eq)]
pub(crate) struct CaptureSource {
    pub owner: Entity,
    pub placement: [u8; 16],
    pub uid: String,
}

impl Metadata {
    fn caption_parts(&self) -> [&str; 6] {
        [
            "Application metadata · read only\nTitle: ",
            lince_interface::text::prefix(&self.title, 1024),
            "\nApp ID: ",
            lince_interface::text::prefix(&self.app_id, 1024),
            "\nWindow: ",
            if self.live {
                self.state.as_str()
            } else {
                "closed or unavailable"
            },
        ]
    }

    fn caption(&self) -> String {
        caption_text(self.caption_parts())
    }
}

fn caption_text(parts: [&str; 6]) -> String {
    crate::host::performance::count(
        crate::host::performance::Count::WindowMetadataCaptionsPrepared,
    );
    parts.concat()
}

#[derive(Component)]
pub(crate) struct Header(Entity);

pub(crate) fn populate(world: &mut World, row: Entity, binding: &RecordBinding) {
    let metadata = world
        .get::<Metadata>(binding.area)
        .filter(|metadata| metadata.uid == binding.uid && binding.source == Source::Local);
    let source = if binding.source == Source::Local {
        world
            .get::<CaptureSource>(row)
            .filter(|source| source.owner == binding.area && source.uid == binding.uid)
            .cloned()
            .or_else(|| {
                world
                    .get::<crate::native::area::InfluenceArea>(binding.area)
                    .and_then(|area| area.protein.as_ref())
                    .and_then(|config| config.window_record.as_ref())
                    .filter(|window| window.uid == binding.uid)
                    .map(|window| CaptureSource {
                        owner: binding.area,
                        placement: window.placement,
                        uid: window.uid.clone(),
                    })
            })
            .or_else(|| {
                metadata.map(|metadata| CaptureSource {
                    owner: binding.area,
                    placement: metadata.placement,
                    uid: metadata.uid.clone(),
                })
            })
    } else {
        None
    };
    let caption = metadata.map(Metadata::caption);
    if world.get::<CaptureSource>(row) != source.as_ref() {
        if let Some(source) = source {
            world.entity_mut(row).insert(source);
        } else {
            world.entity_mut(row).remove::<CaptureSource>();
        }
    }
    let Some(caption) = caption else {
        return;
    };
    let entity = crate::native::edit_mode::label(world, row, &caption, 14.0);
    world.entity_mut(entity).insert((
        Header(binding.area),
        Name::new("window.properties.metadata"),
        Node {
            width: percent(100),
            min_width: px(0),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    crate::native::layout::desktop::opening::ui::window_panel(world, row, binding);
    crate::native::capture::window_panel(world, row, binding);
    crate::native::layout::desktop::windowed_fullscreen::panel(world, row, binding);
    crate::native::layout::desktop::compact::window_panel(world, row, binding);
    crate::native::layout::desktop::scratch::window_panel(world, row, binding);
}

pub(crate) fn refresh(metadata: Query<Ref<Metadata>>, mut headers: Query<(&Header, &mut Text)>) {
    let _headers =
        crate::host::performance::Span::new(crate::host::performance::Stage::WindowMetadataHeaders);
    for (header, mut text) in &mut headers {
        let Ok(metadata) = metadata.get(header.0) else {
            continue;
        };
        if !metadata.is_changed() {
            continue;
        }
        let parts = metadata.caption_parts();
        if parts.iter().try_fold(text.0.as_str(), |remaining, part| {
            remaining.strip_prefix(*part)
        }) != Some("")
        {
            text.0 = caption_text(parts);
        }
    }
}

#[cfg(test)]
mod tests;
