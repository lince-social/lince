use bevy::{
    input::{
        mouse::MouseScrollUnit,
        touch::{TouchInput, TouchPhase},
    },
    prelude::*,
};

#[derive(Resource, Default)]
pub(crate) struct Gesture {
    touch: Option<u64>,
    start: Vec2,
    last: Vec2,
    target: Option<Entity>,
    pub(crate) moved: bool,
}

pub struct ScrollPlugin;

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gesture>()
            .add_observer(wheel)
            .add_systems(
                PreUpdate,
                touch
                    .after(bevy::input::InputSystems)
                    .before(bevy::picking::PickingSystems::Hover),
            );
    }
}

fn clamp(node: &ComputedNode, scroll: &mut ScrollPosition, delta: f32) {
    let maximum = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
}

fn wheel(
    mut event: On<Pointer<Scroll>>,
    mut content: Query<(&Node, &ComputedNode, &mut ScrollPosition)>,
) {
    if let Ok((style, node, mut scroll)) = content.get_mut(event.entity)
        && style.overflow.y == bevy::ui::OverflowAxis::Scroll
    {
        let scale = if event.unit == MouseScrollUnit::Line {
            32.0
        } else {
            1.0
        };
        clamp(node, &mut scroll, -event.y * scale);
        event.propagate(false);
    }
}

fn touch(
    mut events: MessageReader<TouchInput>,
    mut gesture: ResMut<Gesture>,
    stack: Option<Res<bevy::ui::UiStack>>,
    nodes: Query<(
        &Node,
        &ComputedNode,
        &bevy::ui::UiGlobalTransform,
        Option<&ChildOf>,
        Option<&InheritedVisibility>,
    )>,
    mut content: Query<(&ComputedNode, &mut ScrollPosition)>,
    mut buttons: Query<Entity, With<bevy::ui::Pressed>>,
    mut commands: Commands,
    dragging: Option<Res<crate::kanban::Dragging>>,
) {
    for event in events.read() {
        match event.phase {
            TouchPhase::Started if gesture.touch.is_none() => {
                gesture.touch = Some(event.id);
                gesture.start = event.position;
                gesture.last = event.position;
                gesture.moved = false;
                gesture.target = stack.as_ref().and_then(|stack| {
                    let mut current = stack.uinodes.iter().rev().copied().find(|entity| {
                        nodes
                            .get(*entity)
                            .is_ok_and(|(_, node, transform, _, visible)| {
                                visible.is_none_or(|visible| visible.get())
                                    && node
                                        .normalize_point(
                                            *transform,
                                            event.position / node.inverse_scale_factor(),
                                        )
                                        .is_some_and(|point| point.abs().max_element() <= 0.5)
                            })
                    });
                    while let Some(entity) = current {
                        let Ok((style, _, _, parent, _)) = nodes.get(entity) else {
                            break;
                        };
                        if style.overflow.y == bevy::ui::OverflowAxis::Scroll {
                            return Some(entity);
                        }
                        current = parent.map(ChildOf::parent);
                    }
                    None
                });
            }
            TouchPhase::Moved if gesture.touch == Some(event.id) => {
                if event.position.distance(gesture.start) >= 10.0 {
                    gesture.moved = true;
                }
                if gesture.moved {
                    let delta = gesture.last.y - event.position.y;
                    if !dragging.as_ref().is_some_and(|dragging| dragging.0) {
                        if let Some(target) = gesture.target
                            && let Ok((node, mut scroll)) = content.get_mut(target)
                        {
                            clamp(node, &mut scroll, delta);
                        }
                    }
                    for entity in &mut buttons {
                        commands.entity(entity).remove::<bevy::ui::Pressed>();
                    }
                }
                gesture.last = event.position;
            }
            TouchPhase::Ended | TouchPhase::Canceled if gesture.touch == Some(event.id) => {
                gesture.touch = None;
                gesture.target = None;
            }
            _ => {}
        }
    }
}
