// non-lean-yet
// Scope: mod presentation, const SAVE_WARNING_AFTER, struct SaveWake, struct SaveFeedback.
// BeautifulRust (baseline lines):
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 28, 35, 92, 93.

use crate::host::performance::{self, Count, Stage};
use crate::native::{
    sand::Unsaved,
    token_style::{BorderToken, Tokens},
    tokens::Token,
};
use bevy::ecs::system::InRef;
use bevy::{prelude::*, text::EditableText};
use std::time::{Duration, Instant};

mod presentation;

const SAVE_WARNING_AFTER: Duration = Duration::from_secs(10);

#[derive(Resource, Default)]
struct SaveWake(Option<Instant>);

#[derive(Component, Clone, Copy, Default, PartialEq)]
pub(crate) struct SaveFeedback {
    since: Option<Instant>,
    failed: bool,
    border: Option<UiRect>,
    styled_border: Option<UiRect>,
    pub(crate) color_owned: bool,
}

impl SaveFeedback {
    pub(crate) fn style_border(feedback: &mut Mut<Self>, border: UiRect) {
        if feedback.styled_border != Some(border) {
            feedback.bypass_change_detection().styled_border = Some(border);
            performance::count(Count::SaveFeedbackBordersStaged);
        }
    }

    pub(crate) fn set_failed(feedback: &mut Mut<Self>, failed: bool) {
        if feedback.failed != failed {
            feedback.failed = failed;
            performance::count(Count::SaveFeedbackStateWrites);
        }
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

#[cfg(feature = "behavior-checks")]
pub(crate) fn warning(world: &World, entity: Entity) -> bool {
    world.get::<SaveFeedback>(entity).is_some_and(|feedback| {
        !feedback.failed
            && feedback
                .since
                .is_some_and(|since| since.elapsed() >= SAVE_WARNING_AFTER)
    })
}

pub(crate) fn set_failed(world: &mut World, entity: Entity, failed: bool) {
    let mut descendants = vec![entity];
    while let Some(entity) = descendants.pop() {
        if let Some(children) = world.get::<Children>(entity) {
            descendants.extend(children.iter());
        }
        if let Some(mut feedback) = world.get_mut::<SaveFeedback>(entity) {
            SaveFeedback::set_failed(&mut feedback, failed);
        }
    }
}

pub(crate) fn borders(world: &mut World) {
    borders_at(world, Instant::now());
}

fn borders_at(world: &mut World, now: Instant) {
    presentation::borders(world, now);
}

fn style_borders(
    InRef(fields): InRef<[(Entity, Token)]>,
    mut tokens: Tokens,
    mut colors: Query<&mut BorderColor>,
    mut commands: Commands,
) {
    let _style = performance::Span::new(Stage::SaveFeedbackStyle);
    tokens.refresh();
    for &(entity, token) in fields {
        tokens.prepare(entity, [token]);
    }
    tokens.finish_preparing();
    for &(entity, token) in fields {
        let color = BorderColor::all(tokens.resolve(entity, token).0.color());
        if let Ok(mut current) = colors.get_mut(entity) {
            current.set_if_neq(color);
        } else {
            commands.entity(entity).insert(color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tokens::{ThemeSettings, TokenOverrides, TokenValue};

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
        feedback.failed = true;
        assert_eq!(
            feedback.update(true, false, now + SAVE_WARNING_AFTER),
            (Some(Token::Error), false)
        );
        feedback.failed = false;
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
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(crate::native::sand::SandPlugin);
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::ControlBorder, TokenValue::Number(0.0));
        let field = app
            .world_mut()
            .spawn((
                crate::native::sand::editable("Draft"),
                Node::default(),
                Unsaved(true),
            ))
            .id();
        let toggle = app
            .world_mut()
            .spawn((
                crate::native::sand::button(0),
                Node::default(),
                crate::native::sand::Borderless,
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
        world.insert_resource(crate::native::wake::WakeSignal::new(|| {}));
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
