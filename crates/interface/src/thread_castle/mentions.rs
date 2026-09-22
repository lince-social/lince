use super::*;
use bevy::{
    input_focus::{FocusCause, InputFocus},
    text::{FontCx, LayoutCx},
};
use cell::{ClientMessage, ServerMessage};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mention {
    range: Range<usize>,
    uid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    uid: String,
    slug: String,
    head: String,
}

#[derive(Component, Default)]
struct Draft {
    text: String,
    links: Vec<Mention>,
    rows: Vec<Candidate>,
    options: Vec<Candidate>,
    filter: Option<String>,
    query: Option<Range<usize>>,
    dismissed: Option<(String, usize)>,
    selected: usize,
    popup: Option<Entity>,
    subscription: Option<String>,
    error: Option<String>,
    loaded: bool,
    rendered: String,
}

#[derive(Resource, Default)]
struct Subscriptions(HashMap<String, (Entity, tokio::sync::mpsc::Sender<ClientMessage>)>);

#[derive(Component)]
struct InlineLink {
    input: Entity,
    range: Range<usize>,
    line: usize,
    uid: String,
}

#[derive(Component)]
struct SelectedOption;

pub(super) fn attach(world: &mut World, input: Entity) {
    world.entity_mut(input).insert(Draft::default());
}

fn reconcile(draft: &mut Draft, text: &str) {
    if draft.text == text {
        return;
    }
    let prefix = draft
        .text
        .chars()
        .zip(text.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = draft.text[prefix..]
        .chars()
        .rev()
        .zip(text[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let end = draft.text.len() - suffix;
    let delta = text.len() as isize - draft.text.len() as isize;
    draft.links.retain_mut(|link| {
        if link.range.end <= prefix {
            true
        } else if link.range.start >= end {
            link.range = link.range.start.checked_add_signed(delta).unwrap()
                ..link.range.end.checked_add_signed(delta).unwrap();
            true
        } else {
            false
        }
    });
    draft.text = text.into();
    draft.links.retain(|link| {
        !text[link.range.end..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
    });
}

fn query(text: &str, caret: usize) -> Option<Range<usize>> {
    let before = text.get(..caret)?;
    let start = before.rfind('@')?;
    if before[..start]
        .chars()
        .next_back()
        .is_some_and(|ch| !ch.is_whitespace() && !"([{".contains(ch))
    {
        return None;
    }
    if !before[start + 1..]
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    let mut code = 0;
    let mut chars = before[..start].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            let mut count = 1;
            while chars.peek() == Some(&'`') {
                count += 1;
                chars.next();
            }
            if code == 0 {
                code = count;
            } else if code == count {
                code = 0;
            }
        }
    }
    (code == 0).then_some(start..caret)
}

fn candidates(rows: &[Candidate], query: &str) -> Vec<Candidate> {
    let query = query.to_lowercase();
    let mut result: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.slug.to_lowercase().contains(&query)
                || row.head.to_lowercase().contains(&query)
                || row.uid.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    result.sort_by_key(|row| {
        let slug = row.slug.to_lowercase();
        (
            slug != query,
            !slug.starts_with(&query),
            row.head.to_lowercase(),
            row.uid.clone(),
        )
    });
    result.truncate(12);
    result
}

pub(super) fn body(world: &mut World, input: Entity, text: &str) -> String {
    let Some(mut draft) = world.get_mut::<Draft>(input) else {
        return text.into();
    };
    reconcile(&mut draft, text);
    let mut body = text.to_string();
    for link in draft.links.iter().rev() {
        let mut label = String::new();
        for ch in text[link.range.clone()].chars() {
            if "\\[]`*_<>~".contains(ch) {
                label.push('\\');
            }
            label.push(ch);
        }
        body.replace_range(
            link.range.clone(),
            &format!("[{label}](record:{})", link.uid),
        );
    }
    body
}

pub(crate) fn receive_message(world: &mut World, event: &ServerMessage) {
    let (id, rows, error) = match event {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, Some(rows), None)
        }
        ServerMessage::Error { id, message, .. } => (id, None, Some(message.clone())),
        _ => return,
    };
    let input = world
        .get_resource::<Subscriptions>()
        .and_then(|s| s.0.get(id))
        .map(|(input, _)| *input);
    let Some(mut draft) = input.and_then(|input| world.get_mut::<Draft>(input)) else {
        return;
    };
    if draft.subscription.as_ref() != Some(id) {
        return;
    }
    draft.error = error;
    if let Some(rows) = rows {
        draft.loaded = true;
        draft.rows = rows
            .iter()
            .filter_map(|row| {
                Some(Candidate {
                    uid: row["uid"].as_str()?.into(),
                    slug: row["slug"].as_str().unwrap_or("").into(),
                    head: row["head"].as_str().unwrap_or("").into(),
                })
            })
            .collect();
    }
    draft.filter = None;
}

