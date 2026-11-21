//! Integrates Bevy text input widgets and normalizes commits before applying edits.

use bevy::{
    prelude::*,
    text::{EditableText, EditableTextSystems, TextEdit},
    ui_widgets::TextInput,
};
use std::mem::take;

/// Makes editors focusable as widgets and applies ordinary insert limits to IME commits.
pub struct TextEditingPlugin;

impl Plugin for TextEditingPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(attach_widget)
            .add_systems(PostUpdate, normalize.before(EditableTextSystems));
    }
}

fn attach_widget(
    event: On<Add<EditableText>>,
    without_widget: Query<(), Without<TextInput>>,
    mut commands: Commands,
) {
    if without_widget.contains(event.entity) {
        commands.entity(event.entity).try_insert(TextInput);
    }
}

/// Queues an IME commit through Bevy's ordinary insertion path.
pub fn commit(input: &mut EditableText, value: &str) {
    input.queue_edit(TextEdit::clear_ime_compose());
    input.queue_edit(TextEdit::Insert(value.into()));
}

fn normalize(mut inputs: Query<&mut EditableText, Changed<EditableText>>) {
    for mut input in &mut inputs {
        if !input
            .pending_edits
            .iter()
            .any(|edit| matches!(edit, TextEdit::ImeCommit { .. }))
        {
            continue;
        }
        for edit in take(&mut input.pending_edits) {
            let edit = match edit {
                TextEdit::ImeCommit { value } => {
                    input.queue_edit(TextEdit::clear_ime_compose());
                    TextEdit::Insert(value)
                }
                edit => edit,
            };
            input.queue_edit(edit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::bundled_font;
    use bevy::{
        clipboard::Clipboard,
        ecs::system::RunSystemOnce,
        text::{EditableTextFilter, FontCx, LayoutCx, apply_text_edits},
    };

    fn apply_pending_edits(world: &mut World) {
        world.run_system_once(normalize).unwrap();
        world.run_system_once(apply_text_edits).unwrap();
    }

    #[test]
    fn ime_replacement_obeys_result_length_filters_and_edit_order() {
        let mut app = App::new();
        app.add_plugins(TextEditingPlugin);
        let world = app.world_mut();
        world.init_resource::<FontCx>();
        let font = bundled_font(include_bytes!(
            "../../../../institute/assets/fonts/Lato/Lato-Regular.ttf"
        ));
        world
            .resource_mut::<FontCx>()
            .collection
            .register_fonts(font.data, None);
        world
            .resource_mut::<FontCx>()
            .set_sans_serif_family("Lato")
            .unwrap();
        world.init_resource::<LayoutCx>();
        world.init_resource::<Clipboard>();
        let mut input = EditableText::new("25");
        input.max_characters = Some(3);
        input.queue_edit(TextEdit::SelectAll);
        input.queue_edit(TextEdit::ImeCommit {
            value: "999".into(),
        });
        input.queue_edit(TextEdit::ImeCommit { value: "0".into() });
        let entity = world.spawn(input).id();
        world.flush();
        assert!(world.get::<TextInput>(entity).is_some());
        apply_pending_edits(world);
        assert_eq!(world.get::<EditableText>(entity).unwrap().value(), "999");
        assert_eq!(
            world.get::<EditableText>(entity).unwrap().max_characters,
            Some(3)
        );

        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        input.queue_edit(TextEdit::SelectAll);
        input.queue_edit(TextEdit::ImeCommit {
            value: "é世".into(),
        });
        input.queue_edit(TextEdit::ImeCommit {
            value: "界".into()
        });
        input.queue_edit(TextEdit::ImeCommit { value: "a".into() });
        apply_pending_edits(world);
        assert_eq!(world.get::<EditableText>(entity).unwrap().value(), "é世界");

        world
            .entity_mut(entity)
            .insert(EditableTextFilter::new(|character| {
                character.is_ascii_digit()
            }));
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        input.queue_edit(TextEdit::SelectAll);
        input.queue_edit(TextEdit::ImeCommit { value: "no".into() });
        input.queue_edit(TextEdit::ImeCommit { value: "35".into() });
        apply_pending_edits(world);
        assert_eq!(world.get::<EditableText>(entity).unwrap().value(), "35");
    }
}
