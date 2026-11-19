use super::*;
mod links;
mod popup;
use bevy::{
    input_focus::{FocusCause, InputFocus},
    text::{FontCx, LayoutCx},
};
use cell::{ClientMessage, ServerMessage};
use std::{ops::Range, sync::Arc};

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

struct PreparedCandidate {
    row: Candidate,
    slug: String,
    head: String,
    uid: String,
}

impl PreparedCandidate {
    fn new(row: Candidate) -> Self {
        Self {
            slug: row.slug.to_lowercase(),
            head: row.head.to_lowercase(),
            uid: row.uid.to_lowercase(),
            row,
        }
    }
}

struct Rendered {
    options: Arc<[Candidate]>,
    selected: usize,
    error: Option<String>,
    loaded: bool,
    form: Entity,
    choices: Vec<(Entity, Entity)>,
    children: Vec<Entity>,
    font: Handle<Font>,
}

struct QueryAt {
    caret: usize,
    range: Option<Range<usize>>,
}

#[derive(Component, Default)]
struct Draft {
    text: String,
    links: Vec<Mention>,
    rows: Vec<PreparedCandidate>,
    options: Arc<[Candidate]>,
    filter: Option<String>,
    query: Option<Range<usize>>,
    query_at: Option<QueryAt>,
    dismissed: Option<(String, usize)>,
    selected: usize,
    popup: Option<Entity>,
    subscription: Option<String>,
    error: Option<String>,
    loaded: bool,
    rendered: Option<Rendered>,
}

#[derive(Resource, Default)]
struct Subscriptions(HashMap<String, (Entity, crate::native::cell_bridge::Sender)>);

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
    draft.query_at = None;
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

fn current_query(draft: &mut Mut<Draft>, caret: usize) -> Option<Range<usize>> {
    if draft
        .query_at
        .as_ref()
        .is_none_or(|query| query.caret != caret)
    {
        let range = query(&draft.text, caret);
        draft.query_at = Some(QueryAt { caret, range });
        nucleus::performance::add("mention-query-scans", 1);
    }
    draft.query_at.as_ref().unwrap().range.clone()
}

fn candidates(rows: &[PreparedCandidate], query: &str) -> Arc<[Candidate]> {
    let _preparation = nucleus::performance::Span::new("mention-candidate-selection");
    let query = query.to_lowercase();
    let mut result: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.slug.contains(&query) || row.head.contains(&query) || row.uid.contains(&query)
        })
        .collect();
    result.sort_by(|left, right| {
        let rank = |row: &PreparedCandidate| (row.slug != query, !row.slug.starts_with(&query));
        rank(left)
            .cmp(&rank(right))
            .then_with(|| left.head.cmp(&right.head))
            .then_with(|| left.row.uid.cmp(&right.row.uid))
    });
    result.truncate(12);
    nucleus::performance::add("mention-candidates-materialized", result.len() as u64);
    result
        .into_iter()
        .map(|prepared| prepared.row.clone())
        .collect::<Vec<_>>()
        .into()
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

fn candidate_rows(rows: &[Value]) -> impl Iterator<Item = (&str, &str, &str)> {
    rows.iter().filter_map(|row| {
        Some((
            row["uid"].as_str()?,
            row["slug"].as_str().unwrap_or(""),
            row["head"].as_str().unwrap_or(""),
        ))
    })
}

pub(crate) fn receive_message(world: &mut World, event: &ServerMessage) {
    let (id, rows, error) = match event {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, Some(rows), None)
        }
        ServerMessage::Error { id, message, .. } => (id, None, Some(message.as_str())),
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
    let _reply = nucleus::performance::Span::new("mention-catalog-preparation");
    let mut changed = false;
    if draft.error.as_deref() != error {
        draft.error = error.map(str::to_owned);
        changed = true;
    }
    if let Some(rows) = rows {
        if !draft.loaded {
            draft.loaded = true;
            changed = true;
        }
        if !draft
            .rows
            .iter()
            .map(|row| {
                (
                    row.row.uid.as_str(),
                    row.row.slug.as_str(),
                    row.row.head.as_str(),
                )
            })
            .eq(candidate_rows(rows))
        {
            draft.rows = candidate_rows(rows)
                .map(|(uid, slug, head)| {
                    PreparedCandidate::new(Candidate {
                        uid: uid.into(),
                        slug: slug.into(),
                        head: head.into(),
                    })
                })
                .collect();
            nucleus::performance::add("mention-catalog-rows-prepared", draft.rows.len() as u64);
            changed = true;
        }
    }
    if changed && draft.filter.is_some() {
        draft.filter = None;
    }
}

pub(super) fn sync(world: &mut World) {
    let _sync = nucleus::performance::Span::new("mention-input-preparation");
    world
        .run_system_cached(sync_current)
        .expect("synchronize current mention inputs");
}

