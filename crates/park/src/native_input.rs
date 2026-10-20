use bevy::{asset::AssetId, ecs::entity_disabling::Disabled, prelude::*, text::{EditableText, FontCx, LayoutCx, PreeditCursor, TextEdit}};
use lince_interface::native;
use std::{collections::{HashMap, VecDeque}, hash::Hasher, num::NonZeroUsize};

#[cfg(feature = "nested")]
pub(crate) mod client;
pub mod transfer;

pub const MAX_SURROUNDING: usize = 4000;
pub const MAX_TEXT: usize = 4000;

#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub owner: u64,
    pub revision: u64,
    pub from_ime: bool,
    pub selection: [usize; 2],
    pub surrounding: String,
    pub cursor: usize,
    pub anchor: usize,
    pub sensitive: bool,
    pub multiline: bool,
    pub caret: [i32; 4],
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub context: Editor,
    pub preedit: Option<(String, Option<(usize, usize)>)>,
    pub commit: Option<String>,
    pub delete: Option<(u32, u32)>,
}

#[derive(Resource, Default)]
struct Pending {
    edits: VecDeque<Edit>,
    focus: Option<Entity>,
    limits: HashMap<Entity, Option<usize>>,
    ime: Option<(Entity, u64)>,
}

pub(crate) fn install(app: &mut App) {
    transfer::install(app);
    app.init_resource::<Pending>().add_systems(
        PostUpdate,
        (apply.before(bevy::text::EditableTextSystems),
            finish.after(bevy::text::EditableTextSystems).before(native::record_binding::SyncBindings)),
    );
}

pub(crate) fn enqueue(world: &mut World, edit: Edit) {
    let mut pending = world.resource_mut::<Pending>();
    if pending.edits.len() < 32 { pending.edits.push_back(edit); }
}

pub(crate) fn cancel(world: &mut World) {
    world.resource_mut::<Pending>().edits.clear();
    world.resource_mut::<Pending>().ime = None;
    if let Some(entity) = world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get())
        && let Some(mut input) = world.get_mut::<EditableText>(entity)
        && input.is_composing()
    { input.queue_edit(TextEdit::clear_ime_compose()); }
}

fn allowed(world: &World, entity: Entity) -> bool {
    world.get::<EditableText>(entity).is_some()
        && world.get::<Disabled>(entity).is_none()
        && world.get::<bevy::ui::InteractionDisabled>(entity).is_none()
        && world.get::<InheritedVisibility>(entity).is_none_or(|visibility| visibility.get())
        && (!world.get::<native::record_binding::TextBinding>(entity).is_some()
            || native::record_binding::can_edit(world, entity))
}

fn text_state(world: &World, entity: Entity, caret: [i32; 4]) -> Option<Editor> {
    if !allowed(world, entity) { return None; }
    let input = world.get::<EditableText>(entity)?;
    let sensitive = native::protected_text_input(world, entity);
    let selection = input.editor.raw_selection();
    let compose = input.editor.raw_compose();
    let position = |value: usize| compose.as_ref().map_or(value, |range| {
        value.saturating_sub(value.saturating_sub(range.start).min(range.len()))
    });
    let cursor = position(selection.focus().index());
    let anchor = position(selection.anchor().index());
    let raw = input.editor.raw_text().as_bytes();
    let start = selection.focus().index().saturating_sub(MAX_SURROUNDING / 2).min(raw.len());
    let end = raw.len().min(start.saturating_add(MAX_SURROUNDING));
    let mut revision = std::hash::DefaultHasher::new();
    revision.write_usize(raw.len());
    revision.write_usize(selection.anchor().index());
    revision.write_usize(selection.focus().index());
    revision.write(&raw[start..end]);
    if let Some(compose) = compose {
        revision.write_usize(compose.start);
        revision.write_usize(compose.end);
        revision.write(&raw[compose.start..compose.end.min(compose.start.saturating_add(MAX_TEXT))]);
    }
    let (surrounding, cursor, anchor) = if sensitive {
        (String::new(), 0, 0)
    } else {
        let mut parts = input.value().into_iter();
        let first = parts.next()?;
        let second = parts.next()?;
        surrounding(first, second, cursor, anchor)?
    };
    let revision = revision.finish();
    let from_ime = world.get_resource::<Pending>().is_some_and(|pending| pending.ime == Some((entity, revision)));
    Some(Editor { owner: entity.to_bits(), revision, from_ime, selection: [selection.anchor().index(), selection.focus().index()], surrounding, cursor, anchor, sensitive,
        multiline: input.allow_newlines, caret })
}