pub(super) fn sync(world: &mut World) {
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    let forms: Vec<_> = world
        .query::<(Entity, &ThreadForm)>()
        .iter(world)
        .filter(|(_, form)| form.thread.is_some())
        .map(|(form, state)| {
            (
                form,
                state.input,
                state.binding.clone(),
                state.pending.is_some(),
            )
        })
        .collect();
    for (form, input, binding, pending) in forms {
        let Some(mut draft) = world.entity_mut(input).take::<Draft>() else {
            continue;
        };
        let editor = world.get::<EditableText>(input).unwrap();
        let text = editor.value().to_string();
        let caret = editor.editor.raw_selection().focus().index();
        let composing = editor.is_composing();
        reconcile(&mut draft, &text);
        let mut focus = focused;
        let mut picker_focused = false;
        while let Some(entity) = focus {
            if draft.popup == Some(entity) {
                picker_focused = true;
                break;
            }
            focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        let active = if (focused == Some(input) || picker_focused)
            && !pending
            && !composing
            && draft.dismissed.as_ref() != Some(&(text.clone(), caret))
        {
            query(&text, caret).filter(|range| {
                !draft
                    .links
                    .iter()
                    .any(|link| link.range.start <= range.start && link.range.end >= range.end)
            })
        } else {
            None
        };
        if draft.query != active {
            draft.selected = 0;
        }
        draft.query = active;
        if let Some(range) = &draft.query {
            if draft.subscription.is_none() {
                if let Some(sender) = crate::protein_area::editor_sender(world, &binding) {
                    let id = nucleus::new_uid("mention");
                    let protein = serde_json::from_value(serde_json::json!({
                        "source":"record", "fields":["uid","head","slug"], "limit":null
                    }))
                    .unwrap();
                    if sender
                        .try_send(ClientMessage::Subscribe {
                            id: id.clone(),
                            protein,
                        })
                        .is_ok()
                    {
                        world.init_resource::<Subscriptions>();
                        world
                            .resource_mut::<Subscriptions>()
                            .0
                            .insert(id.clone(), (input, sender));
                        draft.subscription = Some(id);
                        draft.error = None;
                        draft.loaded = false;
                    } else {
                        draft.error = Some("Record search is busy. Try again.".into());
                    }
                } else {
                    draft.error = Some("Record search is disconnected.".into());
                }
            }
            let filter = &text[range.start + 1..range.end];
            if draft.filter.as_deref() != Some(filter) {
                draft.options = candidates(&draft.rows, filter);
                draft.filter = Some(filter.into());
            }
            draft.selected = draft.selected.min(draft.options.len().saturating_sub(1));
            render(world, form, input, &mut draft);
        } else {
            draft.options.clear();
            draft.filter = None;
            draft.subscription = None;
            draft.rows.clear();
            draft.rendered.clear();
            if let Some(popup) = draft.popup.take() {
                world.despawn(popup);
            }
        }
        world.entity_mut(input).insert(draft);
    }
    let active: HashSet<_> = world
        .query::<&Draft>()
        .iter(world)
        .filter_map(|draft| draft.subscription.clone())
        .collect();
    if let Some(mut subscriptions) = world.get_resource_mut::<Subscriptions>() {
        subscriptions.0.retain(|id, (_, sender)| {
            active.contains(id)
                || (sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_err()
                    && !sender.is_closed())
        });
    }
}

fn render(world: &mut World, form: Entity, input: Entity, draft: &mut Draft) {
    let signature = format!(
        "{:?}:{}:{:?}:{}",
        draft.options, draft.selected, draft.error, draft.loaded
    );
    if draft.rendered == signature {
        return;
    }
    draft.rendered = signature;
    let popup = *draft.popup.get_or_insert_with(|| {
        world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: percent(100),
                    left: px(0),
                    width: percent(100),
                    max_height: px(260),
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(px(1)),
                    padding: UiRect::all(px(4)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
                crate::scroll_sand::ScrollSand,
                crate::token_style::background(crate::tokens::Token::Surface),
                crate::token_style::border(crate::tokens::Token::Accent),
                GlobalZIndex(80),
                ChildOf(world.get::<ChildOf>(input).unwrap().parent()),
            ))
            .id()
    });
    if let Some(children) = world.get::<Children>(popup) {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    if draft.options.is_empty() {
        crate::edit_mode::label(
            world,
            popup,
            draft.error.as_deref().unwrap_or(if draft.loaded {
                "No matching Records"
            } else {
                "Searching Records…"
            }),
            14.0,
        );
    }
    for (index, row) in draft.options.iter().enumerate() {
        let slug = if row.slug.is_empty() {
            &row.uid
        } else {
            &row.slug
        };
        let title = format!(
            "{}@{} · {}",
            if index == draft.selected {
                "› "
            } else {
                "  "
            },
            slug,
            row.head
        );
        let button = control(
            world,
            popup,
            form,
            &title,
            Choose {
                input,
                uid: row.uid.clone(),
            },
        );
        world.entity_mut(button).insert(crate::sand::Borderless);
        if index == draft.selected {
            world.entity_mut(button).insert((
                SelectedOption,
                crate::token_style::border(crate::tokens::Token::Accent),
            ));
        }
    }
}

#[derive(Clone)]
struct Choose {
    input: Entity,
    uid: String,
}
impl Action for Choose {
    fn apply(&self, world: &mut World, _: Entity) {
        let Some(mut draft) = world.entity_mut(self.input).take::<Draft>() else {
            return;
        };
        let editor = world.get::<EditableText>(self.input).unwrap();
        let text = editor.value().to_string();
        let caret = editor.editor.raw_selection().focus().index();
        reconcile(&mut draft, &text);
        let range = query(&text, caret);
        if let Some(mut range) = range
            && let Some(row) = draft.options.iter().find(|row| row.uid == self.uid)
        {
            range.end += text[range.end..]
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '-')
                .map(char::len_utf8)
                .sum::<usize>();
            if text[range.end..].starts_with(' ') {
                range.end += 1;
            }
            let label = format!(
                "@{}",
                if row.slug.is_empty() {
                    if row.head.is_empty() {
                        &row.uid
                    } else {
                        &row.head
                    }
                } else {
                    &row.slug
                }
            )
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
            let mut value = text;
            value.replace_range(range.clone(), &format!("{label} "));
            reconcile(&mut draft, &value);
            let end = range.start + label.len();
            draft.links.push(Mention {
                range: range.start..end,
                uid: self.uid.clone(),
            });
            draft.links.sort_by_key(|link| link.range.start);
            if world.contains_resource::<FontCx>() && world.contains_resource::<LayoutCx>() {
                let mut fonts = world.remove_resource::<FontCx>().unwrap();
                let mut layout = world.remove_resource::<LayoutCx>().unwrap();
                let mut text = world.get_mut::<EditableText>(self.input).unwrap();
                let mut driver = text.editor.driver(&mut fonts.context, &mut layout.0);
                driver.select_byte_range(range.start, range.end);
                driver.insert_or_replace_selection(&format!("{label} "));
                world.insert_resource(fonts);
                world.insert_resource(layout);
            } else {
                world
                    .get_mut::<EditableText>(self.input)
                    .unwrap()
                    .editor
                    .set_text(&value);
            }
            draft.query = None;
            draft.dismissed = Some((value, end + 1));
            draft.options.clear();
            if let Some(popup) = draft.popup.take() {
                world.despawn(popup);
            }
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
                focus.set(self.input, FocusCause::Pressed);
            }
        }
        world.entity_mut(self.input).insert(draft);
    }
}

