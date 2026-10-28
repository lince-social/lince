use bevy::{
    prelude::*,
    text::{EditableText, EditableTextSystems, TextEdit},
};

pub struct TextEditingPlugin;

impl Plugin for TextEditingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, normalize.before(EditableTextSystems));
    }
}

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
        let edits = std::mem::take(&mut input.pending_edits);
        for edit in edits {
            match edit {
                TextEdit::ImeCommit { value } => {
                    commit(&mut input, value.as_str());
                }
                edit => input.queue_edit(edit),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        ecs::system::RunSystemOnce,
        text::{FontCx, LayoutCx},
    };

    #[test]
    fn ime_replacement_obeys_result_length_filters_and_edit_order() {
        let mut world = World::new();
        world.init_resource::<FontCx>();
        let font = Font::from_bytes(
            include_bytes!("../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
        );
        world
            .resource_mut::<FontCx>()
            .collection
            .register_fonts(font.data, None);
        world
            .resource_mut::<FontCx>()
            .set_sans_serif_family("Lato")
            .unwrap();
        world.init_resource::<LayoutCx>();
        world.init_resource::<bevy::clipboard::Clipboard>();
        let mut input = EditableText::new("25");
        input.max_characters = Some(3);
        input.queue_edit(TextEdit::SelectAll);
        input.queue_edit(TextEdit::ImeCommit {
            value: "999".into(),
        });
        input.queue_edit(TextEdit::ImeCommit { value: "0".into() });
        let entity = world.spawn(input).id();
        world.run_system_once(normalize).unwrap();
        world.run_system_once(bevy::text::apply_text_edits).unwrap();
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
        world.run_system_once(normalize).unwrap();
        world.run_system_once(bevy::text::apply_text_edits).unwrap();
        assert_eq!(world.get::<EditableText>(entity).unwrap().value(), "é世界");

        world
            .entity_mut(entity)
            .insert(bevy::text::EditableTextFilter::new(|character| {
                character.is_ascii_digit()
            }));
        let mut input = world.get_mut::<EditableText>(entity).unwrap();
        input.queue_edit(TextEdit::SelectAll);
        input.queue_edit(TextEdit::ImeCommit { value: "no".into() });
        input.queue_edit(TextEdit::ImeCommit { value: "35".into() });
        world.run_system_once(normalize).unwrap();
        world.run_system_once(bevy::text::apply_text_edits).unwrap();
        assert_eq!(world.get::<EditableText>(entity).unwrap().value(), "35");
    }
}
