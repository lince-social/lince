use crate::app::{ButtonIntent, Content, Input};
use bevy::{prelude::*, text::EditableText};
use std::sync::atomic::{AtomicBool, Ordering};

pub static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Resource, Default)]
pub struct Snapshot(String);

pub fn publish(world: &mut World) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let Some(window) = world.query::<&Window>().iter(world).next() else {
        return;
    };
    let screen = Vec2::new(
        window.physical_width() as f32,
        window.physical_height() as f32,
    );
    let viewport = world
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<Content>>()
        .iter(world)
        .next()
        .map(|(node, transform)| Rect::from_center_size(transform.translation, node.size()));
    let mut items = Vec::new();
    for (entity, node, transform) in world
        .query::<(Entity, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
    {
        let (kind, label, password) = if let Some(input) = world.get::<Input>(entity) {
            let password = input.key == "login/password";
            let value = if password {
                String::new()
            } else {
                world
                    .get::<EditableText>(entity)
                    .map(|text| text.value().to_string())
                    .unwrap_or_default()
            };
            ("edit", format!("{}: {value}", input.title), password)
        } else if let Some(input) = world.get::<lince_interface::location::Editor>(entity) {
            let value = if input.secret { String::new() } else { world.get::<EditableText>(entity).map(|text| text.value().to_string()).unwrap_or_default() };
            ("edit", format!("{}: {value}", input.title), input.secret)
        } else if world.get::<ButtonIntent>(entity).is_some() || world.get::<lince_interface::location::Button>(entity).is_some() {
            let label = world
                .get::<Children>(entity)
                .map(|children| {
                    children
                        .iter()
                        .filter_map(|child| world.get::<Text>(child))
                        .map(|text| text.0.as_str())
                        .collect::<String>()
                })
                .unwrap_or_default();
            ("button", label, false)
        } else if let Some(text) = world.get::<Text>(entity) {
            if world.get::<ChildOf>(entity).is_some_and(|parent| {
                world.get::<ButtonIntent>(parent.parent()).is_some()
                    || world.get::<lince_interface::location::Button>(parent.parent()).is_some()
                    || world.get::<lince_interface::location::Editor>(parent.parent()).is_some()
                    || world.get::<Input>(parent.parent()).is_some()
            }) {
                continue;
            }
            let mut label = text.0.clone();
            if let Some(children) = world.get::<Children>(entity) {
                for child in children.iter() {
                    if let Some(span) = world.get::<TextSpan>(child) {
                        label.push_str(&span.0);
                    }
                }
            }
            ("text", label, false)
        } else {
            continue;
        };
        if label.is_empty() {
            continue;
        }
        let mut bounds = Rect::from_center_size(transform.translation, node.size());
        bounds.min = bounds.min.max(Vec2::ZERO);
        bounds.max = bounds.max.min(screen);
        let mut parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        while let Some(current) = parent {
            if world.get::<Content>(current).is_some() {
                if let Some(viewport) = viewport {
                    bounds.min = bounds.min.max(viewport.min);
                    bounds.max = bounds.max.min(viewport.max);
                }
                break;
            }
            parent = world.get::<ChildOf>(current).map(ChildOf::parent);
        }
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            continue;
        }
        items.push(serde_json::json!({"token":entity.to_bits().to_string(),"kind":kind,"label":label,"password":password,"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y]}));
    }
    items.sort_by(|a, b| {
        a["bounds"][1]
            .as_f64()
            .partial_cmp(&b["bounds"][1].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a["bounds"][0]
                    .as_f64()
                    .partial_cmp(&b["bounds"][0].as_f64())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let encoded = serde_json::Value::Array(items).to_string();
    if world.resource::<Snapshot>().0 == encoded {
        return;
    }
    if crate::android::accessibility_snapshot(&encoded).is_ok() {
        world.resource_mut::<Snapshot>().0 = encoded;
    }
}

pub fn activate(world: &mut World, token: u64, action: i32) {
    if action == 4096 || action == 8192 {
        for (node, mut scroll) in world
            .query_filtered::<(&ComputedNode, &mut ScrollPosition), With<Content>>()
            .iter_mut(world)
        {
            let direction = if action == 4096 { 1.0 } else { -1.0 };
            scroll.0.y = (scroll.0.y
                + direction * node.size().y * node.inverse_scale_factor() * 0.7)
                .max(0.0);
        }
        return;
    }
    if action != 16 {
        return;
    }
    let entity = Entity::from_bits(token);
    if let Some(button) = world.get::<ButtonIntent>(entity) {
        let intent = button.0.clone();
        crate::app::queue_intent(world, intent);
    } else if world.get::<lince_interface::location::Button>(entity).is_some() {
        lince_interface::location::click(world, entity);
    } else if let (Some(editor), Some(text)) = (world.get::<lince_interface::location::Editor>(entity), world.get::<EditableText>(entity)) {
        crate::android::open_location_editor(entity, editor, text, &world.resource::<crate::app::Mobile>().scope_key());
    } else if let (Some(input), Some(text)) = (
        world.get::<Input>(entity),
        world.get::<EditableText>(entity),
    ) {
        crate::android::open_editor(
            input,
            text,
            &world.resource::<crate::app::Mobile>().scope_key(),
        );
    }
}
