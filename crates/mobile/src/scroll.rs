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
    pub(crate) moved: bool,
}

pub struct ScrollPlugin;

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gesture>()
            .add_observer(wheel)
            .add_systems(Update, touch);
    }
}

fn clamp(node: &ComputedNode, scroll: &mut ScrollPosition, delta: f32) {
    let maximum = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
    scroll.0.y = (scroll.0.y + delta).clamp(0.0, maximum);
}

fn wheel(
    mut event: On<Pointer<Scroll>>,
    mut content: Query<(&ComputedNode, &mut ScrollPosition), With<crate::app::Content>>,
) {
    if let Ok((node, mut scroll)) = content.get_mut(event.entity) {
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
    mut content: Query<(&ComputedNode, &mut ScrollPosition), With<crate::app::Content>>,
    mut buttons: Query<Entity, With<bevy::ui::Pressed>>,
    mut commands: Commands,
) {
    for event in events.read() {
        match event.phase {
            TouchPhase::Started if gesture.touch.is_none() => {
                gesture.touch = Some(event.id);
                gesture.start = event.position;
                gesture.last = event.position;
                gesture.moved = false;
            }
            TouchPhase::Moved if gesture.touch == Some(event.id) => {
                if event.position.distance(gesture.start) >= 10.0 {
                    gesture.moved = true;
                }
                if gesture.moved {
                    let delta = gesture.last.y - event.position.y;
                    for (node, mut scroll) in &mut content {
                        clamp(node, &mut scroll, delta);
                    }
                    for entity in &mut buttons {
                        commands.entity(entity).remove::<bevy::ui::Pressed>();
                    }
                }
                gesture.last = event.position;
            }
            TouchPhase::Ended | TouchPhase::Canceled if gesture.touch == Some(event.id) => {
                gesture.touch = None;
            }
            _ => {}
        }
    }
}