pub(super) fn keys(world: &mut World) -> bool {
    let Some(input) = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
    else {
        return false;
    };
    let Some(draft) = world.get::<Draft>(input) else {
        return false;
    };
    if draft.query.is_none() {
        return false;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return false;
    };
    if keys.any_pressed([
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]) {
        return false;
    }
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    let escape = keys.just_pressed(KeyCode::Escape);
    let step = i32::from(keys.just_pressed(KeyCode::ArrowDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp));
    if !enter && !escape && step == 0 {
        return false;
    }
    let mut text = world.get_mut::<EditableText>(input).unwrap();
    if text.is_composing()
        || text.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                bevy::text::TextEdit::ImeCommit { .. } | bevy::text::TextEdit::ImeSetCompose { .. }
            )
        })
    {
        return false;
    }
    text.pending_edits.retain(|edit| !matches!(edit,
        bevy::text::TextEdit::Insert(value) if enter && (value.as_str() == "\n" || value.as_str() == "\r"))
        && !(step != 0 && matches!(edit, bevy::text::TextEdit::Up(_) | bevy::text::TextEdit::Down(_))));
    let value = text.value().to_string();
    let caret = text.editor.raw_selection().focus().index();
    let mut draft = world.get_mut::<Draft>(input).unwrap();
    if escape {
        draft.dismissed = Some((value, caret));
    } else if enter && draft.options.is_empty() && (draft.loaded || draft.error.is_some()) {
        draft.dismissed = Some((value, caret));
        return false;
    } else if !draft.options.is_empty() {
        draft.selected =
            (draft.selected as i32 + step).rem_euclid(draft.options.len() as i32) as usize;
        let uid = draft.options[draft.selected].uid.clone();
        if enter {
            Choose { input, uid }.apply(world, input);
        }
    }
    true
}

