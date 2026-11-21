// non-lean-yet
// Scope: struct Option, struct State, struct Input, fn token.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 202, 208, 213,
//   283, 336, 349, 363, 441, 453.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   56, 66, 209, 294, 336, 367.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (113, 115, 153, 158, 217, 218, 219, 228, 235, 240, 246, 252, 265, 277, 283, 293,
//   372, 390, 402, 415, 429, 431, 439).

use super::*;
use crate::host::performance::{self, Count, Stage};
use std::ops::Range;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Option {
    text: String,
    label: String,
    range: Range<usize>,
}

#[derive(Default)]
pub(super) struct State {
    input: std::option::Option<Input>,
    dirty: bool,
}

struct Input {
    value: String,
    caret: usize,
    range: std::option::Option<(Range<usize>, bool)>,
    visible: bool,
}

impl State {
    pub(super) fn invalidate(&mut self) {
        self.dirty = true;
    }
}

fn token(text: &str, caret: usize) -> std::option::Option<(Range<usize>, bool)> {
    let before = text.get(..caret)?;
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    let mut end = text.len();
    for (index, ch) in text.char_indices() {
        if !quoted && (ch.is_whitespace() || matches!(ch, ',' | ';' | ':')) {
            if start < caret && caret <= index {
                end = index;
                break;
            }
            start = index + ch.len_utf8();
            if start >= caret {
                return None;
            }
        } else if ch == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = quoted && ch == '\\' && !escaped;
    }
    if !(start < caret && caret <= end) {
        return None;
    }
    let range = start..end;
    let prefix = &text[start..caret];
    if prefix.starts_with('^') {
        return None;
    }
    if prefix.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-') {
        return None;
    }
    let mut amount = false;
    let mut quoted = false;
    let mut escaped = false;
    for ch in before[..start].chars() {
        if !quoted {
            if matches!(ch, ',' | ';' | '\n' | '#') {
                amount = false;
            }
            if ch == ':' {
                amount = true;
            }
        }
        if ch == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = quoted && ch == '\\' && !escaped;
    }
    Some((range, prefix.starts_with('@') && !amount))
}

