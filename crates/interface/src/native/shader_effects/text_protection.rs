use super::{Background, Surface, disabled, shader};
use crate::native::{token_style::Tokens, tokens::Token, topology::appearance::Material};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    text::{EditableText, TextBackgroundColor},
};
use std::collections::HashMap;

#[derive(Component)]
struct ProtectedText;

pub(crate) fn refresh(world: &mut World, owners: impl IntoIterator<Item = Entity>) {
    let mut targets = HashMap::new();
    let mut pending = Vec::new();
    for owner in owners {
        let enabled = world
            .get::<Material>(owner)
            .is_some_and(|material| material.0.valid())
            || (!disabled(world)
                && world
                    .get::<Surface>(owner)
                    .is_some_and(|surface| !surface.disabled)
                && world
                    .get::<Background>(owner)
                    .and_then(|background| world.get::<shader::EffectPreview>(background.preview))
                    .is_some_and(|preview| !preview.disabled));
        pending.push(owner);
        while let Some(entity) = pending.pop() {
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            if world.get::<Text>(entity).is_some() || world.get::<EditableText>(entity).is_some() {
                targets.insert(entity, enabled);
            }
        }
    }
    let mut active = Vec::new();
    for (entity, enabled) in targets {
        let protected = world.get::<ProtectedText>(entity).is_some();
        if !enabled {
            if protected {
                world
                    .entity_mut(entity)
                    .remove::<(TextBackgroundColor, ProtectedText)>();
            }
        } else if protected
            || world
                .get::<TextBackgroundColor>(entity)
                .is_none_or(|background| background.0 == Color::NONE)
        {
            active.push(entity);
        }
    }
    if !active.is_empty() {
        world.init_resource::<crate::native::tokens::ThemeSettings>();
        world
            .run_system_cached_with(style, active)
            .expect("style protected text");
    }
}

fn style(
    In(entities): In<Vec<Entity>>,
    mut tokens: Tokens,
    mut texts: Query<(Has<ProtectedText>, Option<&mut TextBackgroundColor>), Allow<Disabled>>,
    mut commands: Commands,
) {
    tokens.refresh();
    let mut pending = Vec::new();
    for entity in entities {
        let Ok((protected, background)) = texts.get_mut(entity) else {
            continue;
        };
        if protected
            && !tokens.invalidated(entity)
            && background
                .as_ref()
                .is_some_and(|background| !background.is_changed())
        {
            continue;
        }
        tokens.prepare(entity, [Token::Surface]);
        pending.push(entity);
    }
    tokens.finish_preparing();
    for entity in pending {
        let Ok((protected, background)) = texts.get_mut(entity) else {
            continue;
        };
        let color = tokens
            .resolve(entity, Token::Surface)
            .0
            .color()
            .with_alpha(1.0);
        if let Some(mut background) = background {
            if background.0 != color {
                background.0 = color;
            }
        } else {
            commands.entity(entity).insert(TextBackgroundColor(color));
        }
        if !protected {
            commands.entity(entity).insert(ProtectedText);
        }
    }
}