pub(super) fn paint(world: &mut World) {
    let selected: Vec<_> = world
        .query_filtered::<(Entity, &ChildOf, &ComputedNode), With<SelectedOption>>()
        .iter(world)
        .map(|(entity, parent, _)| (entity, parent.parent()))
        .collect();
    for (entity, popup) in selected {
        let Some(children) = world.get::<Children>(popup) else {
            continue;
        };
        let mut top = 0.0;
        let mut selected = None;
        for child in children.iter() {
            let height = world
                .get::<ComputedNode>(child)
                .map_or(0.0, |node| node.size.y * node.inverse_scale_factor);
            if world.get::<SelectedOption>(child).is_some() {
                selected = Some((top, top + height));
            }
            top += height;
        }
        let height = world.get::<ComputedNode>(popup).map_or(0.0, |node| {
            node.content_box().height() * node.inverse_scale_factor
        });
        if height > 0.0
            && let Some((top, bottom)) = selected
            && let Some(mut scroll) = world.get_mut::<ScrollPosition>(popup)
        {
            if top < scroll.0.y {
                scroll.0.y = top;
            } else if bottom > scroll.0.y + height {
                scroll.0.y = bottom - height;
            }
            world.entity_mut(entity).remove::<SelectedOption>();
        }
    }
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() {
        return;
    }
    let mut old: HashMap<_, _> = world
        .query::<(Entity, &InlineLink)>()
        .iter(world)
        .map(|(entity, link)| {
            (
                (
                    link.input,
                    link.range.start,
                    link.range.end,
                    link.line,
                    link.uid.clone(),
                ),
                entity,
            )
        })
        .collect();
    let inputs: Vec<_> = world
        .query::<(Entity, &Draft)>()
        .iter(world)
        .filter(|(_, draft)| !draft.links.is_empty())
        .map(|(input, draft)| (input, draft.links.clone()))
        .collect();
    for (input, links) in inputs {
        let Some(text) = world.get::<EditableText>(input) else {
            continue;
        };
        let mut editor = text.editor.clone();
        let computed = world
            .get::<ComputedNode>(input)
            .copied()
            .unwrap_or_default();
        let scroll = world
            .get::<bevy::ui::widget::TextScroll>(input)
            .map_or(Vec2::ZERO, |scroll| scroll.0);
        let mut fonts = world.remove_resource::<FontCx>().unwrap();
        let mut layout = world.remove_resource::<LayoutCx>().unwrap();
        let mut boxes = Vec::new();
        for link in links {
            editor
                .driver(&mut fonts.context, &mut layout.0)
                .select_byte_range(link.range.start, link.range.end);
            for (rect, line) in editor.selection_geometry() {
                boxes.push((link.clone(), rect, line));
            }
        }
        world.insert_resource(fonts);
        world.insert_resource(layout);
        for (link, rect, line) in boxes {
            let min = (Vec2::new(rect.x0 as f32, rect.y0 as f32) - scroll).max(Vec2::ZERO);
            let max = (Vec2::new(rect.x1 as f32, rect.y1 as f32) - scroll)
                .min(computed.content_box().size());
            if max.x <= min.x || max.y <= min.y {
                continue;
            }
            let scale = computed.inverse_scale_factor;
            let node = Node {
                position_type: PositionType::Absolute,
                left: px((computed.padding.min_inset.x + min.x) * scale),
                top: px((computed.padding.min_inset.y + min.y) * scale),
                width: px((max.x - min.x) * scale),
                height: px((max.y - min.y) * scale),
                border: UiRect::bottom(px(1)),
                ..default()
            };
            let key = (
                input,
                link.range.start,
                link.range.end,
                line,
                link.uid.clone(),
            );
            if let Some(entity) = old.remove(&key) {
                if world.get::<Node>(entity) != Some(&node) {
                    world.entity_mut(entity).insert(node);
                }
            } else {
                let mut accessibility = accesskit::Node::new(accesskit::Role::Link);
                accessibility.set_label(format!(
                    "Open {}",
                    &world
                        .get::<EditableText>(input)
                        .unwrap()
                        .value()
                        .to_string()[link.range.clone()]
                ));
                world
                    .spawn((
                        node,
                        InlineLink {
                            input,
                            range: link.range,
                            line,
                            uid: link.uid.clone(),
                        },
                        crate::token_style::border(crate::tokens::Token::Accent),
                        crate::icons::Tooltip("Open Record".into()),
                        AccessibilityNode::from(accessibility),
                        bevy::input_focus::tab_navigation::TabIndex(0),
                        ChildOf(input),
                    ))
                    .observe(
                        |mut event: On<Pointer<Click>>,
                         links: Query<&InlineLink>,
                         mut commands: Commands| {
                            if event.button != bevy::picking::pointer::PointerButton::Primary {
                                return;
                            }
                            if let Ok(link) = links.get(event.entity) {
                                let (input, uid) = (link.input, link.uid.clone());
                                event.propagate(false);
                                commands.queue(move |world: &mut World| open(world, input, &uid));
                            }
                        },
                    )
                    .observe(
                        |mut event: On<
                            bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>,
                        >,
                         links: Query<&InlineLink>,
                         mut commands: Commands| {
                            if event.input.state.is_pressed()
                                && matches!(
                                    event.input.key_code,
                                    KeyCode::Enter | KeyCode::NumpadEnter
                                )
                                && let Ok(link) = links.get(event.focused_entity)
                            {
                                let (input, uid) = (link.input, link.uid.clone());
                                event.propagate(false);
                                commands.queue(move |world: &mut World| open(world, input, &uid));
                            }
                        },
                    );
            }
        }
    }
    for entity in old.into_values() {
        world.despawn(entity);
    }
}

