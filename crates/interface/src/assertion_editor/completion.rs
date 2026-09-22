use super::*;
use std::ops::Range;

#[derive(Clone)]
pub(super) struct Option {
    text: String,
    label: String,
    range: Range<usize>,
}

fn token(text: &str, caret: usize) -> std::option::Option<(Range<usize>, bool)> {
    let before = text.get(..caret)?;
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
        .map_or(0, |(index, c)| index + c.len_utf8());
    if start == caret {
        return None;
    }
    let prefix = &text[start..caret];
    if prefix.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-') {
        return None;
    }
    let end = text[caret..]
        .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
        .map_or(text.len(), |index| caret + index);
    let clause = before[..start]
        .rsplit([',', ';', '\n', '#'])
        .next()
        .unwrap_or_default();
    Some((start..end, prefix.starts_with('@') && !clause.contains(':')))
}

pub(super) fn keys(world: &mut World) {
    let Some(focused) = world.resource::<InputFocus>().get() else {
        return;
    };
    let Some(entity) = world
        .query::<(Entity, &Field)>()
        .iter(world)
        .find(|(_, field)| field.input == focused)
        .map(|(entity, _)| entity)
    else {
        return;
    };
    let keys = world.resource::<ButtonInput<KeyCode>>();
    if keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
    ]) {
        return;
    }
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    let tab = keys.just_pressed(KeyCode::Tab);
    let escape = keys.just_pressed(KeyCode::Escape);
    let step = i32::from(keys.just_pressed(KeyCode::ArrowDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp));
    let field = world.get::<Field>(entity).unwrap();
    let options = !field.options.is_empty();
    let mut text = world.get_mut::<EditableText>(focused).unwrap();
    if text.is_composing()
        || text.pending_paste.is_some()
        || text.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                bevy::text::TextEdit::ImeCommit { .. } | bevy::text::TextEdit::ImeSetCompose { .. }
            )
        })
    {
        return;
    }
    if enter || tab && options {
        text.pending_edits.retain(|edit| !matches!(edit, bevy::text::TextEdit::Insert(value) if value.contains(['\n', '\r', '\t'])));
    }
    if options && step != 0 {
        text.pending_edits.retain(|edit| {
            !matches!(
                edit,
                bevy::text::TextEdit::Up(_) | bevy::text::TextEdit::Down(_)
            )
        });
    }
    let value = text.value().to_string();
    let mut field = world.get_mut::<Field>(entity).unwrap();
    field.submit = enter && !options;
    field.complete = (tab || enter) && options;
    field.step = step;
    if escape {
        field.dismissed = Some(value);
    }
}

#[derive(Clone)]
struct Complete(Option);

pub(super) fn capture(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    fields: Query<&Field>,
    parents: Query<&ChildOf>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if event.input.key_code != KeyCode::Tab
        || keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    {
        return;
    }
    if parents
        .get(event.focused_entity)
        .ok()
        .and_then(|parent| fields.get(parent.parent()).ok())
        .is_some_and(|field| !field.options.is_empty())
    {
        event.propagate(false);
    }
}

impl Action for Complete {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(field) = world.get::<Field>(entity) else {
            return;
        };
        let input = field.input;
        if world.get::<bevy::ui::InteractionDisabled>(input).is_some() {
            return;
        }
        let mut value = value(world, input);
        let Some(current) = value.get(self.0.range.clone()) else {
            return;
        };
        if current.contains(char::is_whitespace) {
            return;
        }
        let mut range = self.0.range.clone();
        if value[range.end..].starts_with(' ') {
            range.end += 1;
        }
        let text = format!("{} ", self.0.text);
        value.replace_range(range.clone(), &text);
        if world.contains_resource::<bevy::text::FontCx>()
            && world.contains_resource::<bevy::text::LayoutCx>()
        {
            let mut fonts = world.remove_resource::<bevy::text::FontCx>().unwrap();
            let mut layout = world.remove_resource::<bevy::text::LayoutCx>().unwrap();
            let mut editor = world.get_mut::<EditableText>(input).unwrap();
            let mut driver = editor.editor.driver(&mut fonts.context, &mut layout.0);
            driver.select_byte_range(range.start, range.end);
            driver.insert_or_replace_selection(&text);
            world.insert_resource(fonts);
            world.insert_resource(layout);
        } else {
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(&value);
        }
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Pressed);
        let mut field = world.get_mut::<Field>(entity).unwrap();
        field.dismissed = Some(value);
        field.options.clear();
        field.signature.clear();
        let suggestions = field.suggestions;
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
    }
}

