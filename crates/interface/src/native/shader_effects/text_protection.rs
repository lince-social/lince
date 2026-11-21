// non-lean-yet
// Scope: struct ProtectedText, struct Scratch, fn refresh, fn refresh_current.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (24, 92).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 97, 98.

use super::{Background, Surface, disabled, shader};
use crate::native::{token_style::Tokens, tokens::Token, topology::appearance::Material};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow, system::InRef},
    prelude::*,
    text::{EditableText, TextBackgroundColor},
};
use std::collections::HashSet;

#[derive(Component)]
struct ProtectedText;

#[derive(Default)]
struct Scratch {
    visited: HashSet<Entity>,
    pending: Vec<Entity>,
    targets: Vec<(Entity, bool)>,
    active: Vec<Entity>,
}

pub(crate) fn refresh(world: &mut World, owners: &[Entity]) {
    world
        .run_system_cached_with(refresh_current, owners)
        .expect("refresh current effect text protection");
}

fn refresh_current(InRef(owners): InRef<[Entity]>, world: &mut World, mut scratch: Local<Scratch>) {
    let _protection = nucleus::performance::Span::new("effect-text-protection-preparation");
    if owners.is_empty() {
        *scratch = Scratch::default();
        return;
    }
    let Scratch {
        visited,
        pending,
        targets,
        active,
    } = &mut *scratch;
    visited.clear();
    pending.clear();
    targets.clear();
    active.clear();
    for &owner in owners.iter().rev() {
        if visited.contains(&owner) {
            continue;
        }
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
            if !visited.insert(entity) {
                continue;
            }
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            if world.get::<Text>(entity).is_some() || world.get::<EditableText>(entity).is_some() {
                targets.push((entity, enabled));
            }
        }
    }
    nucleus::performance::add("effect-text-protection-visits", visited.len() as u64);
    for (entity, enabled) in targets.drain(..) {
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
            .run_system_cached_with(style, active.as_slice())
            .expect("style protected text");
    }
}

fn style(
    InRef(entities): InRef<[Entity]>,
    mut tokens: Tokens,
    mut texts: Query<(Has<ProtectedText>, Option<&mut TextBackgroundColor>), Allow<Disabled>>,
    mut commands: Commands,
    mut pending: Local<Vec<Entity>>,
) {
    let _style = nucleus::performance::Span::new("effect-text-protection-style");
    tokens.refresh();
    pending.clear();
    for &entity in entities {
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
    if pending.is_empty() {
        *pending = Vec::new();
        return;
    }
    for entity in pending.drain(..) {
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