pub(crate) fn capture(world: &World, output: AssetId<Image>, offset: [f64; 2]) -> Option<Editor> {
    let entity = world.get_resource::<bevy::input_focus::InputFocus>()?.get()?;
    let camera = world.get::<bevy::ui::ComputedUiTargetCamera>(entity)?.get()?;
    let bevy::camera::RenderTarget::Image(target) = world.get::<bevy::camera::RenderTarget>(camera)? else { return None };
    if target.handle.id() != output { return None; }
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let area = world.get::<EditableText>(entity)?.editor.ime_cursor_area();
    let caret = caret_rectangle(transform, [area.x0 as f32, area.y0 as f32, area.x1 as f32, area.y1 as f32],
        node.size(), node.inverse_scale_factor(), offset)?;
    text_state(world, entity, caret)
}

fn caret_rectangle(transform: &UiGlobalTransform, area: [f32; 4], size: Vec2, scale: f32, offset: [f64; 2]) -> Option<[i32; 4]> {
    if !area.iter().all(|value| value.is_finite()) || !size.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let corners = [Vec2::new(area[0], area[1]), Vec2::new(area[2], area[1]),
        Vec2::new(area[2], area[3]), Vec2::new(area[0], area[3])]
        .map(|point| transform.transform_point2(point * scale - size * 0.5));
    if !corners.iter().all(|point| point.is_finite()) { return None; }
    let low = corners.into_iter().fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let high = corners.into_iter().fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    let caret = [f64::from(low.x) + offset[0], f64::from(low.y) + offset[1],
        f64::from(high.x - low.x).max(1.0), f64::from(high.y - low.y).max(1.0)];
    if !caret.iter().all(|value| value.is_finite() && value.abs() <= 131072.0) { return None; }
    Some(caret.map(|value| value.round() as i32))
}

fn surrounding(first: &str, second: &str, cursor: usize, anchor: usize) -> Option<(String, usize, usize)> {
    let length = first.len().checked_add(second.len())?;
    if cursor > length || anchor > length { return None; }
    let low = cursor.min(anchor);
    let high = cursor.max(anchor);
    if high - low > MAX_SURROUNDING { return Some((String::new(), 0, 0)); }
    let remaining = MAX_SURROUNDING - (high - low);
    let mut start = low.saturating_sub(remaining / 2);
    let mut end = length.min(start + MAX_SURROUNDING);
    let boundary = |position: usize| if position <= first.len() {
        first.is_char_boundary(position)
    } else { second.is_char_boundary(position - first.len()) };
    if !boundary(cursor) || !boundary(anchor) { return None; }
    while start < low && !boundary(start) { start += 1; }
    while end > high && !boundary(end) { end -= 1; }
    let mut text = String::with_capacity(end - start);
    if start < first.len() { text.push_str(&first[start..end.min(first.len())]); }
    if end > first.len() { text.push_str(&second[start.saturating_sub(first.len())..end - first.len()]); }
    Some((text, cursor - start, anchor - start))
}

fn valid_edit(edit: &Edit) -> bool {
    let valid = |text: &str| text.len() <= MAX_TEXT && !text.contains('\0');
    edit.commit.as_ref().is_none_or(|text| valid(text))
        && edit.preedit.as_ref().is_none_or(|(text, cursor)| valid(text) && cursor.is_none_or(|(anchor, focus)| {
            anchor <= text.len() && focus <= text.len() && text.is_char_boundary(anchor) && text.is_char_boundary(focus)
        }))
        && edit.delete.is_none_or(|(before, after)| {
            if edit.context.sensitive { return before == 0 && after == 0; }
            let low = edit.context.cursor.min(edit.context.anchor);
            let high = edit.context.cursor.max(edit.context.anchor);
            let Some(start) = low.checked_sub(before as usize) else { return false };
            let Some(end) = high.checked_add(after as usize) else { return false };
            edit.context.surrounding.is_char_boundary(start)
                && edit.context.surrounding.is_char_boundary(end)
        })
}