pub(super) fn update(world: &mut World, entity: Entity) {
    let field = world.get::<Field>(entity).unwrap();
    let input = field.input;
    let suggestions = field.suggestions;
    if world.get::<bevy::ui::InteractionDisabled>(input).is_some() {
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
        return;
    }
    let mut focus = world.resource::<InputFocus>().get();
    let focused = loop {
        let Some(entity) = focus else { break false };
        if entity == input || entity == suggestions {
            break true;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    };
    let text = world.get::<EditableText>(input).unwrap();
    if text.is_composing() || text.pending_paste.is_some() {
        return;
    }
    let value = value(world, input);
    let caret = text.editor.raw_selection().focus().index();
    let range = token(&value, caret);
    let visible = focused && field.dismissed.as_ref() != Some(&value) && range.is_some();
    let signature = format!("{value}:{caret}:{visible}");
    if field.signature != signature {
        let mut options = Vec::new();
        if visible {
            let (range, records) = range.unwrap();
            let query = value[range.start..caret]
                .trim_start_matches(['#', '@'])
                .to_lowercase();
            let rows = if records {
                &field.records
            } else {
                &field.concepts
            };
            for row in rows {
                let name = if records {
                    row["slug"].as_str().unwrap_or_default()
                } else {
                    row["name"].as_str().unwrap_or_default()
                };
                let head = row["head"].as_str().unwrap_or_default();
                if name.is_empty() || !format!("{name} {head}").to_lowercase().contains(&query) {
                    continue;
                }
                let marker = if records || value[range.clone()].starts_with('@') {
                    '@'
                } else {
                    '#'
                };
                let text = format!("{marker}{name}");
                let label = if records && !head.is_empty() {
                    format!("{text} · {head}")
                } else {
                    text.clone()
                };
                options.push(Option {
                    text,
                    label,
                    range: range.clone(),
                });
            }
            options.sort_by_key(|option| {
                let name = option.text[1..].to_lowercase();
                (name != query, !name.starts_with(&query), name)
            });
            options.truncate(12);
        }
        let mut field = world.get_mut::<Field>(entity).unwrap();
        field.options = options;
        field.selected = 0;
        field.signature = signature;
        redraw(world, entity);
    }
    let mut field = world.get_mut::<Field>(entity).unwrap();
    let step = std::mem::take(&mut field.step);
    field.selected = (field.selected as i32 + step)
        .clamp(0, field.options.len().saturating_sub(1) as i32) as usize;
    let complete = std::mem::take(&mut field.complete);
    let submit = std::mem::take(&mut field.submit);
    let option = field.options.get(field.selected).cloned();
    if complete && let Some(option) = option {
        Complete(option).apply(world, entity);
    } else if submit {
        super::submit(world, entity);
    } else if step != 0 {
        redraw(world, entity);
    }
    let show =
        visible && !world.get::<Field>(entity).unwrap().options.is_empty() && !submit && !complete;
    world.get_mut::<Node>(suggestions).unwrap().display =
        if show { Display::Flex } else { Display::None };
}

fn redraw(world: &mut World, entity: Entity) {
    let field = world.get::<Field>(entity).unwrap();
    let parent = field.suggestions;
    let options = field.options.clone();
    let selected = field.selected;
    let children: Vec<_> = world
        .get::<Children>(parent)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
    for (index, option) in options.into_iter().enumerate() {
        let label = option.label.clone();
        let button = world
            .spawn((
                crate::sand::button(0),
                Node {
                    width: percent(100),
                    min_height: px(28),
                    flex_shrink: 0.0,
                    padding: UiRect::all(px(4)),
                    ..default()
                },
                ActionButton::new(entity, crate::actions![Complete(option)]),
                ChildOf(parent),
            ))
            .id();
        if index == selected {
            world
                .entity_mut(button)
                .insert(crate::token_style::background(Token::Accent));
        }
        crate::edit_mode::label(world, button, &label, 14.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_at(world: &mut World, input: Entity, value: &str, caret: usize) {
        world.init_resource::<bevy::text::FontCx>();
        world.init_resource::<bevy::text::LayoutCx>();
        let mut fonts = world.remove_resource::<bevy::text::FontCx>().unwrap();
        let mut layout = world.remove_resource::<bevy::text::LayoutCx>().unwrap();
        let font = world
            .resource::<Assets<Font>>()
            .get(&world.resource::<crate::theme::Typography>().0)
            .unwrap();
        fonts.collection.register_fonts(font.data.clone(), None);
        fonts.set_sans_serif_family("Lato").unwrap();
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.editor.set_text(value);
        text.editor
            .driver(&mut fonts.context, &mut layout.0)
            .move_to_byte(caret);
        world.insert_resource(fonts);
        world.insert_resource(layout);
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Pressed);
    }

    #[test]
    fn autocomplete_replaces_only_the_active_concept_slug_or_unit() {
        let (mut app, _, owner) = crate::protein_area::tests::fixture();
        let parent = app.world_mut().spawn(Node::default()).id();
        super::super::spawn(
            app.world_mut(),
            parent,
            RecordBinding {
                area: owner,
                uid: String::new(),
                source: crate::protein_area::Source::Local,
            },
            &json!({}),
            true,
        );
        let input = super::super::input(app.world(), parent).unwrap();
        {
            let mut field = app.world_mut().get_mut::<Field>(parent).unwrap();
            field.concepts = vec![json!({"name":"planned"}), json!({"name":"day"})];
            field.records = vec![json!({"uid":"project", "slug":"project", "head":"My project"})];
        }
        for (text, caret, expected, key) in [
            ("#pla #done", 4, "#planned #done", KeyCode::Tab),
            (
                "#depends-on @pro",
                16,
                "#depends-on @project ",
                KeyCode::Enter,
            ),
            ("#cost: 2 @da", 12, "#cost: 2 @day ", KeyCode::Tab),
        ] {
            type_at(app.world_mut(), input, text, caret);
            update(app.world_mut(), parent);
            assert_eq!(app.world().get::<Field>(parent).unwrap().options.len(), 1);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            keys(app.world_mut());
            update(app.world_mut(), parent);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            assert_eq!(super::super::value(app.world(), input), expected);
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(input));
        }
    }

    #[test]
    fn completion_tracks_caret_links_and_units() {
        assert_eq!(token("#done #dépe other", 12), Some((6..12, false)));
        assert_eq!(token("#depends-on @pro rest", 15), Some((12..16, true)));
        assert_eq!(token("#cost: 12 @re", 13), Some((10..13, false)));
        assert_eq!(token("#done ", 6), None);
        assert_eq!(token("#done\u{2003}#pla", 12), Some((8..12, false)));
    }
}
