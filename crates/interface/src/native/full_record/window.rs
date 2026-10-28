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

impl Metadata {
    fn caption(&self) -> String {
        format!(
            "Application metadata · read only\nTitle: {}\nApp ID: {}\nWindow: {}",
            self.title.chars().take(1024).collect::<String>(),
            self.app_id.chars().take(1024).collect::<String>(),
            if self.live {
                self.state.as_str()
            } else {
                "closed or unavailable"
            },
        )
    }
}

#[derive(Component)]
pub(crate) struct Header(Entity);

pub(crate) fn populate(world: &mut World, row: Entity, binding: &RecordBinding) {
    let Some(metadata) = world
        .get::<Metadata>(binding.area)
        .filter(|metadata| metadata.uid == binding.uid && binding.source == Source::Local)
    else {
        return;
    };
    let caption = metadata.caption();
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
    for (header, mut text) in &mut headers {
        let Ok(metadata) = metadata.get(header.0) else {
            continue;
        };
        if !metadata.is_changed() {
            continue;
        }
        let caption = metadata.caption();
        if text.0 != caption {
            text.0 = caption;
        }
    }
}

#[cfg(test)]
mod tests;