fn open(world: &mut World, input: Entity, uid: &str) {
    let binding = world
        .query::<&ThreadForm>()
        .iter(world)
        .find(|form| form.input == input)
        .map(|form| form.binding.clone());
    if let Some(binding) = binding {
        crate::description::Link {
            reference: uid.into(),
            context: crate::description::Context {
                owner: input,
                source: binding.source,
            },
        }
        .apply(world, input);
    }
}

#[cfg(test)]
mod tests {
    use super::super::integration::{composer, pump, setup};
    use super::*;

    #[test]
    fn search_uses_the_caret_and_ignores_email_and_code() {
        assert_eq!(query("Olá @dévelop", 13), Some(5..13));
        assert_eq!(query("(@slug_1)", 8), Some(1..8));
        assert_eq!(query("@one @two", 4), Some(0..4));
        for text in ["mail@host", "`@code", "```\n@code", "@two words", "hello"] {
            assert_eq!(query(text, text.len()), None, "{text}");
        }
        assert_eq!(query("`code` @", 8), Some(7..8));
    }

    #[test]
    fn stable_links_follow_unicode_edits_and_drop_when_the_label_changes() {
        let mut draft = Draft {
            text: "Hi @dévelop and @person".into(),
            links: vec![
                Mention {
                    range: 3..12,
                    uid: "r_agent".into(),
                },
                Mention {
                    range: 17..24,
                    uid: "r_person".into(),
                },
            ],
            ..default()
        };
        reconcile(&mut draft, "😀 Hi @dévelop and @person");
        assert_eq!(draft.links[0].range, 8..17);
        reconcile(&mut draft, "😀 Hi @develop and @person");
        assert_eq!(
            draft.links,
            vec![Mention {
                range: 21..28,
                uid: "r_person".into()
            }]
        );
        reconcile(&mut draft, "😀 Hi @develop and @persons");
        assert!(draft.links.is_empty());
    }

