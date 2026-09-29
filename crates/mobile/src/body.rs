use bevy::prelude::*;
use lince_interface::theme::{INK, PURPLE, Typography};

#[derive(Resource)]
struct Fonts {
    bold: Handle<Font>,
    italic: Handle<Font>,
}

impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let mut fonts = world.resource_mut::<Assets<Font>>();
        Self {
            bold: fonts.add(Font::from_bytes(
                include_bytes!("../../../institute/assets/fonts/Lato/Lato-Bold.ttf").to_vec(),
            )),
            italic: fonts.add(Font::from_bytes(
                include_bytes!("../../../institute/assets/fonts/Lato/Lato-Italic.ttf").to_vec(),
            )),
        }
    }
}

pub fn render(world: &mut World, parent: Entity, source: &str) {
    world.init_resource::<Fonts>();
    let end = source
        .char_indices()
        .nth(32_000)
        .map_or(source.len(), |(index, _)| index);
    let blocks = lince_interface::markup::parse(&source[..end]);
    let shortened = end < source.len() || blocks.len() > 256;
    for block in blocks.into_iter().take(256) {
        let links: Vec<_> = block
            .runs
            .iter()
            .filter_map(|run| {
                run.link
                    .as_ref()
                    .map(|link| (run.text.clone(), link.clone()))
            })
            .collect();
        let size = match block.heading {
            1 => 28.0,
            2 => 25.0,
            3.. => 22.0,
            _ => 18.0,
        };
        let font = world.resource::<Typography>().text(size);
        let entity = world
            .spawn((
                ChildOf(parent),
                Text::new(""),
                font,
                TextColor(INK),
                TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
                Node {
                    width: percent(100),
                    flex_shrink: 0.0,
                    padding: UiRect::left(px(if block.quote { 12.0 } else { 0.0 })),
                    ..default()
                },
            ))
            .id();
        for run in block.runs {
            let mut font = world.resource::<Typography>().text(size);
            if run.bold || block.heading > 0 {
                font.font = world.resource::<Fonts>().bold.clone().into();
            } else if run.italic {
                font.font = world.resource::<Fonts>().italic.clone().into();
            }
            let span = world
                .spawn((
                    ChildOf(entity),
                    TextSpan::new(&run.text),
                    font,
                    TextColor(if run.link.is_some() { PURPLE } else { INK }),
                ))
                .id();
            if run.code {
                world
                    .entity_mut(span)
                    .insert(bevy::text::TextBackgroundColor(Color::srgba(
                        0.5, 0.5, 0.5, 0.15,
                    )));
            }
            if run.strike {
                world.entity_mut(span).insert(bevy::text::Strikethrough);
            }
        }
        if let Some(image) = block.image {
            crate::images::render(world, parent, &image);
        }
        for (title, link) in links {
            crate::app::button(
                world,
                parent,
                &format!("Open {title}"),
                crate::app::Intent::OpenLink(link),
            );
        }
    }
    if shortened {
        crate::app::label(
            world,
            parent,
            "Preview shortened. The editor below keeps the complete text.",
            14.0,
        );
    }
}