fn apply(world: &mut World) {
    let focus = world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get());
    let previous = std::mem::replace(&mut world.resource_mut::<Pending>().focus, focus);
    if previous != focus { world.resource_mut::<Pending>().ime = None; }
    if previous != focus && let Some(previous) = previous
        && let Some(mut input) = world.get_mut::<EditableText>(previous)
        && input.is_composing()
    { input.queue_edit(TextEdit::clear_ime_compose()); }
    let edits = std::mem::take(&mut world.resource_mut::<Pending>().edits);
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>()
        || !world.contains_resource::<bevy::clipboard::Clipboard>() { return; }
    for edit in edits {
        let Some(entity) = world.get_resource::<bevy::input_focus::InputFocus>().and_then(|focus| focus.get()) else { continue };
        if entity.to_bits() != edit.context.owner || !valid_edit(&edit)
            || world.resource::<Pending>().limits.contains_key(&entity)
            || text_state(world, entity, edit.context.caret).as_ref() != Some(&edit.context)
            || world.get::<EditableText>(entity).is_none_or(|input| !input.pending_edits.is_empty() || input.pending_paste.is_some()) { continue; }
        let maximum = world.get::<EditableText>(entity).unwrap().max_characters;
        world.resource_mut::<Pending>().limits.insert(entity, maximum);
        world.resource_scope(|world, mut fonts: Mut<FontCx>| {
            world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                    let mut input = world.get_mut::<EditableText>(entity).unwrap();
                    let multiline = input.allow_newlines;
                    let mut driver = input.editor.driver(&mut fonts.context, &mut layout.0);
                    if edit.delete.is_some() || edit.commit.is_some() { driver.clear_compose(); }
                    if let Some((before, after)) = edit.delete {
                        if let Some(bytes) = NonZeroUsize::new(before as usize) { driver.delete_bytes_before_selection(bytes); }
                        if let Some(bytes) = NonZeroUsize::new(after as usize) { driver.delete_bytes_after_selection(bytes); }
                    }
                    let selected = driver.editor.raw_selection().text_range();
                    let selected = driver.editor.raw_text()[selected].chars().count();
                    input.max_characters = maximum.map(|limit| limit.saturating_add(selected));
                    if let Some(value) = edit.commit
                        && value.chars().all(|character| !character.is_control() || (multiline && matches!(character, '\n' | '\t'))) {
                        input.queue_edit(TextEdit::ImeCommit { value: value.into() });
                    }
                    if let Some((value, cursor)) = edit.preedit {
                        input.queue_edit(TextEdit::ImeSetCompose { value: value.into(), cursor: cursor.map(|(anchor, focus)| PreeditCursor { anchor, focus }) });
                    }
            });
        });
    }
}