pub(super) fn keys(world: &mut World, fields: &mut QueryState<(Entity, &'static Field)>) {
    let _keys = performance::Span::new(Stage::AssertionEditorKeys);
    let Some(focused) = world.resource::<InputFocus>().get() else {
        return;
    };
    let Some(entity) = fields
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
    let text = world.get::<EditableText>(focused).unwrap();
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
    let captured = |edit: &bevy::text::TextEdit| {
        ((enter || tab && options)
            && matches!(edit, bevy::text::TextEdit::Insert(value) if value.contains(['\n', '\r', '\t'])))
            || (options
                && step != 0
                && matches!(
                    edit,
                    bevy::text::TextEdit::Up(_) | bevy::text::TextEdit::Down(_)
                ))
    };
    let strip = text.pending_edits.iter().any(captured);
    let dismissed = (escape
        && field
            .dismissed
            .as_deref()
            .is_none_or(|value| text.value() != value))
    .then(|| text.value().to_string());
    let submit = enter && !options;
    let complete = (tab || enter) && options;
    let changed = field.submit != submit
        || field.complete != complete
        || field.step != step
        || dismissed.is_some();
    if strip {
        world
            .get_mut::<EditableText>(focused)
            .unwrap()
            .pending_edits
            .retain(|edit| !captured(edit));
    }
    if changed {
        let mut field = world.get_mut::<Field>(entity).unwrap();
        field.submit = submit;
        field.complete = complete;
        field.step = step;
        if let Some(value) = dismissed {
            field.dismissed = Some(value);
        }
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
        field.completion.invalidate();
        let suggestions = field.suggestions;
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
    }
}

pub(super) fn update(world: &mut World, entity: Entity) {
    let _update = performance::Span::new(Stage::AssertionCompletion);
    let field = world.get::<Field>(entity).unwrap();
    let input = field.input;
    let suggestions = field.suggestions;
    if world.get::<bevy::ui::InteractionDisabled>(input).is_some() {
        world
            .get_mut::<Node>(suggestions)
            .unwrap()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(Display::None);
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
    let previous = field.completion.input.as_ref();
    let changed = previous.is_none_or(|previous| text.value() != previous.value.as_str());
    let owned = changed.then(|| {
        performance::add(Count::AssertionCompletionInputsCopied, 1);
        text.value().to_string()
    });
    let value = owned
        .as_deref()
        .unwrap_or_else(|| previous.unwrap().value.as_str());
    let caret = text.editor.raw_selection().focus().index();
    let moved = changed || previous.is_none_or(|previous| previous.caret != caret);
    let range = if moved {
        token(value, caret)
    } else {
        previous.unwrap().range.clone()
    };
    let visible = focused && field.dismissed.as_deref() != Some(value) && range.is_some();
    if moved
        || previous.is_none_or(|previous| previous.visible != visible)
        || field.completion.dirty
    {
        performance::add(Count::AssertionCompletionRefreshes, 1);
        let mut options = Vec::new();
        if visible {
            let (range, records) = range.as_ref().unwrap();
            let query = value[range.start..caret]
                .trim_start_matches(['#', '@'])
                .trim_matches('"');
            let query = store::vocabulary::term_text(query)
                .unwrap_or_else(|_| query.into())
                .to_lowercase();
            let rows = if *records {
                &field.records
            } else {
                &field.concepts
            };
            let mut search = String::new();
            for row in rows.iter() {
                let name = if *records {
                    row["slug"].as_str().unwrap_or_default()
                } else {
                    row["name"].as_str().unwrap_or_default()
                };
                if name.is_empty() {
                    continue;
                }
                let head = row["head"].as_str().unwrap_or_default();
                search.clear();
                search.push_str(name);
                search.push(' ');
                search.push_str(head);
                search.push(' ');
                for (index, alternative) in row["terms"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|term| term["name"].as_str())
                    .enumerate()
                {
                    if index != 0 {
                        search.push(' ');
                    }
                    search.push_str(alternative);
                }
                if !search.to_lowercase().contains(&query) {
                    continue;
                }
                let marker = if *records || value[range.clone()].starts_with('@') {
                    '@'
                } else {
                    '#'
                };
                let readable = format!("{marker}{}", syntax::term_text(name));
                let text = if marker == '#'
                    && let Some(uid) = row["uid"]
                        .as_str()
                        .filter(|uid| nucleus::valid_uid(uid, "c"))
                {
                    format!("{readable} ^{uid}")
                } else {
                    readable.clone()
                };
                let label = if *records && !head.is_empty() {
                    format!("{readable} · {head}")
                } else {
                    format!(
                        "{readable} · {} · {}",
                        row["source"]["original_name"].as_str().unwrap_or(name),
                        row["uid"].as_str().unwrap_or("")
                    )
                };
                options.push(Option {
                    text,
                    label,
                    range: range.clone(),
                });
            }
            options.sort_by_cached_key(|option| {
                let name = option.text[1..].to_lowercase();
                (name != query, !name.starts_with(&query), name)
            });
            options.truncate(12);
        }
        let mut field = world.get_mut::<Field>(entity).unwrap();
        let options_changed = field.options != options;
        let redraw_needed = options_changed || field.selected != 0;
        if options_changed {
            field.options = options;
        }
        field.selected = 0;
        match field.completion.input.as_mut() {
            Some(previous) => {
                if let Some(value) = owned {
                    previous.value = value;
                }
                previous.caret = caret;
                previous.range = range;
                previous.visible = visible;
            }
            None => {
                field.completion.input = Some(Input {
                    value: owned.unwrap(),
                    caret,
                    range,
                    visible,
                });
            }
        }
        field.completion.dirty = false;
        if redraw_needed {
            redraw(world, entity);
        }
    }
    let field = world.get::<Field>(entity).unwrap();
    let step = field.step;
    let selected = (field.selected as i32 + step)
        .clamp(0, field.options.len().saturating_sub(1) as i32) as usize;
    let changed = selected != field.selected;
    let complete = field.complete;
    let submit = field.submit;
    let option = if complete {
        field.options.get(selected).cloned()
    } else {
        None
    };
    if step != 0 || complete || submit || changed {
        let mut field = world.get_mut::<Field>(entity).unwrap();
        field.step = 0;
        field.complete = false;
        field.submit = false;
        field.selected = selected;
    }
    if complete && let Some(option) = option {
        Complete(option).apply(world, entity);
    } else if submit {
        super::submit(world, entity);
    } else if changed {
        redraw(world, entity);
    }
    let show =
        visible && !world.get::<Field>(entity).unwrap().options.is_empty() && !submit && !complete;
    let display = if show { Display::Flex } else { Display::None };
    let mut node = world.get_mut::<Node>(suggestions).unwrap();
    if node.display != display {
        node.display = display;
    }
}

fn redraw(world: &mut World, entity: Entity) {
    performance::add(Count::AssertionCompletionRedraws, 1);
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
                crate::native::sand::button(0),
                Node {
                    width: percent(100),
                    min_height: px(28),
                    flex_shrink: 0.0,
                    padding: UiRect::all(px(4)),
                    ..default()
                },
                ActionButton::new(entity, crate::native::actions![Complete(option)]),
                ChildOf(parent),
            ))
            .id();
        if index == selected {
            world
                .entity_mut(button)
                .insert(crate::native::token_style::background(Token::Accent));
        }
        crate::native::edit_mode::label(world, button, &label, 14.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_terms_keep_their_range_and_do_not_change_address_completion_into_units() {
        let text = "#\"saved geographic: place\" @street4";
        let caret = text.find("geographic").unwrap() + "geographic".len();
        let (range, record) = token(text, caret).unwrap();
        assert_eq!(&text[range], "#\"saved geographic: place\"");
        assert!(!record);
        assert!(token(text, text.len()).unwrap().1);
        assert!(
            !token("#cost: 2 @kilograms", "#cost: 2 @kilograms".len())
                .unwrap()
                .1
        );
    }

    fn type_at(world: &mut World, input: Entity, value: &str, caret: usize) {
        world.init_resource::<bevy::text::FontCx>();
        world.init_resource::<bevy::text::LayoutCx>();
        let mut fonts = world.remove_resource::<bevy::text::FontCx>().unwrap();
        let mut layout = world.remove_resource::<bevy::text::LayoutCx>().unwrap();
        let font = world
            .resource::<Assets<Font>>()
            .get(&world.resource::<crate::native::theme::Typography>().0)
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
        let (mut app, _, owner) = crate::native::protein_area::tests::fixture();
        let parent = app.world_mut().spawn(Node::default()).id();
        super::super::spawn(
            app.world_mut(),
            parent,
            RecordBinding {
                area: owner,
                uid: String::new(),
                source: crate::native::protein_area::Source::Local,
            },
            &json!({}),
            true,
        );
        let input = super::super::input(app.world(), parent).unwrap();
        {
            let mut field = app.world_mut().get_mut::<Field>(parent).unwrap();
            field.concepts = vec![json!({"name":"planned"}), json!({"name":"day"})].into();
            field.records =
                vec![json!({"uid":"project", "slug":"project", "head":"My project"})].into();
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
            app.world_mut().run_system_cached(keys).unwrap();
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
