use crate::{
    sand::Unsaved,
    token_style::{BorderToken, resolve},
    tokens::Token,
};
use bevy::{prelude::*, text::EditableText};
use std::time::{Duration, Instant};

const SAVE_WARNING_AFTER: Duration = Duration::from_secs(10);

#[derive(Resource, Default)]
struct SaveWake(Option<Instant>);

#[derive(Component, Default)]
pub(crate) struct SaveFeedback {
    since: Option<Instant>,
    failed: bool,
    border: Option<UiRect>,
}

impl SaveFeedback {
    pub(crate) fn set_failed(&mut self, failed: bool) {
        self.failed = failed;
    }

    fn update(&mut self, dirty: bool, failed: bool, now: Instant) -> (Option<Token>, bool) {
        if !dirty {
            self.since = None;
            self.failed = false;
            return (None, false);
        }
        let started = self.since.is_none();
        let since = *self.since.get_or_insert(now);
        let token = if failed || self.failed {
            Some(Token::Error)
        } else if now.saturating_duration_since(since) >= SAVE_WARNING_AFTER {
            Some(Token::Warning)
        } else {
            None
        };
        (token, started)
    }
}

pub(crate) fn set_failed(world: &mut World, entity: Entity, failed: bool) {
    let mut descendants = vec![entity];
    while let Some(entity) = descendants.pop() {
        if let Some(children) = world.get::<Children>(entity) {
            descendants.extend(children.iter());
        }
        if let Some(mut feedback) = world.get_mut::<SaveFeedback>(entity) {
            feedback.set_failed(failed);
        }
    }
}

pub(crate) fn borders(world: &mut World) {
    borders_at(world, Instant::now());
}

