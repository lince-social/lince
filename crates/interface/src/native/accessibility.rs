use bevy::{a11y::AccessibilityNode, prelude::*};

mod actions;
mod text;
pub use actions::MAX_ACTION_BYTES;
pub use actions::{Context, context, owner_context, owns_keyboard, perform, supported_actions};
pub use actions::{reveal_target, revealed};
pub use text::text_nodes;

#[derive(Component)]
struct LiveText;

#[derive(Component)]
struct AccessibleInput(bool);

pub fn protected_text_input(world: &World, entity: Entity) -> bool {
    world
        .get::<AccessibleInput>(entity)
        .is_some_and(|input| input.0)
}

pub(crate) struct AccessibilityPlugin;

impl Plugin for AccessibilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(
            |event: On<bevy::input_focus::FocusGained>, mut commands: Commands| {
                if event.cause == bevy::input_focus::FocusCause::Navigated
                    && event.entity == event.original_event_target()
                {
                    let entity = event.entity;
                    commands.queue(move |world: &mut World| actions::navigated(world, entity));
                }
            },
        );
        app.add_systems(
            PostUpdate,
            (sync, inputs)
                .after(bevy::text::EditableTextSystems)
                .after(bevy::ui::UiSystems::PostLayout)
                .before(bevy::a11y::AccessibilitySystems::Update),
        );
    }
}

pub(crate) fn input(world: &mut World, entity: Entity, label: &str, multiline: bool) {
    let mut node = accesskit::Node::new(if multiline {
        accesskit::Role::MultilineTextInput
    } else {
        accesskit::Role::TextInput
    });
    node.set_label(label);
    let protected = label.to_lowercase().contains("password")
        || label.to_lowercase().contains("api key")
        || label.to_lowercase().contains("secret");
    if protected {
        node.set_role(accesskit::Role::PasswordInput);
    }
    world
        .entity_mut(entity)
        .insert((AccessibilityNode(node), AccessibleInput(protected)));
}

pub(crate) fn status(world: &mut World, entity: Entity) {
    let mut node = accesskit::Node::new(accesskit::Role::Status);
    node.set_live(accesskit::Live::Polite);
    node.set_live_atomic();
    if let Some(text) = world.get::<Text>(entity) {
        node.set_label(text.0.clone());
    }
    world
        .entity_mut(entity)
        .insert((AccessibilityNode(node), LiveText));
}

pub(crate) fn question(world: &mut World, entity: Entity, prompt: &str) {
    let mut node = accesskit::Node::new(accesskit::Role::Group);
    node.set_label(prompt);
    node.set_live(accesskit::Live::Polite);
    node.set_live_atomic();
    world.entity_mut(entity).insert(AccessibilityNode(node));
}

fn sync(mut labels: Query<(&Text, &mut AccessibilityNode), (With<LiveText>, Changed<Text>)>) {
    for (text, mut node) in &mut labels {
        node.set_label(text.0.clone());
    }
}

fn inputs(
    mut inputs: Query<(
        Ref<bevy::text::EditableText>,
        Ref<AccessibleInput>,
        &mut AccessibilityNode,
    )>,
) {
    use crate::host::performance::{self, Count, Stage};
    let _timing = performance::Span::new(Stage::NativeAccessibleInputs);
    for (input, policy, mut node) in &mut inputs {
        if !input.is_changed() && !policy.is_changed() && !node.is_changed() {
            continue;
        }
        performance::count(Count::AccessibleInputChecked);
        let text = input.value();
        let value = if policy.0 {
            let characters = text.chars().count();
            if node.value().is_some_and(|current| {
                current.len() == characters.saturating_mul('•'.len_utf8())
                    && current.chars().all(|character| character == '•')
            }) {
                continue;
            }
            "•".repeat(characters)
        } else {
            if node.value().is_some_and(|current| text == current) {
                continue;
            }
            text.to_string()
        };
        performance::add(Count::AccessibleValueBytes, value.len() as u64);
        node.set_value(value);
    }
}

pub fn secret(world: &mut World, entity: Entity) {
    if let Some(mut node) = world.get_mut::<AccessibilityNode>(entity) {
        node.set_role(accesskit::Role::PasswordInput);
    }
    world.entity_mut(entity).insert(AccessibleInput(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_values_are_retained_and_replaced_values_and_secret_policy_are_current() {
        let mut app = App::new();
        app.add_plugins(AccessibilityPlugin);
        let field = app
            .world_mut()
            .spawn(crate::native::sand::editable("é界"))
            .id();
        input(app.world_mut(), field, "Text", true);
        app.update();
        let changed = app
            .world()
            .entity(field)
            .get_ref::<AccessibilityNode>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(field)
                .get_ref::<AccessibilityNode>()
                .unwrap()
                .last_changed(),
            changed
        );
        app.world_mut()
            .get_mut::<AccessibilityNode>(field)
            .unwrap()
            .set_value("stale");
        app.update();
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().value(),
            Some("é界")
        );
        secret(app.world_mut(), field);
        app.update();
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().value(),
            Some("••")
        );
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor
            .set_text("é");
        app.update();
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().value(),
            Some("•")
        );
        app.world_mut()
            .entity_mut(field)
            .insert(AccessibleInput(false));
        app.update();
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().value(),
            Some("é")
        );
    }

    #[test]
    fn live_status_changes_and_input_labels_are_exposed() {
        let mut app = App::new();
        app.add_plugins(AccessibilityPlugin);
        let field = app.world_mut().spawn_empty().id();
        input(app.world_mut(), field, "Message", true);
        let label = app.world_mut().spawn(Text::new("Working")).id();
        status(app.world_mut(), label);
        let password = app
            .world_mut()
            .spawn(crate::native::sand::editable("a界"))
            .id();
        input(app.world_mut(), password, "Provider key", false);
        secret(app.world_mut(), password);
        app.update();
        let protected = app.world().get::<AccessibilityNode>(password).unwrap();
        assert_eq!(protected.role(), accesskit::Role::PasswordInput);
        assert_eq!(protected.value(), Some("••"));
        assert!(protected_text_input(app.world(), password));
        assert!(!protected_text_input(app.world(), field));
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().label(),
            Some("Message")
        );
        assert_eq!(
            app.world().get::<AccessibilityNode>(field).unwrap().role(),
            accesskit::Role::MultilineTextInput
        );
        assert_eq!(
            app.world().get::<AccessibilityNode>(label).unwrap().live(),
            Some(accesskit::Live::Polite)
        );
        app.world_mut().get_mut::<Text>(label).unwrap().0 = "Refused: the revision changed".into();
        app.update();
        assert_eq!(
            app.world().get::<AccessibilityNode>(label).unwrap().label(),
            Some("Refused: the revision changed")
        );
    }
}
