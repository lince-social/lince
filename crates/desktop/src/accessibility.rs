use bevy::{a11y::AccessibilityNode, prelude::*};

#[derive(Component)]
struct LiveText;

#[derive(Component)]
struct AccessibleInput(bool);

pub(crate) struct AccessibilityPlugin;

impl Plugin for AccessibilityPlugin {
    fn build(&self, app: &mut App) {
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
        &bevy::text::EditableText,
        &AccessibleInput,
        &mut AccessibilityNode,
    )>,
) {
    for (input, policy, mut node) in &mut inputs {
        let value = if policy.0 {
            "•".repeat(input.value().chars().count())
        } else {
            input.value().to_string()
        };
        if node.value() != Some(value.as_str()) {
            node.set_value(value);
        }
    }
}

pub(crate) fn secret(world: &mut World, entity: Entity) {
    if let Some(mut node) = world.get_mut::<AccessibilityNode>(entity) {
        node.set_role(accesskit::Role::PasswordInput);
    }
    world.entity_mut(entity).insert(AccessibleInput(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_status_changes_and_input_labels_are_exposed() {
        let mut app = App::new();
        app.add_plugins(AccessibilityPlugin);
        let field = app.world_mut().spawn_empty().id();
        input(app.world_mut(), field, "Message", true);
        let label = app.world_mut().spawn(Text::new("Working")).id();
        status(app.world_mut(), label);
        let password = app.world_mut().spawn(crate::sand::editable("a界")).id();
        input(app.world_mut(), password, "Provider key", false);
        secret(app.world_mut(), password);
        app.update();
        let protected = app.world().get::<AccessibilityNode>(password).unwrap();
        assert_eq!(protected.role(), accesskit::Role::PasswordInput);
        assert_eq!(protected.value(), Some("••"));
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
