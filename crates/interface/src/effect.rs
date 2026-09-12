use crate::{
    container::BoxRoot,
    sand::{InBox, Square},
};
use bevy::{input_focus::InputFocus, prelude::*, ui_widgets::Activate};

#[derive(Component, Default)]
#[require(Node)]
pub struct HoverEvents;

#[derive(EntityEvent, Debug)]
pub struct SandHoveredOn {
    pub entity: Entity,
}

#[derive(EntityEvent, Debug)]
pub struct SandHoveredOff {
    pub entity: Entity,
}

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct SendBoxEvent {
    #[entities]
    pub box_entity: Entity,
    #[entities]
    pub square: Entity,
}

#[derive(EntityEvent, Debug)]
pub struct ToggleSquare {
    pub entity: Entity,
    pub source: Entity,
    pub square: Entity,
}

#[derive(EntityEvent, Debug)]
pub struct SquareToggled {
    pub entity: Entity,
    pub square: Entity,
    pub showing: bool,
}

pub struct EffectPlugin;

impl Plugin for EffectPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SendBoxEvent>()
            .add_plugins(crate::time_limit::TimeLimitPlugin)
            .init_resource::<InputFocus>()
            .add_observer(
                |event: On<Pointer<Enter>>,
                 sources: Query<(), With<HoverEvents>>,
                 mut commands: Commands| {
                    if sources.contains(event.entity) {
                        commands.trigger(SandHoveredOn {
                            entity: event.entity,
                        });
                    }
                },
            )
            .add_observer(
                |event: On<Pointer<Leave>>,
                 sources: Query<(), With<HoverEvents>>,
                 mut commands: Commands| {
                    if sources.contains(event.entity) {
                        commands.trigger(SandHoveredOff {
                            entity: event.entity,
                        });
                    }
                },
            )
            .add_observer(send_box_event)
            .add_observer(toggle_square);
    }
}

fn send_box_event(
    activate: On<Activate>,
    effects: Query<&SendBoxEvent, With<Square>>,
    mut commands: Commands,
) {
    if let Ok(effect) = effects.get(activate.entity) {
        commands.trigger(ToggleSquare {
            entity: effect.box_entity,
            source: activate.entity,
            square: effect.square,
        });
    }
}

fn toggle_square(
    event: On<ToggleSquare>,
    boxes: Query<&BoxRoot>,
    sources: Query<(&InBox, &SendBoxEvent), With<Square>>,
    mut squares: Query<(&InBox, &mut Visibility), With<Square>>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    mut focus: ResMut<InputFocus>,
) {
    let Ok((owner, effect)) = sources.get(event.source) else {
        return;
    };
    if owner.0 != event.entity || effect.box_entity != event.entity || effect.square != event.square
    {
        return;
    }
    let Ok(_) = boxes.get(event.entity) else {
        return;
    };
    let Ok((owner, mut visibility)) = squares.get_mut(event.square) else {
        return;
    };
    if owner.0 != event.entity {
        return;
    }

    let showing = *visibility == Visibility::Hidden;
    *visibility = if showing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !showing {
        let mut focused = focus.get();
        while let Some(entity) = focused {
            if entity == event.square {
                focus.clear();
                break;
            }
            focused = parents.get(entity).ok().map(ChildOf::parent);
        }
    }
    commands.trigger(SquareToggled {
        entity: event.entity,
        square: event.square,
        showing,
    });
}

pub(crate) mod tests {
    use super::*;
    use crate::{sand::SandPlugin, theme::idle_settings};
    use bevy::{text::EditableText, winit::UpdateMode};
    use std::time::Duration;

    #[derive(Component, Default)]
    struct TestEvents(u64);
    use bevy::{
        camera::RenderTarget,
        input::{
            ButtonState, InputPlugin,
            keyboard::{Key, KeyboardInput},
        },
        input_focus::{FocusCause, InputDispatchPlugin, InputFocusPlugin},
        picking::{
            backend::HitData,
            events::{Click, Pointer, Press},
            pointer::{Location, PointerButton, PointerId},
        },
        ui_widgets::{Button as WidgetButton, ButtonPlugin},
        window::PrimaryWindow,
    };