    #[test]
    fn selected_links_serialize_without_exposing_markup_in_the_draft() {
        let mut world = World::new();
        let text = "Ask @Jane [work] and @dev";
        let input = world
            .spawn(Draft {
                text: text.into(),
                links: vec![
                    Mention {
                        range: 4..16,
                        uid: "r_person".into(),
                    },
                    Mention {
                        range: 21..25,
                        uid: "r_agent".into(),
                    },
                ],
                ..default()
            })
            .id();
        assert_eq!(
            body(&mut world, input, text),
            "Ask [@Jane \\[work\\]](record:r_person) and [@dev](record:r_agent)"
        );
        assert_eq!(world.get::<Draft>(input).unwrap().text, text);
        assert_eq!(body(&mut world, input, ""), "");
        assert!(world.get::<Draft>(input).unwrap().links.is_empty());
    }

    fn type_at(world: &mut World, input: Entity, text: &str, caret: usize) {
        world.init_resource::<FontCx>();
        world.init_resource::<LayoutCx>();
        let mut fonts = world.remove_resource::<FontCx>().unwrap();
        let mut layout = world.remove_resource::<LayoutCx>().unwrap();
        let font = world
            .resource::<Assets<Font>>()
            .get(&world.resource::<crate::theme::Typography>().0)
            .unwrap();
        fonts.collection.register_fonts(font.data.clone(), None);
        fonts.set_sans_serif_family("Lato").unwrap();
        let mut editor = world.get_mut::<EditableText>(input).unwrap();
        editor.editor.set_text(text);
        editor
            .editor
            .driver(&mut fonts.context, &mut layout.0)
            .move_to_byte(caret);
        world.insert_resource(fonts);
        world.insert_resource(layout);
        world
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
    }