fn borders_at(world: &mut World, now: Instant) {
    let fields: Vec<_> = world
        .query::<(
            Entity,
            &Unsaved,
            Option<&BorderToken>,
            Option<&crate::record_binding::TextBinding>,
            Option<&crate::record_view::RecordEditor>,
            Option<&EditableText>,
        )>()
        .iter(world)
        .map(|(entity, unsaved, border, binding, record, text)| {
            let value = text
                .filter(|_| binding.is_some() || record.is_some())
                .map(|text| text.value().to_string());
            let dirty = match (binding, record, value.as_deref()) {
                (Some(binding), _, Some(value)) => binding.unsaved(value),
                (_, Some(record), Some(value)) => {
                    record.pending.is_some() || value != record.confirmed
                }
                _ => unsaved.0,
            };
            (
                entity,
                dirty,
                binding
                    .is_some_and(|binding| crate::record_binding::failed(world, &binding.record)),
                border.map_or(Token::Accent, |border| border.0),
                text.is_some(),
            )
        })
        .collect();
    let mut deadline = None;
    for (entity, dirty, failed, base_token, editable) in fields {
        let mut feedback = world.get_mut::<SaveFeedback>(entity).unwrap();
        let (alert, _) = feedback.update(dirty, failed, now);
        if dirty && alert.is_none() {
            let at = feedback.since.unwrap() + SAVE_WARNING_AFTER;
            deadline = Some(deadline.map_or(at, |previous: Instant| previous.min(at)));
        }
        let token = alert.unwrap_or(base_token);
        let color = BorderColor::all(resolve(world, entity, token).0.color());
        if alert.is_some() {
            let border = world.get::<Node>(entity).map(|node| node.border);
            let mut feedback = world.get_mut::<SaveFeedback>(entity).unwrap();
            if feedback.border.is_none() {
                feedback.border = border;
            }
            if let Some(mut node) = world.get_mut::<Node>(entity) {
                let border = &mut node.border;
                for side in [
                    &mut border.left,
                    &mut border.right,
                    &mut border.top,
                    &mut border.bottom,
                ] {
                    if matches!(*side, Val::Auto | Val::Px(0.0) | Val::Percent(0.0)) {
                        *side = px(1);
                    }
                }
            }
        } else {
            let border = world.get_mut::<SaveFeedback>(entity).unwrap().border.take();
            if !editable
                && let Some(border) = border
                && let Some(mut node) = world.get_mut::<Node>(entity)
            {
                node.border = border;
            }
        }
        if world.get::<BorderColor>(entity) != Some(&color) {
            world.entity_mut(entity).insert(color);
        }
    }
    world.init_resource::<SaveWake>();
    let scheduled = world.resource::<SaveWake>().0.filter(|at| *at > now);
    world.resource_mut::<SaveWake>().0 = scheduled;
    if let Some(deadline) = deadline
        && scheduled.is_none_or(|scheduled| deadline < scheduled)
        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
    {
        world.resource_mut::<SaveWake>().0 = Some(deadline);
        wake.after(deadline.saturating_duration_since(now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{ThemeSettings, TokenOverrides, TokenValue};

    #[test]
    fn typing_keeps_the_first_unsaved_deadline_and_failure_is_immediate() {
        let now = Instant::now();
        let mut feedback = SaveFeedback::default();
        assert_eq!(feedback.update(true, false, now), (None, true));
        for seconds in 1..10 {
            assert_eq!(
                feedback.update(true, false, now + Duration::from_secs(seconds)),
                (None, false)
            );
        }
        assert_eq!(
            feedback.update(true, false, now + SAVE_WARNING_AFTER),
            (Some(Token::Warning), false)
        );
        feedback.set_failed(true);
        assert_eq!(
            feedback.update(true, false, now + SAVE_WARNING_AFTER),
            (Some(Token::Error), false)
        );
        feedback.set_failed(false);
        assert_eq!(
            feedback.update(true, false, now + SAVE_WARNING_AFTER),
            (Some(Token::Warning), false)
        );
        assert_eq!(
            feedback.update(false, true, now + SAVE_WARNING_AFTER),
            (None, false)
        );
        let later = now + Duration::from_secs(20);
        assert_eq!(feedback.update(true, false, later), (None, true));
        assert_eq!(
            feedback.update(true, true, later),
            (Some(Token::Error), false)
        );
    }

    #[test]
    fn alerts_follow_inherited_tokens_preserve_focus_and_restore_borderless_fields() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        let mut tokens = TokenOverrides::default();
        tokens.set(Token::Warning, TokenValue::Color([200, 150, 20, 255]));
        tokens.set(Token::Error, TokenValue::Color([200, 30, 40, 255]));
        tokens.set(Token::Accent, TokenValue::Color([30, 50, 200, 255]));
        let parent = world.spawn(tokens).id();
        let focus = Outline {
            width: px(2),
            offset: px(4),
            color: Color::WHITE,
        };
        let field = world
            .spawn((Unsaved(true), Node::default(), focus, ChildOf(parent)))
            .id();
        let untouched = world
            .spawn((Unsaved(false), Node::default(), ChildOf(parent)))
            .id();
        let now = Instant::now();
        borders_at(&mut world, now);
        assert_eq!(world.get::<Node>(field).unwrap().border, UiRect::ZERO);
        assert_eq!(
            world.get::<BorderColor>(field).unwrap().top,
            Color::srgb_u8(30, 50, 200)
        );
        borders_at(&mut world, now + SAVE_WARNING_AFTER);
        assert_eq!(world.get::<Node>(field).unwrap().border, UiRect::all(px(1)));
        assert_eq!(
            world.get::<BorderColor>(field).unwrap().top,
            Color::srgb_u8(200, 150, 20)
        );
        set_failed(&mut world, parent, true);
        borders_at(&mut world, now + SAVE_WARNING_AFTER);
        assert_eq!(
            world.get::<BorderColor>(field).unwrap().top,
            Color::srgb_u8(200, 30, 40)
        );
        assert_eq!(
            world.get::<BorderColor>(untouched).unwrap().top,
            Color::srgb_u8(30, 50, 200)
        );
        assert_eq!(world.get::<Outline>(field).unwrap(), &focus);
        world.get_mut::<Unsaved>(field).unwrap().0 = false;
        borders_at(&mut world, now + SAVE_WARNING_AFTER);
        assert_eq!(world.get::<Node>(field).unwrap().border, UiRect::ZERO);
        assert_eq!(
            world.get::<BorderColor>(field).unwrap().top,
            Color::srgb_u8(30, 50, 200)
        );
    }

    #[test]
    fn editable_alerts_restore_the_current_theme_border_after_saving() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(crate::sand::SandPlugin);
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::ControlBorder, TokenValue::Number(0.0));
        let field = app
            .world_mut()
            .spawn((
                crate::sand::editable("Draft"),
                Node::default(),
                Unsaved(true),
            ))
            .id();
        let toggle = app
            .world_mut()
            .spawn((
                crate::sand::button(0),
                Node::default(),
                crate::sand::Borderless,
            ))
            .id();
        app.update();
        assert_eq!(app.world().get::<Node>(field).unwrap().border, UiRect::ZERO);
        set_failed(app.world_mut(), field, true);
        app.update();
        assert_eq!(
            app.world().get::<Node>(field).unwrap().border,
            UiRect::all(px(1))
        );
        assert_eq!(
            app.world().get::<BorderColor>(field).unwrap().top,
            Token::Error.default_value(Default::default()).color()
        );
        assert_eq!(
            app.world().get::<Node>(toggle).unwrap().border,
            UiRect::ZERO
        );
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::ControlBorder, TokenValue::Number(3.0));
        app.world_mut().get_mut::<Unsaved>(field).unwrap().0 = false;
        app.update();
        assert_eq!(
            app.world().get::<Node>(field).unwrap().border,
            UiRect::all(px(3))
        );
        assert_eq!(
            app.world().get::<BorderColor>(field).unwrap().top,
            Token::Accent.default_value(Default::default()).color()
        );
    }

    #[test]
    fn pending_save_wake_is_shared_across_fields_and_quick_saves() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        world.insert_resource(crate::wake::WakeSignal::new(|| {}));
        let field = world.spawn(Unsaved(true)).id();
        let now = Instant::now();
        borders_at(&mut world, now);
        assert_eq!(
            world.resource::<SaveWake>().0,
            Some(now + SAVE_WARNING_AFTER)
        );
        world.get_mut::<Unsaved>(field).unwrap().0 = false;
        borders_at(&mut world, now + Duration::from_secs(1));
        let another = world.spawn(Unsaved(true)).id();
        borders_at(&mut world, now + Duration::from_secs(2));
        assert_eq!(
            world.resource::<SaveWake>().0,
            Some(now + SAVE_WARNING_AFTER)
        );
        borders_at(&mut world, now + SAVE_WARNING_AFTER);
        assert_eq!(
            world.resource::<SaveWake>().0,
            Some(now + Duration::from_secs(12))
        );
        borders_at(&mut world, now + Duration::from_secs(12));
        assert_eq!(world.resource::<SaveWake>().0, None);
        assert_eq!(
            world.get::<BorderColor>(another).unwrap().top,
            Token::Warning.default_value(Default::default()).color()
        );
    }
}