    fn fixture() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins((SandPlugin, EffectPlugin));
        app.add_observer(
            |event: On<SquareToggled>, mut counts: Query<&mut TestEvents>| {
                counts.get_mut(event.entity).unwrap().0 += 1;
            },
        );
        let workspace = app.world_mut().spawn((BoxRoot, TestEvents::default())).id();
        let reply = app
            .world_mut()
            .spawn((Square, InBox(workspace), Visibility::Inherited))
            .id();
        let trigger = app
            .world_mut()
            .spawn((
                Square,
                InBox(workspace),
                SendBoxEvent {
                    box_entity: workspace,
                    square: reply,
                },
            ))
            .id();
        (app, workspace, trigger, reply)
    }

    fn activate(app: &mut App, entity: Entity) {
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
    }

    #[cfg_attr(test, test)]
    fn activation_sends_one_box_event_and_toggles_without_replacing_the_entity() {
        let (mut app, workspace, trigger, reply) = fixture();
        let label = app
            .world_mut()
            .spawn((Text::new("my text"), ChildOf(reply)))
            .id();
        for (count, expected) in [(1, Visibility::Hidden), (2, Visibility::Inherited)] {
            activate(&mut app, trigger);
            assert_eq!(*app.world().get::<Visibility>(reply).unwrap(), expected);
            assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, count);
            assert_eq!(app.world().get::<Text>(label).unwrap().0, "my text");
        }
    }

    #[cfg_attr(test, test)]
    fn removed_effect_and_missing_target_are_inert() {
        let (mut app, workspace, trigger, reply) = fixture();
        let effect = *app.world().get::<SendBoxEvent>(trigger).unwrap();
        app.world_mut().entity_mut(trigger).remove::<SendBoxEvent>();
        activate(&mut app, trigger);
        app.world_mut().entity_mut(trigger).insert(effect);
        app.world_mut().despawn(reply);
        activate(&mut app, trigger);
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 0);
    }

    #[cfg_attr(test, test)]
    fn event_cannot_toggle_a_square_owned_by_another_box() {
        let (mut app, workspace, trigger, reply) = fixture();
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(reply).insert(InBox(other));
        activate(&mut app, trigger);
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 0);
    }

    #[cfg_attr(test, test)]
    fn quiet_updates_do_not_touch_square_visibility_or_send_events() {
        let (mut app, workspace, _, reply) = fixture();
        app.world_mut().clear_trackers();
        for _ in 0..100 {
            app.update();
        }
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 0);
        assert!(
            !app.world()
                .entity(reply)
                .get_ref::<Visibility>()
                .unwrap()
                .is_changed()
        );
        assert_eq!(
            idle_settings().focused_mode,
            UpdateMode::reactive_low_power(Duration::MAX)
        );
    }

    #[cfg_attr(test, test)]
    fn clicking_a_text_child_uses_bevys_button_and_emits_only_once() {
        let (mut app, workspace, trigger, reply) = fixture();
        app.add_plugins(ButtonPlugin);
        app.world_mut().entity_mut(trigger).insert(WidgetButton);
        let label = app
            .world_mut()
            .spawn((Text::new("click me"), ChildOf(trigger)))
            .id();
        let window = app.world_mut().spawn(Window::default()).id();
        let location = Location {
            target: RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                .normalize(None)
                .unwrap(),
            position: Vec2::ZERO,
        };
        let hit = HitData::new(window, 0.0, None, None);
        app.world_mut().flush();
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location.clone(),
            Press {
                button: PointerButton::Primary,
                hit: hit.clone(),
                count: 1,
            },
            label,
        ));
        app.world_mut().flush();
        assert!(app.world().get::<bevy::ui::Pressed>(trigger).is_some());
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location,
            Click {
                button: PointerButton::Primary,
                hit,
                duration: Duration::from_millis(50),
                count: 1,
            },
            label,
        ));
        app.world_mut().flush();
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 1);
    }

    #[cfg_attr(test, test)]
    fn keyboard_activation_uses_focused_bevy_button_and_ignores_repeat() {
        let (mut app, workspace, trigger, reply) = fixture();
        app.add_plugins((
            InputPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
            ButtonPlugin,
        ));
        app.world_mut().entity_mut(trigger).insert(WidgetButton);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(trigger, FocusCause::Navigated);
        for repeat in [false, true] {
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: ButtonState::Pressed,
                text: None,
                repeat,
                window,
            });
            app.update();
        }
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 1);
    }

    #[cfg_attr(test, test)]
    fn hiding_an_editor_releases_focus_but_keeps_its_text() {
        let (mut app, _, trigger, reply) = fixture();
        let editor = app
            .world_mut()
            .spawn((EditableText::new("preserve me"), ChildOf(reply)))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, FocusCause::Pressed);
        activate(&mut app, trigger);
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "preserve me"
        );
    }

    #[cfg_attr(test, test)]
    fn one_event_leaves_ten_thousand_unrelated_squares_unchanged() {
        let (mut app, workspace, trigger, _) = fixture();
        let unrelated = (0..10_000)
            .map(|_| {
                app.world_mut()
                    .spawn((Square, InBox(workspace), Visibility::Inherited))
                    .id()
            })
            .collect::<Vec<_>>();
        app.world_mut().clear_trackers();
        activate(&mut app, trigger);
        assert_eq!(app.world().get::<TestEvents>(workspace).unwrap().0, 1);
        assert!(unrelated.into_iter().all(|entity| {
            !app.world()
                .entity(entity)
                .get_ref::<Visibility>()
                .unwrap()
                .is_changed()
        }));
    }

    #[cfg_attr(test, test)]
    fn pointer_enter_and_leave_emit_sand_events_only_for_enabled_sources() {
        #[derive(Resource, Default)]
        struct Events(Vec<(Entity, bool)>);
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(EffectPlugin).init_resource::<Events>();
        app.add_observer(|event: On<SandHoveredOn>, mut events: ResMut<Events>| {
            events.0.push((event.entity, true));
        });
        app.add_observer(|event: On<SandHoveredOff>, mut events: ResMut<Events>| {
            events.0.push((event.entity, false));
        });
        let source = app.world_mut().spawn(HoverEvents).id();
        let unrelated = app.world_mut().spawn(Square).id();
        let window = app.world_mut().spawn(Window::default()).id();
        let location = Location {
            target: RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                .normalize(None)
                .unwrap(),
            position: Vec2::ZERO,
        };
        let hit = HitData::new(window, 0.0, None, None);
        for entity in [source, unrelated] {
            app.world_mut().trigger(Pointer::new(
                PointerId::Mouse,
                location.clone(),
                Enter {
                    hit: hit.clone(),
                    is_in_bounds: true,
                },
                entity,
            ));
            app.world_mut().flush();
            app.world_mut().trigger(Pointer::new(
                PointerId::Mouse,
                location.clone(),
                Leave {
                    hit: hit.clone(),
                    was_in_bounds: true,
                },
                entity,
            ));
            app.world_mut().flush();
        }
        assert_eq!(
            app.world().resource::<Events>().0,
            [(source, true), (source, false)]
        );
    }

    crate::laboratory_cases! {
        activation_sends_one_box_event_and_toggles_without_replacing_the_entity,
        removed_effect_and_missing_target_are_inert,
        event_cannot_toggle_a_square_owned_by_another_box,
        quiet_updates_do_not_touch_square_visibility_or_send_events,
        clicking_a_text_child_uses_bevys_button_and_emits_only_once,
        keyboard_activation_uses_focused_bevy_button_and_ignores_repeat,
        hiding_an_editor_releases_focus_but_keeps_its_text,
        one_event_leaves_ten_thousand_unrelated_squares_unchanged,
        pointer_enter_and_leave_emit_sand_events_only_for_enabled_sources,
    }
}