    #[test]
    fn selection_replaces_the_whole_token_and_keeps_the_rest_of_the_message() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<InputFocus>();
        let input = world
            .spawn(crate::sand::text_editor(
                "",
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .id();
        type_at(&mut world, input, "Ask @janet later", 7);
        world.entity_mut(input).insert(Draft {
            options: vec![Candidate {
                uid: "r_person".into(),
                slug: String::new(),
                head: "Jane [work]".into(),
            }],
            ..default()
        });
        Choose {
            input,
            uid: "r_person".into(),
        }
        .apply(&mut world, input);
        let text = world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string();
        assert_eq!(text, "Ask @Jane [work] later");
        assert_eq!(
            body(&mut world, input, &text),
            "Ask [@Jane \\[work\\]](record:r_person) later"
        );
        assert_eq!(
            world
                .get::<EditableText>(input)
                .unwrap()
                .editor
                .raw_selection()
                .focus()
                .index(),
            17
        );
    }

    #[tokio::test]
    async fn record_picker_includes_people_and_agents_selects_without_sending_and_opens_links() {
        let (mut app, engine, _host, _root, agent) = setup(false).await;
        let mut ids = vec![agent.clone()];
        for (kind, slug, head) in [
            (nucleus::RecordKind::Plain, "mention-task", "Task"),
            (nucleus::RecordKind::Person, "mention-person", "Person"),
        ] {
            ids.push(
                engine
                    .act(
                        engine::actions::Action::CreateRecord {
                            slug: Some(slug.into()),
                            kind,
                            head: head.into(),
                            body: String::new(),
                            quantity: 0.0,
                        },
                        None,
                    )
                    .await
                    .unwrap()
                    .created
                    .unwrap(),
            );
        }
        engine
            .act(
                engine::actions::Action::SetSlug {
                    target: agent.clone(),
                    slug: Some("mention-agent".into()),
                },
                None,
            )
            .await
            .unwrap();
        let castle = app
            .world_mut()
            .query_filtered::<Entity, With<ThreadCastle>>()
            .single(app.world())
            .unwrap();
        super::super::controls::Add.apply(app.world_mut(), castle);
        pump(&mut app, |world| composer(world).is_some()).await;
        let (form, input, _) = composer(app.world_mut()).unwrap();
        type_at(app.world_mut(), input, "Hello @mention", 14);
        pump(&mut app, |world| {
            world.get::<Draft>(input).unwrap().options.len() == 3
        })
        .await;
        for uid in &ids {
            assert!(
                app.world()
                    .get::<Draft>(input)
                    .unwrap()
                    .options
                    .iter()
                    .any(|row| &row.uid == uid)
            );
        }
        let popup = app.world().get::<Draft>(input).unwrap().popup.unwrap();
        let option = app.world().get::<Children>(popup).unwrap()[0];
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(option, FocusCause::Pressed);
        app.update();
        assert!(
            app.world().get_entity(option).is_ok(),
            "Pressing a picker option must keep it alive until the click"
        );
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        assert_eq!(app.world().get::<Draft>(input).unwrap().selected, 1);
        type_at(app.world_mut(), input, "Hello @mention-per", 18);
        pump(&mut app, |world| {
            world.get::<Draft>(input).unwrap().options.len() == 1
        })
        .await;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        let text = app
            .world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string();
        assert_eq!(text, "Hello @mention-person ");
        assert!(
            app.world()
                .get::<ThreadForm>(form)
                .unwrap()
                .pending
                .is_none()
        );
        assert_eq!(
            app.world().get::<Draft>(input).unwrap().links[0].uid,
            ids[2]
        );
        let encoded = body(app.world_mut(), input, &text);
        assert_eq!(
            encoded,
            format!("Hello [@mention-person](record:{}) ", ids[2])
        );
        let count = app
            .world_mut()
            .query::<&crate::area::InfluenceArea>()
            .iter(app.world())
            .count();
        let link = app
            .world_mut()
            .query::<(Entity, &InlineLink)>()
            .iter(app.world())
            .find(|(_, link)| link.input == input)
            .map(|(entity, _)| entity)
            .expect("The composer must render the selected mention as a clickable link");
        app.world_mut().trigger(Pointer::new(
            bevy::picking::pointer::PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ZERO,
            },
            bevy::picking::events::Click {
                button: bevy::picking::pointer::PointerButton::Primary,
                hit: bevy::picking::backend::HitData::new(link, 0.0, None, None),
                duration: std::time::Duration::ZERO,
                count: 1,
            },
            link,
        ));
        app.world_mut().flush();
        assert_eq!(
            app.world_mut()
                .query::<&crate::area::InfluenceArea>()
                .iter(app.world())
                .count(),
            count + 1
        );
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(input, FocusCause::Navigated);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        pump(&mut app, |world| {
            world.get::<ThreadForm>(form).unwrap().pending.is_none()
                && world.query::<&Message>().iter(world).any(|message| {
                    world
                        .get::<crate::description::Description>(message.preview)
                        .is_some_and(|preview| preview.source == encoded)
                })
        })
        .await;
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            ""
        );
    }
}