fn finish(world: &mut World) {
    let limits = std::mem::take(&mut world.resource_mut::<Pending>().limits);
    for (entity, maximum) in limits {
        if let Some(mut input) = world.get_mut::<EditableText>(entity) { input.max_characters = maximum; }
        let revision = text_state(world, entity, [0, 0, 1, 1]).map(|editor| editor.revision);
        world.resource_mut::<Pending>().ime = revision.map(|revision| (entity, revision));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn process(world: &mut World) {
        apply(world);
        world.run_system_once(bevy::text::apply_text_edits).unwrap();
        finish(world);
    }

    #[test]
    fn caret_bounds_include_all_transformed_corners_scale_and_output_origin() {
        let transform = UiGlobalTransform::from(bevy::math::Affine2::from_scale_angle_translation(
            Vec2::ONE, std::f32::consts::FRAC_PI_2, Vec2::splat(100.0),
        ));
        assert_eq!(caret_rectangle(&transform, [0.0, 0.0, 4.0, 10.0], Vec2::ZERO, 0.5, [640.0, 30.0]),
            Some([735, 130, 5, 2]));
        assert!(caret_rectangle(&transform, [f32::NAN, 0.0, 4.0, 10.0], Vec2::ZERO, 1.0, [0.0; 2]).is_none());
    }

    pub(super) fn editing_world(value: &str) -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Pending>();
        world.init_resource::<FontCx>();
        let font = Font::from_bytes(include_bytes!("../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
        world.resource_mut::<FontCx>().collection.register_fonts(font.data, None);
        world.resource_mut::<FontCx>().set_sans_serif_family("Lato").unwrap();
        world.init_resource::<LayoutCx>();
        world.init_resource::<bevy::clipboard::Clipboard>();
        world.init_resource::<bevy::input_focus::InputFocus>();
        let mut input = EditableText::new(value);
        input.pending_edits.clear();
        let owner = world.spawn(input).id();
        world.resource_mut::<bevy::input_focus::InputFocus>().set(owner, bevy::input_focus::FocusCause::Navigated);
        (world, owner)
    }

    #[test]
    fn unicode_edits_apply_atomically_and_stale_or_destroyed_targets_are_ignored() {
        let (mut world, owner) = editing_world("世");
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context: context.clone(), commit: Some("é".into()), delete: Some((0, 3)),
            preedit: Some(("界".into(), Some((0, 3)))) });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "é");
        assert!(world.get::<EditableText>(owner).unwrap().is_composing());
        assert!(text_state(&world, owner, [0, 0, 1, 20]).unwrap().from_ime);
        enqueue(&mut world, Edit { context, commit: Some("stale".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "é");
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context, commit: Some("gone".into()), preedit: None, delete: None });
        world.despawn(owner);
        process(&mut world);
        assert!(world.resource::<Pending>().edits.is_empty());
    }

    #[test]
    fn replacing_selected_text_respects_the_resulting_character_limit() {
        let (mut world, owner) = editing_world("abcd");
        world.resource_scope(|world, mut fonts: Mut<FontCx>| {
            world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                let mut input = world.get_mut::<EditableText>(owner).unwrap();
                input.max_characters = Some(4);
                input.editor.driver(&mut fonts.context, &mut layout.0).select_byte_range(0, 4);
            });
        });
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context, commit: Some("é世".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "é世");
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context, commit: Some("12345".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "é世");
    }

    #[test]
    fn changing_focus_cancels_preedit_and_new_editors_do_not_receive_old_responses() {
        let (mut world, owner) = editing_world("original");
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context: context.clone(), commit: None, delete: None,
            preedit: Some(("é".into(), Some((2, 2)))) });
        process(&mut world);
        let other = world.spawn(EditableText::new("other")).id();
        world.resource_mut::<bevy::input_focus::InputFocus>().set(other, bevy::input_focus::FocusCause::Navigated);
        enqueue(&mut world, Edit { context, commit: Some("misdirected".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(other).unwrap().value().to_string(), "other");
        assert!(!world.get::<EditableText>(owner).unwrap().is_composing());
    }

    #[test]
    fn native_commits_use_existing_character_filters_and_disabled_fields_are_unavailable() {
        let (mut world, owner) = editing_world("12");
        world.entity_mut(owner).insert(bevy::text::EditableTextFilter::new(|character| character.is_ascii_digit()));
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context, commit: Some("letters".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "12");
        let context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        enqueue(&mut world, Edit { context, commit: Some("3".into()), preedit: None, delete: None });
        process(&mut world);
        assert_eq!(world.get::<EditableText>(owner).unwrap().value().to_string(), "312");
        assert!(text_state(&world, owner, [0, 0, 1, 20]).unwrap().from_ime);
        world.get_mut::<EditableText>(owner).unwrap().editor.set_text("local");
        assert!(!text_state(&world, owner, [0, 0, 1, 20]).unwrap().from_ime);
        world.entity_mut(owner).insert(bevy::ui::InteractionDisabled);
        assert!(text_state(&world, owner, [0, 0, 1, 20]).is_none());
    }

    #[test]
    fn surrounding_is_bounded_and_offsets_remain_utf8_boundaries_across_preedit_split() {
        let first = "é".repeat(3000);
        let second = "世界".repeat(2000);
        let (text, cursor, anchor) = surrounding(&first, &second, 6003, 5998).unwrap();
        assert!(text.len() <= MAX_SURROUNDING);
        assert!(text.is_char_boundary(cursor));
        assert!(text.is_char_boundary(anchor));
        assert_eq!(&text[anchor..cursor], "é世");
        assert!(surrounding(&first, &second, 1, 0).is_none());
    }

    #[test]
    fn protected_editors_never_expose_surrounding_text_and_deletions_cannot_escape_context() {
        let mut world = World::new();
        let owner = world.spawn(EditableText::new("private")).id();
        native::protect_text_input(&mut world, owner);
        let protected = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        assert!(protected.sensitive);
        assert!(protected.surrounding.is_empty());
        assert_eq!((protected.cursor, protected.anchor), (0, 0));
        let mut context = text_state(&world, owner, [0, 0, 1, 20]).unwrap();
        context.sensitive = false;
        context.surrounding = "é世".into();
        context.cursor = 2;
        context.anchor = 2;
        let mut edit = Edit { context, commit: None, preedit: None, delete: Some((1, 0)) };
        assert!(!valid_edit(&edit));
        edit.delete = Some((2, 3));
        assert!(valid_edit(&edit));
        edit.context.sensitive = true;
        assert!(!valid_edit(&edit));
        edit.delete = None;
        edit.preedit = Some(("é".into(), Some((1, 2))));
        assert!(!valid_edit(&edit));
    }
}
