use super::*;
use crate::actions::Action;
pub(super) use crate::castle_feed::button;
use bevy::text::EditableText;

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(6),
                row_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn input(
    world: &mut World,
    parent: Entity,
    title: &str,
    value: &str,
    limit: usize,
) -> Entity {
    let input = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(input).insert(ChildOf(parent));
    let mut text = world.get_mut::<EditableText>(input).unwrap();
    text.allow_newlines = false;
    text.max_characters = Some(limit);
    text.visible_lines = Some(1.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(input) {
        node.set_label(title);
    }
    input
}

#[derive(Clone)]
pub(super) enum Control {
    Create,
    Open,
    Browse,
    Previous,
    Next,
    Mode,
    Zoom(f32),
    Go,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, DocumentViewer::default());
            return;
        }
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let (viewport, input, section_input) = (view.viewport, view.input, view.section_input);
        match self {
            Self::Open => {
                let path = world
                    .get::<EditableText>(input)
                    .unwrap()
                    .value()
                    .to_string();
                open(world, owner, path.trim().into());
            }
            Self::Browse => {
                if view.picking {
                    return;
                }
                let Some(worker) = world.get_resource::<worker::Worker>() else {
                    return;
                };
                let sender = worker.reply_sender.clone();
                let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
                world.get_mut::<View>(owner).unwrap().picking = true;
                std::thread::spawn(move || {
                    let path = rfd::FileDialog::new()
                        .add_filter("Documents", &["pdf", "epub"])
                        .pick_file()
                        .map(|path| path.to_string_lossy().into_owned());
                    let _ = sender.send(worker::Reply::Pick { owner, path });
                    if let Some(wake) = wake {
                        wake.ring();
                    }
                });
            }
            Self::Mode => {
                let mut state = world.get_mut::<DocumentViewer>(owner).unwrap();
                let position = state.position_mut();
                position.mode = if position.mode == Mode::Scroll {
                    Mode::Pages
                } else {
                    Mode::Scroll
                };
                world.get_mut::<View>(owner).unwrap().restore = true;
            }
            Self::Zoom(amount) => {
                let mut state = world.get_mut::<DocumentViewer>(owner).unwrap();
                state.zoom = (state.zoom + amount).clamp(0.5, 2.0);
                world.get_mut::<View>(owner).unwrap().restore = true;
            }
            Self::Go => {
                let number = world
                    .get::<EditableText>(section_input)
                    .unwrap()
                    .value()
                    .to_string()
                    .parse::<usize>();
                let count = view.info.as_ref().map_or(0, |info| info.sections.len());
                if let Ok(number) = number
                    && (1..=count).contains(&number)
                {
                    let mut state = world.get_mut::<DocumentViewer>(owner).unwrap();
                    let position = state.position_mut();
                    position.section = number - 1;
                    position.fraction = 0.0;
                    world.get_mut::<View>(owner).unwrap().restore = true;
                }
            }
            Self::Previous | Self::Next => {
                let Some(layout) = view.layout else {
                    return;
                };
                let count = view.info.as_ref().map_or(0, |info| info.sections.len());
                let viewport_height = view.viewport_size.y;
                let display_height = layout.height as f32
                    * runtime::display_width(world.get::<DocumentViewer>(owner).unwrap(), view)
                    / layout.width as f32;
                let old = world.get::<ScrollPosition>(viewport).unwrap().0.y;
                let forward = matches!(self, Self::Next);
                let mut state = world.get_mut::<DocumentViewer>(owner).unwrap();
                let position = state.position_mut();
                let remaining = (display_height - viewport_height).max(0.0);
                if position.mode == Mode::Pages
                    && ((forward && old < remaining - 1.0) || (!forward && old > 1.0))
                {
                    let next = (old
                        + if forward { 1.0 } else { -1.0 } * (viewport_height - 48.0).max(64.0))
                    .clamp(0.0, remaining);
                    position.set_offset(next, display_height, viewport_height);
                    world.get_mut::<ScrollPosition>(viewport).unwrap().0.y = next;
                } else if (forward && position.section + 1 < count)
                    || (!forward && position.section > 0)
                {
                    position.section = if forward {
                        position.section + 1
                    } else {
                        position.section - 1
                    };
                    position.fraction = if !forward && position.mode == Mode::Pages {
                        1.0
                    } else {
                        0.0
                    };
                    world.get_mut::<View>(owner).unwrap().restore = true;
                }
            }
            Self::Create => {}
        }
    }
}

pub(super) fn open(world: &mut World, owner: Entity, path: String) {
    if path.is_empty() || !valid_path(&path) {
        status(world, owner, "Choose a PDF or EPUB file");
        return;
    }
    let path = if std::path::Path::new(&path).is_relative() {
        std::env::current_dir()
            .map(|directory| directory.join(&path).to_string_lossy().into_owned())
            .unwrap_or(path)
    } else {
        path
    };
    let Some(mut state) = world.get_mut::<DocumentViewer>(owner) else {
        return;
    };
    state.path = path.clone();
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.epoch = view.epoch.wrapping_add(1);
    view.restore = true;
    view.failed = false;
    let input = view.input;
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&path);
}
