use super::*;
use std::collections::BTreeSet;

#[derive(Component, Default)]
struct Expansion(BTreeSet<String>);

#[derive(Component)]
pub(super) struct Accordion {
    root: Entity,
    key: String,
    pub body: Entity,
    indicator: Entity,
}

#[derive(Component)]
struct Section;

#[derive(Component)]
struct HiddenTabIndex(i32);

pub(crate) fn synchronize_keyboard(world: &mut World) {
    use bevy::input_focus::tab_navigation::TabIndex;
    if world.query_filtered::<(), (With<Section>, bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>)>().iter(world).next().is_none() { return; }
    let candidates = world.query::<(Entity, &TabIndex, Option<&HiddenTabIndex>)>().iter(world)
        .map(|(entity, index, previous)| (entity, index.0, previous.map(|previous| previous.0))).collect::<Vec<_>>();
    for (entity, index, previous) in candidates {
        let mut current = Some(entity);
        let mut section = false;
        let mut hidden = false;
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(ancestor) = current else { break; };
            section |= world.get::<Section>(ancestor).is_some();
            hidden |= world.get::<Node>(ancestor).is_some_and(|node| node.display == Display::None);
            current = world.get::<ChildOf>(ancestor).map(ChildOf::parent);
        }
        if !section { continue; }
        if hidden && index >= 0 {
            world.entity_mut(entity).insert((HiddenTabIndex(index), TabIndex(-1)));
        } else if !hidden && let Some(previous) = previous {
            world.entity_mut(entity).insert(TabIndex(previous)).remove::<HiddenTabIndex>();
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpenPark;

impl Action for OpenPark {
    fn apply(&self, world: &mut World, root: Entity) {
        world.entity_mut(root).entry::<Expansion>().or_default();
        world.get_mut::<Expansion>(root).unwrap().0.insert("park".into());
        crate::native::edit_mode::show_customization(world, root);
        let tab = world.query::<(Entity, &crate::native::edit_mode::EditControl)>().iter(world)
            .find(|(_, control)| control.root == root && control.action == crate::native::edit_mode::EditAction::Customization)
            .map(|(entity, _)| entity);
        if let Some(tab) = tab {
            world.resource_mut::<bevy::input_focus::InputFocus>().set(tab, bevy::input_focus::FocusCause::Navigated);
        }
    }
}

#[derive(Clone)]
struct Toggle;

impl Action for Toggle {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(section) = world.get::<Accordion>(target) else { return; };
        let (root, key, body, indicator) = (section.root, section.key.clone(), section.body, section.indicator);
        let expanded = world.get::<Node>(body).is_some_and(|node| node.display == Display::None);
        if let Some(mut node) = world.get_mut::<Node>(body) {
            node.display = if expanded { Display::Flex } else { Display::None };
        }
        if let Some(mut caption) = world.get_mut::<Text>(indicator) {
            caption.0 = if expanded { "−" } else { "+" }.into();
        }
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(target) { node.set_expanded(expanded); }
        world.entity_mut(root).entry::<Expansion>().or_default();
        if expanded { world.get_mut::<Expansion>(root).unwrap().0.insert(key); }
        else { world.get_mut::<Expansion>(root).unwrap().0.remove(&key); }
        synchronize_keyboard(world);
        if world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get())
            .is_some_and(|focused| world.get::<HiddenTabIndex>(focused).is_some()) {
            world.resource_mut::<bevy::input_focus::InputFocus>().set(target, bevy::input_focus::FocusCause::Navigated);
        }
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() { wake.ring(); }
    }
}

pub(super) fn accordion(world: &mut World, root: Entity, parent: Entity, key: &str, title: &str) -> Entity {
    world.entity_mut(root).entry::<Expansion>().or_default();
    let expanded = world.get::<Expansion>(root).unwrap().0.contains(key);
    let section = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column,
        row_gap: px(8), flex_shrink: 0.0, ..default() }, Section, Name::new(format!("customization.{key}")), ChildOf(parent))).id();
    let header = world.spawn((crate::native::sand::Square, crate::native::sand::button(0),
        Node { width: percent(100), min_width: px(0), justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center, column_gap: px(8), padding: UiRect::all(px(8)), flex_shrink: 0.0, ..default() },
        crate::native::token_style::background(Token::Surface), AccessibilityNode::default(), ChildOf(section))).id();
    let caption = label(world, header, title, 18.0);
    world.entity_mut(caption).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    {
        let mut node = world.get_mut::<Node>(caption).unwrap();
        node.flex_shrink = 1.0;
        node.flex_grow = 1.0;
        node.min_width = px(0);
    }
    let indicator = label(world, header, if expanded { "−" } else { "+" }, 18.0);
    let body = world.spawn((Node { display: if expanded { Display::Flex } else { Display::None }, width: percent(100), min_width: px(0),
        flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::left(px(8)), flex_shrink: 0.0, ..default() }, ChildOf(section))).id();
    let mut accessibility = world.get_mut::<AccessibilityNode>(header).unwrap();
    accessibility.set_role(accesskit::Role::Button);
    accessibility.set_label(title);
    accessibility.set_expanded(expanded);
    world.entity_mut(header).insert((ActionButton::new(header, crate::native::actions::ActionSequence::default().then(Toggle)),
        Accordion { root, key: key.into(), body, indicator }));
    body
}

