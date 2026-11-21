mod controller;
mod observer;
mod saved_place;

use bevy::prelude::*;
use nucleus::location::{Status, View};

use crate::{controls, theme::Typography};

use super::{Button, Editor, Field, Intent, Panel, PasswordMask, actions::apply, clock::Stamp};

fn label(world: &mut World, parent: Entity, value: &str) -> Entity {
    let font = world.resource::<Typography>().text(16.0);
    world
        .spawn((
            ChildOf(parent),
            Text::new(value),
            font,
            crate::style::text(crate::tokens::Token::Ink),
        ))
        .id()
}

fn button(world: &mut World, parent: Entity, panel: Entity, title: &str, intent: Intent) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Button {
                panel,
                intent: intent.clone(),
            },
            Node {
                min_height: px(44),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::style::border(crate::tokens::Token::Accent),
        ))
        .id();
    label(world, entity, title);
    world.entity_mut(entity).observe(
        move |_: On<bevy::ui_widgets::Activate>, mut commands: Commands| {
            let intent = intent.clone();
            commands.queue(move |world: &mut World| apply(world, panel, intent));
        },
    );
}

fn input(world: &mut World, parent: Entity, title: &str, value: &str) -> Entity {
    label(world, parent, title);
    let bundle = controls::single_line_editor(value, world.resource::<Typography>(), 0, 128);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(title);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            bevy::a11y::AccessibilityNode::from(accessibility),
            Editor {
                title: title.into(),
                secret: false,
            },
            bundle,
        ))
        .id()
}

pub(super) fn draw(world: &mut World, entity: Entity) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    controls::clear(world, entity);
    panel.fields.clear();
    label(world, entity, "Record location");
    label(
        world,
        entity,
        "Live position starts private. Named recipients and Organ visibility control sharing.",
    );
    panel.status_text = Some(label(world, entity, &panel.notice));
    if panel.observer {
        observer::draw(world, entity, &mut panel);
    } else {
        saved_place::draw(world, entity, &mut panel);
        controller::draw(world, entity, &mut panel);
    }
    button(
        world,
        entity,
        entity,
        if panel.observer {
            "View shared location"
        } else {
            "Refresh location"
        },
        Intent::Refresh,
    );
    button(
        world,
        entity,
        entity,
        "Authenticate this device with the sharing Organ",
        Intent::LoginForm,
    );
    if panel.show_login {
        label(
            world,
            entity,
            "Use the Person's login at the location authority. Credentials stay in this login flow.",
        );
        panel.fields.push((
            Field::Username,
            input(world, entity, "Username", &panel.username),
        ));
        let password = input(world, entity, "Password", &panel.password);
        world
            .get_mut::<Editor>(password)
            .expect("password editor")
            .secret = true;
        world
            .entity_mut(password)
            .remove::<crate::style::TextToken>()
            .insert(TextColor(Color::NONE));
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(password) {
            node.set_role(accesskit::Role::PasswordInput);
            node.clear_value();
        }
        let font = world.resource::<Typography>().text(16.0);
        world.spawn((
            ChildOf(password),
            PasswordMask(password),
            Text::new(""),
            font,
            crate::style::text(crate::tokens::Token::Ink),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(4),
                top: px(4),
                ..default()
            },
        ));
        panel.fields.push((Field::Password, password));
        button(
            world,
            entity,
            entity,
            "Authenticate device",
            Intent::Authenticate,
        );
    }
    if !panel.credits.is_empty() {
        button(
            world,
            entity,
            entity,
            "Location credits and licenses",
            Intent::Credits,
        );
        if panel.show_credits {
            for (name, license) in &panel.credits {
                label(world, entity, name);
                label(world, entity, license);
            }
        }
    }
    world.entity_mut(entity).insert(panel);
}

pub(super) fn masks(
    inputs: Query<&bevy::text::EditableText>,
    mut masks: Query<(&PasswordMask, &mut Text)>,
    mut editors: Query<(Entity, &Editor, &mut bevy::a11y::AccessibilityNode)>,
) {
    for (entity, editor, mut node) in &mut editors {
        if editor.secret {
            node.set_role(accesskit::Role::PasswordInput);
            node.clear_value();
        } else if let Ok(input) = inputs.get(entity) {
            node.set_value(input.value().to_string());
        }
    }
    for (mask, text) in &mut masks {
        if let Ok(input) = inputs.get(mask.0) {
            controls::password_mask(text, input.value().chars().count());
        }
    }
}

pub(super) fn position_text(view: Option<&(View, Stamp)>) -> String {
    let Some((view, received)) = view else {
        return "Location stopped".into();
    };
    if view
        .remaining_millis
        .is_some_and(|remaining| remaining <= received.elapsed_millis())
    {
        return "Location expired".into();
    }
    let age = view
        .age_ms
        .unwrap_or_default()
        .saturating_add(received.elapsed_millis());
    let ending = view.remaining_millis.map_or(String::new(), |remaining| {
        format!(
            " · {} min left",
            remaining
                .saturating_sub(received.elapsed_millis())
                .saturating_add(59_999)
                / 60_000
        )
    });
    if age >= nucleus::location::MAX_FIX_AGE_MS as u64 && view.fix.is_some() {
        return "Location unavailable · last fix expired".into();
    }
    if let Some(fix) = &view.fix {
        if let Some(radius) = view.radius_metres {
            return format!(
                "Approximate area · radius {radius} m · {} s old{ending}",
                age / 1000
            );
        }
        let accuracy = fix
            .accuracy_metres
            .map_or(String::new(), |accuracy| format!(" · ±{accuracy:.0} m"));
        return format!(
            "{:.6}, {:.6}{accuracy} · {} s old{}{ending}",
            fix.latitude,
            fix.longitude,
            age / 1000,
            if age > 15_000 { " · stale" } else { "" }
        );
    }
    match view.status {
        Status::Stopped => "Location stopped",
        Status::AwaitingApproval => "Waiting for approval on the selected source device",
        Status::Unavailable => "Device location unavailable · check location permission",
        _ => "Waiting for a fresh location fix",
    }
    .into()
}