fn sync_current(
    world: &mut World,
    forms: &mut QueryState<(Entity, &'static ThreadForm)>,
    editors: &mut QueryState<
        (&'static mut Draft, &'static EditableText),
        bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
    >,
    drafts: &mut QueryState<&'static Draft>,
    mut pending_forms: Local<Vec<(Entity, Entity, bool)>>,
) {
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    pending_forms.clear();
    pending_forms.extend(
        forms
            .iter(world)
            .filter(|(_, form)| form.thread.is_some())
            .map(|(form, state)| (form, state.input, state.pending.is_some())),
    );
    if pending_forms.is_empty() {
        pending_forms.shrink_to_fit();
    }
    for (form, input, pending) in pending_forms.drain(..) {
        let Some(draft) = world.get::<Draft>(input) else {
            continue;
        };
        let mut picker_focused = false;
        if let Some(popup) = draft.popup
            && focused != Some(input)
        {
            let mut focus = focused;
            while let Some(entity) = focus {
                if popup == entity {
                    picker_focused = true;
                    break;
                }
                focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
        }
        {
            let (mut draft, editor) = editors
                .get_mut(world, input)
                .expect("mention composer has an editor");
            let caret = editor.editor.raw_selection().focus().index();
            let value = editor.value();
            if value != draft.text.as_str() {
                reconcile(&mut draft, &value.to_cow());
            }
            let active = if (focused == Some(input) || picker_focused)
                && !pending
                && !editor.is_composing()
                && !draft
                    .dismissed
                    .as_ref()
                    .is_some_and(|(text, index)| text == &draft.text && *index == caret)
            {
                current_query(&mut draft, caret).filter(|range| {
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
                draft.query = active;
            }
        }
        if world.get::<Draft>(input).unwrap().query.is_some() {
            subscribe(world, form, input);
            {
                let mut draft = world.get_mut::<Draft>(input).unwrap();
                let range = draft.query.as_ref().unwrap();
                let filter = &draft.text[range.start + 1..range.end];
                if draft.filter.as_deref() != Some(filter) {
                    let options = candidates(&draft.rows, filter);
                    let filter = filter.to_owned();
                    if draft.options != options {
                        draft.options = options;
                    }
                    draft.filter = Some(filter);
                }
                let selected = draft.selected.min(draft.options.len().saturating_sub(1));
                if draft.selected != selected {
                    draft.selected = selected;
                }
            }
            popup::render(world, form, input);
        } else {
            let mut draft = world.get_mut::<Draft>(input).unwrap();
            if !draft.options.is_empty() {
                draft.options = Arc::default();
            }
            if draft.filter.is_some() {
                draft.filter = None;
            }
            if draft.subscription.is_some() {
                draft.subscription = None;
            }
            if draft.rows.capacity() != 0 {
                draft.rows = Vec::new();
            }
            if draft.rendered.is_some() {
                draft.rendered = None;
            }
            let popup = draft.popup;
            if popup.is_some() {
                draft.popup = None;
            }
            if let Some(popup) = popup {
                world.despawn(popup);
            }
        }
    }
    if world
        .get_resource::<Subscriptions>()
        .is_some_and(|subscriptions| !subscriptions.0.is_empty())
    {
        world.resource_scope(|world, mut subscriptions: Mut<Subscriptions>| {
            let active: HashSet<_> = drafts
                .iter(world)
                .filter_map(|draft| draft.subscription.as_deref())
                .collect();
            subscriptions.0.retain(|id, (_, sender)| {
                active.contains(id.as_str())
                    || match sender.try_reserve() {
                        Ok(permit) => {
                            permit.send(ClientMessage::Unsubscribe { id: id.clone() });
                            false
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Full(())) => {
                            !sender.is_closed()
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => false,
                    }
            });
        });
    }
}

fn subscription_error(world: &mut World, input: Entity, error: &str) {
    let mut draft = world.get_mut::<Draft>(input).unwrap();
    if draft.error.as_deref() != Some(error) {
        draft.error = Some(error.into());
    }
}

fn subscribe(world: &mut World, form: Entity, input: Entity) {
    if world.get::<Draft>(input).unwrap().subscription.is_some() {
        return;
    }
    let Some(sender) = world
        .get::<ThreadForm>(form)
        .and_then(|form| crate::native::protein_area::editor_sender(world, &form.binding))
    else {
        subscription_error(world, input, "Record search is disconnected.");
        return;
    };
    let permit = match sender.try_reserve() {
        Ok(permit) => permit,
        Err(_) => {
            subscription_error(world, input, "Record search is busy. Try again.");
            return;
        }
    };
    let id = nucleus::new_uid("mention");
    let protein = protein::Protein {
        source: protein::Source::Record,
        filter: Vec::new(),
        fields: Some(["uid", "head", "slug"].map(str::to_owned).into()),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    };
    permit.send(ClientMessage::Subscribe {
        id: id.clone(),
        protein,
    });
    world.init_resource::<Subscriptions>();
    world
        .resource_mut::<Subscriptions>()
        .0
        .insert(id.clone(), (input, sender));
    let mut draft = world.get_mut::<Draft>(input).unwrap();
    draft.subscription = Some(id);
    draft.error = None;
    draft.loaded = false;
}

#[derive(Clone)]
struct Choose {
    input: Entity,
    uid: String,
}
impl Action for Choose {
    fn apply(&self, world: &mut World, _: Entity) {
        if world.get::<Draft>(self.input).is_none() {
            return;
        }
        let editor = world.get::<EditableText>(self.input).unwrap();
        let text = editor.value().to_string();
        let caret = editor.editor.raw_selection().focus().index();
        let (range, label, value, end) = {
            let mut draft = world.get_mut::<Draft>(self.input).unwrap();
            if draft.text != text {
                reconcile(&mut draft, &text);
            }
            let Some(mut range) = current_query(&mut draft, caret) else {
                return;
            };
            let Some(row) = draft.options.iter().find(|row| row.uid == self.uid) else {
                return;
            };
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
            let insertion = format!("{label} ");
            let mut value = text;
            value.replace_range(range.clone(), &insertion);
            reconcile(&mut draft, &value);
            let end = range.start + label.len();
            draft.links.push(Mention {
                range: range.start..end,
                uid: self.uid.clone(),
            });
            draft.links.sort_by_key(|link| link.range.start);
            (range, insertion, value, end)
        };
        if world.contains_resource::<FontCx>() && world.contains_resource::<LayoutCx>() {
            let mut fonts = world.remove_resource::<FontCx>().unwrap();
            let mut layout = world.remove_resource::<LayoutCx>().unwrap();
            let mut text = world.get_mut::<EditableText>(self.input).unwrap();
            let mut driver = text.editor.driver(&mut fonts.context, &mut layout.0);
            driver.select_byte_range(range.start, range.end);
            driver.insert_or_replace_selection(&label);
            world.insert_resource(fonts);
            world.insert_resource(layout);
        } else {
            world
                .get_mut::<EditableText>(self.input)
                .unwrap()
                .editor
                .set_text(&value);
        }
        let mut draft = world.get_mut::<Draft>(self.input).unwrap();
        draft.query = None;
        draft.dismissed = Some((value, end + 1));
        draft.options = Arc::default();
        let popup = draft.popup.take();
        if let Some(popup) = popup {
            world.despawn(popup);
        }
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.set(self.input, FocusCause::Pressed);
        }
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
    let dismiss = escape || {
        let draft = world.get::<Draft>(input).unwrap();
        enter && draft.options.is_empty() && (draft.loaded || draft.error.is_some())
    };
    if dismiss {
        let text = world.get::<EditableText>(input).unwrap();
        let value = text.value().to_string();
        let caret = text.editor.raw_selection().focus().index();
        world.get_mut::<Draft>(input).unwrap().dismissed = Some((value, caret));
        return escape;
    }
    let mut draft = world.get_mut::<Draft>(input).unwrap();
    if !draft.options.is_empty() {
        let selected =
            (draft.selected as i32 + step).rem_euclid(draft.options.len() as i32) as usize;
        if draft.selected != selected {
            draft.selected = selected;
        }
        if enter {
            let uid = draft.options[selected].uid.clone();
            Choose { input, uid }.apply(world, input);
        }
    }
    true
}

pub(super) fn paint(world: &mut World) {
    links::paint(world);
}

fn open(world: &mut World, input: Entity, uid: &str) {
    let source = world
        .run_system_cached_with(source_for_input, input)
        .expect("find current mention source");
    if let Some(source) = source {
        crate::native::description::Link {
            reference: uid.into(),
            context: crate::native::description::Context {
                owner: input,
                source,
            },
        }
        .apply(world, input);
    }
}

fn source_for_input(In(input): In<Entity>, forms: Query<&ThreadForm>) -> Option<Source> {
    forms
        .iter()
        .find(|form| form.input == input)
        .map(|form| form.binding.source.clone())
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
            .get(&world.resource::<crate::native::theme::Typography>().0)
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
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<InputFocus>();
        let input = world
            .spawn(crate::native::sand::text_editor(
                "",
                world.resource::<crate::native::theme::Typography>(),
                0,
            ))
            .id();
        type_at(&mut world, input, "Ask @janet later", 7);
        world.entity_mut(input).insert(Draft {
            options: vec![Candidate {
                uid: "r_person".into(),
                slug: String::new(),
                head: "Jane [work]".into(),
            }]
            .into(),
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
            .query::<&crate::native::area::InfluenceArea>()
            .iter(app.world())
            .count();
        paint(app.world_mut());
        let link = app
            .world_mut()
            .query::<(Entity, &InlineLink)>()
            .iter(app.world())
            .find(|(_, link)| link.input == input)
            .map(|(entity, _)| entity)
            .expect("The composer must render the selected mention as a clickable link");
        app.world_mut().trigger(PointerClick {
            entity: link,
            pointer: Pointer::new(bevy::picking::pointer::PointerId::Mouse, bevy::picking::pointer::Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ZERO,
            }),
            button: bevy::picking::pointer::PointerButton::Primary,
                hit: bevy::picking::backend::HitData::new(link, 0.0, None, None),
                duration: std::time::Duration::ZERO,
                count: 1,
        });
        app.world_mut().flush();
        assert_eq!(
            app.world_mut()
                .query::<&crate::native::area::InfluenceArea>()
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
                        .get::<crate::native::description::Description>(message.preview)
                        .is_some_and(|preview| preview.source == encoded.trim())
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