pub(super) fn render(world: &mut World, root: Entity, panel: Entity) {
    label(world, panel, "Customization", 22.0);
    let interface = accordion(world, root, panel, "interface", "Workspace, canvas and interface");
    let appearance = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column,
        row_gap: px(8), flex_shrink: 0.0, ..default() }, super::Appearance(root), ChildOf(interface))).id();
    super::render_theme(world, root, appearance);
    let layout = accordion(world, root, interface, "layout", "Window layout and spacing");
    super::layout::panel(world, layout);
    crate::native::shortcuts::width_settings(world, root, layout);
    let workspace = accordion(world, root, interface, "workspace", "Workspace behavior");
    crate::native::workspace_config::controls(world, root, workspace);
    crate::native::layout::desktop::controls(world, root, workspace);
    let controls = accordion(world, root, interface, "controls", "Interface controls and effects");
    crate::native::edit_mode::controls_settings(world, root, controls);
    crate::native::inspection::controls(world, root, controls);
    crate::native::shader_effects::controls(world, root, controls);
    let park = accordion(world, root, panel, "park", "Park desktop");
    let displays = accordion(world, root, park, "displays", "Displays and magnification");
    crate::native::layout::desktop::monitors::panel(world, root, displays);
    crate::native::layout::desktop::magnification::panel(world, root, displays);
    let keyboard = accordion(world, root, park, "keyboard", "Keyboard");
    crate::native::layout::desktop::input_settings::panel(world, keyboard);
    let pointer = accordion(world, root, park, "pointer", "Pointer and touch devices");
    crate::native::layout::desktop::pointer_settings::panel(world, pointer);
    crate::native::layout::desktop::absolute_settings::panel(world, pointer);
    let focus = accordion(world, root, park, "focus", "Focus and application opening");
    crate::native::layout::desktop::focus::settings::panel(world, focus);
    crate::native::layout::desktop::opening::ui::panel(world, root, focus);
    let gestures = accordion(world, root, park, "gestures", "Gestures and overview");
    crate::native::layout::desktop::gestures::panel(world, gestures);
    crate::native::layout::desktop::overview::settings::panel(world, gestures);
    let external = accordion(world, root, park, "external", "External programs and desktop bindings");
    crate::native::layout::desktop::external_settings::panel(world, root, external);
    let programs = accordion(world, root, park, "programs", "Session programs");
    super::programs::panel(world, root, programs);
    synchronize_keyboard(world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_navigation_rechecks_dynamic_sections_without_losing_drafts() {
        use bevy::{
            input::{InputPlugin, ButtonState, keyboard::{Key, KeyboardInput}},
            input_focus::{InputFocus, FocusCause, InputFocusPlugin, InputDispatchPlugin, tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin}},
            window::PrimaryWindow,
        };
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((InputPlugin, InputFocusPlugin, InputDispatchPlugin, TabNavigationPlugin, crate::native::actions::ActionsPlugin))
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>();
        let world = app.world_mut();
        let window = world.spawn((Window::default(), PrimaryWindow)).id();
        let root = world.spawn_empty().id();
        let panel = world.spawn(TabGroup::modal()).id();
        let body = accordion(world, root, panel, "park", "Park");
        let header = world.query::<(Entity, &Accordion)>().iter(world).find(|(_, section)| section.body == body).unwrap().0;
        let dynamic = world.spawn((Node { display: Display::None, ..default() }, ChildOf(body))).id();
        let draft = world.spawn((TabIndex(0), Text::new("Unsaved draft"), ChildOf(dynamic))).id();
        let hidden = world.spawn((Node { display: Display::None, ..default() }, ChildOf(body))).id();
        let hidden_control = world.spawn((TabIndex(0), ChildOf(hidden))).id();
        Toggle.apply(world, header);
        assert_eq!(world.get::<TabIndex>(draft).unwrap().0, -1);
        world.resource_mut::<InputFocus>().set(header, FocusCause::Navigated);
        app.update();
        for visible in [true, false, true] {
            app.world_mut().get_mut::<Node>(dynamic).unwrap().display = if visible { Display::Flex } else { Display::None };
            for state in [ButtonState::Pressed, ButtonState::Released] {
                app.world_mut().write_message(KeyboardInput {
                    key_code: KeyCode::Tab, logical_key: Key::Tab, state, text: None, repeat: false, window,
                });
            }
            app.update();
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(if visible { draft } else { header }));
            assert_eq!(app.world().get::<TabIndex>(draft).unwrap().0, if visible { 0 } else { -1 });
            assert_eq!(app.world().get::<TabIndex>(hidden_control).unwrap().0, -1);
            assert_eq!(app.world().get::<Text>(draft).unwrap().0, "Unsaved draft");
        }
    }

    #[test]
    fn collapsed_sections_leave_tab_order_without_losing_nested_drafts_or_indices() {
        use bevy::input_focus::{InputFocus, FocusCause, tab_navigation::{TabGroup, TabIndex, TabNavigation, NavAction}};
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<InputFocus>();
        let root = world.spawn_empty().id();
        let panel = world.spawn(TabGroup { order: 0, modal: true }).id();
        let body = accordion(&mut world, root, panel, "park", "Park");
        let header = world.query::<(Entity, &Accordion)>().iter(&world).find(|(_, section)| section.body == body).unwrap().0;
        let draft = world.spawn((TabIndex(0), Text::new("Unsaved draft"), ChildOf(body))).id();
        let nested = accordion(&mut world, root, body, "nested", "Nested");
        let nested_header = world.query::<(Entity, &Accordion)>().iter(&world).find(|(_, section)| section.body == nested).unwrap().0;
        let nested_draft = world.spawn((TabIndex(3), ChildOf(nested))).id();
        let next = world.spawn((TabIndex(0), ChildOf(panel))).id();
        synchronize_keyboard(&mut world);
        world.resource_mut::<InputFocus>().set(header, FocusCause::Navigated);
        let mut navigation = bevy::ecs::system::SystemState::<TabNavigation>::new(&mut world);
        assert_eq!(navigation.get(&world).unwrap().navigate(world.resource::<InputFocus>(), NavAction::Next).unwrap(), next);
        Toggle.apply(&mut world, header);
        assert_eq!(world.get::<TabIndex>(draft).unwrap().0, 0);
        assert_eq!(world.get::<TabIndex>(nested_draft).unwrap().0, -1);
        Toggle.apply(&mut world, nested_header);
        assert_eq!(world.get::<TabIndex>(nested_draft).unwrap().0, 3);
        world.resource_mut::<InputFocus>().set(nested_draft, FocusCause::Navigated);
        Toggle.apply(&mut world, header);
        assert_eq!(world.resource::<InputFocus>().get(), Some(header));
        assert_eq!(world.get::<TabIndex>(draft).unwrap().0, -1);
        assert_eq!(world.get::<TabIndex>(nested_draft).unwrap().0, -1);
        Toggle.apply(&mut world, header);
        assert_eq!(world.get::<TabIndex>(nested_draft).unwrap().0, 3);
        assert_eq!(world.get::<Text>(draft).unwrap().0, "Unsaved draft");
    }

    #[test]
    fn sections_toggle_independently_keep_drafts_and_remember_expansion() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world.spawn_empty().id();
        let panel = world.spawn_empty().id();
        let colors = accordion(&mut world, root, panel, "colors", "Colors");
        let spacing = accordion(&mut world, root, panel, "spacing", "Spacing");
        let header = world.query::<(Entity, &Accordion)>().iter(&world).find(|(_, section)| section.body == colors).unwrap().0;
        let draft = world.spawn((Text::new("Unsaved value"), ChildOf(colors))).id();
        assert_eq!(world.get::<Node>(colors).unwrap().display, Display::None);
        Toggle.apply(&mut world, header);
        assert_eq!(world.get::<Node>(colors).unwrap().display, Display::Flex);
        assert_eq!(world.get::<Node>(spacing).unwrap().display, Display::None);
        Toggle.apply(&mut world, header);
        assert_eq!(world.get::<Text>(draft).unwrap().0, "Unsaved value");
        Toggle.apply(&mut world, header);
        world.despawn(panel);
        let panel = world.spawn_empty().id();
        let colors = accordion(&mut world, root, panel, "colors", "Colors");
        assert_eq!(world.get::<Node>(colors).unwrap().display, Display::Flex);
    }
}
