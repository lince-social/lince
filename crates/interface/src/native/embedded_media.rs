mod model;
#[cfg(test)]
mod tests;

use crate::native::{actions::Action, sand_panel as panel};
use bevy::{input_focus::InputFocus, picking::pointer::PointerButton, prelude::*};

#[derive(Component)]
pub(crate) struct Media {
    pub content: Entity,
    pub controls: Entity,
}

#[derive(Component)]
pub(crate) struct OwnsMediaScroll;

#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub owner: Entity,
    pub content: Entity,
    pub controls: Entity,
}

pub(crate) fn frame(world: &mut World, parent: Entity, title: &str) -> Frame {
    let mut accessibility = accesskit::Node::new(accesskit::Role::Group);
    accessibility.set_label(title);
    accessibility.set_description(
        "Right-click or press the Menu key or F10 for media controls. Escape hides controls.",
    );
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            bevy::input_focus::tab_navigation::TabIndex(0),
            bevy::a11y::AccessibilityNode::from(accessibility),
            ChildOf(parent),
        ))
        .observe(click)
        .observe(keyboard)
        .id();
    let content = panel::column(world, owner);
    let controls = panel::column(world, owner);
    world.entity_mut(controls).insert((
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            right: px(0),
            padding: UiRect::all(px(10)),
            row_gap: px(6),
            flex_direction: FlexDirection::Column,
            max_height: px(360),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ZIndex(20),
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    crate::native::scroll_sand::attach(world, controls);
    crate::native::edit_mode::label(world, controls, title, 14.0);
    panel::button(world, controls, owner, "Hide controls", Toggle(false));
    world.entity_mut(owner).insert(Media { content, controls });
    Frame {
        owner,
        content,
        controls,
    }
}

#[derive(Clone, Copy)]
struct Toggle(bool);

impl Action for Toggle {
    fn apply(&self, world: &mut World, owner: Entity) {
        set_controls(world, owner, self.0);
    }
}

pub(crate) fn set_controls(world: &mut World, owner: Entity, shown: bool) {
    let Some(controls) = world.get::<Media>(owner).map(|media| media.controls) else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(controls) {
        node.display = if shown { Display::Flex } else { Display::None };
    }
    if !shown {
        let mut current = world.get_resource::<InputFocus>().and_then(InputFocus::get);
        while let Some(entity) = current {
            if entity == controls {
                if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
                    focus.set(owner, bevy::input_focus::FocusCause::Navigated);
                }
                break;
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

fn click(mut event: On<Pointer<Click>>, media: Query<&Media>, mut commands: Commands) {
    if event.button != PointerButton::Secondary || media.get(event.entity).is_err() {
        return;
    }
    event.propagate(false);
    let owner = event.entity;
    commands.queue(move |world: &mut World| {
        let shown = world.get::<Media>(owner).is_some_and(|media| {
            world
                .get::<Node>(media.controls)
                .is_some_and(|node| node.display != Display::None)
        });
        set_controls(world, owner, !shown);
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.set(owner, bevy::input_focus::FocusCause::Pressed);
        }
    });
}

fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    media: Query<&Media>,
    mut commands: Commands,
) {
    if !event.input.state.is_pressed() || media.get(event.focused_entity).is_err() {
        return;
    }
    let shown = match event.input.key_code {
        KeyCode::ContextMenu | KeyCode::F10 => true,
        KeyCode::Escape => false,
        _ => return,
    };
    let owner = event.focused_entity;
    event.propagate(false);
    commands.queue(move |world: &mut World| set_controls(world, owner, shown));
}

pub(crate) fn resource(
    world: &mut World,
    parent: Entity,
    resource: &lince_resources::Resource,
    allow_local: bool,
) -> Frame {
    let frame = frame(world, parent, &resource.title);
    if !allow_local
        && lince_resources::address(&resource.uri).is_ok_and(|uri| uri.scheme() == "file")
    {
        crate::native::edit_mode::label(
            world,
            frame.content,
            "This remote reference needs a downloadable source address.",
            13.0,
        );
        return frame;
    }
    let mut resource = resource.clone();
    if !allow_local {
        resource.locations.retain(|value| {
            lince_resources::address(value)
                .is_ok_and(|uri| matches!(uri.scheme(), "http" | "https"))
        });
    }
    if model::supports(&resource.media_type) {
        model::spawn(world, frame, resource.clone(), allow_local);
    } else if matches!(
        resource.media_type.as_str(),
        "application/pdf" | "application/epub+zip"
    ) {
        crate::native::document_viewer::embedded(
            world,
            frame.content,
            frame.controls,
            resource.clone(),
        );
    } else if resource.media_type.starts_with("video/") || resource.media_type.starts_with("audio/")
    {
        crate::native::video_castle::embedded(world, frame, resource.clone());
    } else {
        crate::native::resource_preview::spawn(world, frame.content, &resource);
    }
    frame
}

pub(crate) fn install(app: &mut App) {
    model::install(app);
}

pub(crate) fn is_model(media_type: &str) -> bool {
    model::supports(media_type)
}

pub(crate) fn open_model(
    world: &mut World,
    parent: Entity,
    resource: &lince_resources::Resource,
    allow_local: bool,
) {
    let frame = resource_frame(world, parent, resource, allow_local);
    set_controls(world, frame.owner, false);
}

fn resource_frame(
    world: &mut World,
    parent: Entity,
    resource: &lince_resources::Resource,
    allow_local: bool,
) -> Frame {
    let frame = self::resource(world, parent, resource, allow_local);
    for text in [&resource.attribution, &resource.license] {
        if !text.is_empty() {
            crate::native::edit_mode::label(world, frame.controls, text, 12.0);
        }
    }
    frame
}
