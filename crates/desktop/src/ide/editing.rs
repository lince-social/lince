use super::*;
use bevy::{
    input_focus::{FocusCause, InputFocus},
    text::{FontCx, LayoutCx, TextEdit},
};
use lince_editor::Edit;

pub(super) fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    mut inputs: Query<&mut EditableText>,
    keys: Res<ButtonInput<KeyCode>>,
    views: Query<(Entity, &View)>,
    mut commands: Commands,
) {
    let Ok(mut input) = inputs.get_mut(event.focused_entity) else {
        return;
    };
    if input.is_composing() || !event.input.state.is_pressed() {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let control = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    match event.input.key_code {
        KeyCode::Tab if !control => {
            if let Some((owner, _)) = views
                .iter()
                .find(|(_, view)| view.editor == event.focused_entity)
            {
                commands.queue(move |world: &mut World| {
                    crate::actions::dispatch(
                        world,
                        owner,
                        crate::actions![actions::Control::Indent(shift)],
                    )
                });
            }
            event.propagate(false);
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let up = event.input.key_code == KeyCode::PageUp;
            for _ in 0..20 {
                input.queue_edit(if up {
                    TextEdit::Up(shift)
                } else {
                    TextEdit::Down(shift)
                });
            }
            event.propagate(false);
        }
        _ => {}
    }
}

pub(super) fn prepare(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        let view = world.get::<View>(owner).unwrap();
        let editor = view.editor;
        if view.draft {
            continue;
        }
        shortcuts(world, owner);
        if world
            .get::<EditableText>(editor)
            .unwrap()
            .pending_edits
            .is_empty()
            && world.get::<View>(owner).unwrap().paste.is_none()
        {
            continue;
        }
        let edits =
            std::mem::take(&mut world.get_mut::<EditableText>(editor).unwrap().pending_edits);
        if world.get::<View>(owner).unwrap().window.as_ref().is_none() {
            continue;
        }
        let mut native = Vec::new();
        let mut extending = false;
        for edit in edits {
            if !native.is_empty()
                && matches!(
                    edit,
                    TextEdit::WordLeft(_)
                        | TextEdit::WordRight(_)
                        | TextEdit::BackspaceWord
                        | TextEdit::DeleteWord
                )
            {
                flush(world, owner, &mut native);
            }
            let view = world.get::<View>(owner).unwrap();
            let Some(path) = &view.path else {
                continue;
            };
            let Some(doc) = world.resource::<Documents>().0.get(path) else {
                continue;
            };
            if (doc.preview.is_some() || view.window.as_ref().is_some_and(|window| window.clipped))
                && matches!(
                    edit,
                    TextEdit::Insert(_)
                        | TextEdit::Cut
                        | TextEdit::Paste
                        | TextEdit::Backspace
                        | TextEdit::BackspaceWord
                        | TextEdit::Delete
                        | TextEdit::DeleteWord
                        | TextEdit::ImeCommit { .. }
                        | TextEdit::ImeSetCompose { .. }
                )
            {
                continue;
            }
            let rope = doc.buffer.snapshot();
            let window = view.window.as_ref().unwrap();
            let selection = view.selection;
            let outside = selection
                .iter()
                .any(|p| *p < window.start || *p > window.end);
            match edit {
                TextEdit::WordLeft(extend) | TextEdit::WordRight(extend) => {
                    let backwards = matches!(edit, TextEdit::WordLeft(_));
                    let position = lince_editor::words::boundary(&rope, selection[1], backwards);
                    select(
                        world,
                        owner,
                        [if extend { selection[0] } else { position }, position],
                        rope.char_to_line(position),
                    );
                }
                TextEdit::BackspaceWord | TextEdit::DeleteWord => {
                    if selection[0] == selection[1] {
                        let position = lince_editor::words::boundary(
                            &rope,
                            selection[1],
                            matches!(edit, TextEdit::BackspaceWord),
                        );
                        world.get_mut::<View>(owner).unwrap().selection = [selection[1], position];
                    }
                    replace_selection(world, owner, String::new());
                }
                TextEdit::SelectAll => {
                    select(world, owner, [0, rope.len_chars()], 0);
                }
                TextEdit::TextStart(extend) | TextEdit::TextEnd(extend) => {
                    let position = if matches!(edit, TextEdit::TextStart(_)) {
                        0
                    } else {
                        rope.len_chars()
                    };
                    select(
                        world,
                        owner,
                        [if extend { selection[0] } else { position }, position],
                        rope.char_to_line(position),
                    );
                }
                TextEdit::Copy | TextEdit::Cut if outside => {
                    let range = selection[0].min(selection[1])..selection[0].max(selection[1]);
                    let text = rope.slice(range).to_string();
                    if let Some(mut clipboard) =
                        world.get_resource_mut::<bevy::clipboard::Clipboard>()
                    {
                        match clipboard.set_text(text) {
                            Ok(()) if matches!(edit, TextEdit::Cut) => {
                                replace_selection(world, owner, String::new())
                            }
                            Ok(()) => {}
                            Err(e) => status(world, owner, format!("Clipboard unavailable: {e}")),
                        }
                    }
                }
                TextEdit::Paste => {
                    if let Some(mut clipboard) =
                        world.get_resource_mut::<bevy::clipboard::Clipboard>()
                    {
                        let paste = clipboard.fetch_text();
                        world.get_mut::<View>(owner).unwrap().paste = Some(paste);
                    }
                }
                TextEdit::Insert(value) if outside => {
                    replace_selection(world, owner, value.to_string())
                }
                TextEdit::ImeCommit { value } if outside => {
                    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
                        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
                            world
                                .get_mut::<EditableText>(editor)
                                .unwrap()
                                .editor
                                .driver(&mut fonts.context, &mut layout.0)
                                .clear_compose();
                        });
                    });
                    replace_selection(world, owner, value.to_string());
                }
                TextEdit::Backspace | TextEdit::Delete
                    if outside && selection[0] != selection[1] =>
                {
                    replace_selection(world, owner, String::new())
                }
                TextEdit::Up(extend) if selection[1] <= window.start && window.first_line > 0 => {
                    let line = rope.char_to_line(selection[1]).saturating_sub(1);
                    let position = rope.line_to_char(line);
                    select(
                        world,
                        owner,
                        [if extend { selection[0] } else { position }, position],
                        line,
                    );
                }
                TextEdit::Down(extend)
                    if selection[1] >= window.end && window.end < rope.len_chars() =>
                {
                    let line = (rope.char_to_line(selection[1]) + 1).min(rope.len_lines() - 1);
                    let position = rope.line_to_char(line);
                    select(
                        world,
                        owner,
                        [if extend { selection[0] } else { position }, position],
                        line,
                    );
                }
                TextEdit::Insert(value)
                    if value.as_str() == "\n"
                        && doc
                            .disk
                            .text
                            .as_bytes()
                            .windows(2)
                            .take(4096)
                            .any(|bytes| bytes == b"\r\n") =>
                {
                    native.push(TextEdit::Insert("\r\n".into()))
                }
                other => {
                    if outside
                        && selection[0] == selection[1]
                        && !matches!(
                            other,
                            TextEdit::MoveToPoint(_)
                                | TextEdit::SelectWordAtPoint(_)
                                | TextEdit::SelectLineAtPoint(_)
                                | TextEdit::SelectedHardLineAtPoint(_)
                                | TextEdit::ShiftClickExtension(_)
                                | TextEdit::ExtendSelectionToPoint(_)
                        )
                    {
                        select(world, owner, selection, rope.char_to_line(selection[1]));
                        runtime::render(world, owner);
                    }
                    extending |= matches!(
                        other,
                        TextEdit::Left(true)
                            | TextEdit::Right(true)
                            | TextEdit::WordLeft(true)
                            | TextEdit::WordRight(true)
                            | TextEdit::Up(true)
                            | TextEdit::Down(true)
                            | TextEdit::HardLineStart(true)
                            | TextEdit::HardLineEnd(true)
                            | TextEdit::LineStart(true)
                            | TextEdit::LineEnd(true)
                            | TextEdit::ExtendSelectionToPoint(_)
                            | TextEdit::ShiftClickExtension(_)
                    );
                    native.push(other);
                }
            }
        }
        let paste = world
            .get_mut::<View>(owner)
            .unwrap()
            .paste
            .as_mut()
            .and_then(|paste| paste.poll_result());
        if let Some(result) = paste {
            world.get_mut::<View>(owner).unwrap().paste = None;
            match result {
                Ok(text) => replace_selection(world, owner, text),
                Err(e) => status(world, owner, format!("Clipboard unavailable: {e}")),
            }
        }
        runtime::render(world, owner);
        let had_input = !native.is_empty();
        if !native.is_empty() {
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .pending_edits
                .extend(native);
        }
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.had_input = had_input;
        view.extending = extending;
    }
}

fn flush(world: &mut World, owner: Entity, native: &mut Vec<TextEdit>) {
    world.init_resource::<bevy::clipboard::Clipboard>();
    let editor = world.get::<View>(owner).unwrap().editor;
    world
        .get_mut::<EditableText>(editor)
        .unwrap()
        .pending_edits
        .extend(std::mem::take(native));
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            world.resource_scope(|world, mut clipboard: Mut<bevy::clipboard::Clipboard>| {
                world
                    .get_mut::<EditableText>(editor)
                    .unwrap()
                    .apply_pending_edits(&mut fonts.context, &mut layout.0, &mut clipboard, |_| {
                        true
                    });
            });
        });
    });
    world.get_mut::<View>(owner).unwrap().had_input = true;
    capture_one(world, owner);
}

fn shortcuts(world: &mut World, owner: Entity) {
    let Some(focused) = world.get_resource::<InputFocus>().and_then(InputFocus::get) else {
        return;
    };
    let mut current = Some(focused);
    while let Some(entity) = current {
        if entity == owner {
            break;
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    if current != Some(owner)
        || world
            .get::<EditableText>(focused)
            .is_some_and(|input| input.is_composing())
    {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let control = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let editor = world.get::<View>(owner).unwrap().editor;
    let action = if control {
        if keys.just_pressed(KeyCode::Space) {
            Some(actions::Control::Complete)
        } else if shift && keys.just_pressed(KeyCode::KeyI) {
            Some(actions::Control::Format)
        } else if keys.just_pressed(KeyCode::KeyS) {
            Some(if shift {
                actions::Control::SaveAs
            } else {
                actions::Control::Save
            })
        } else if keys.just_pressed(KeyCode::KeyF) {
            Some(actions::Control::Focus(if shift { 4 } else { 1 }))
        } else if keys.just_pressed(KeyCode::KeyH) {
            Some(actions::Control::Focus(2))
        } else if keys.just_pressed(KeyCode::KeyG) {
            Some(actions::Control::Focus(3))
        } else if keys.just_pressed(KeyCode::KeyW) {
            Some(actions::Control::Close)
        } else if keys.just_pressed(KeyCode::KeyO) {
            Some(actions::Control::Open)
        } else if keys.just_pressed(KeyCode::Tab) {
            Some(actions::Control::CycleTab(shift))
        } else if focused == editor && keys.just_pressed(KeyCode::KeyZ) {
            Some(actions::Control::Undo(shift))
        } else if focused == editor && keys.just_pressed(KeyCode::KeyY) {
            Some(actions::Control::Undo(true))
        } else {
            None
        }
    } else if keys.just_pressed(KeyCode::F3) {
        Some(if shift {
            actions::Control::FindPrevious
        } else {
            actions::Control::Find
        })
    } else if keys.just_pressed(KeyCode::Escape) {
        Some(actions::Control::Focus(0))
    } else {
        None
    };
    if let Some(action) = action {
        crate::actions::dispatch(world, owner, crate::actions![action]);
    }
}

fn replace_selection(world: &mut World, owner: Entity, text: String) {
    let view = world.get::<View>(owner).unwrap();
    let Some(path) = view.path.clone() else {
        return;
    };
    if world
        .resource::<Documents>()
        .0
        .get(&path)
        .is_none_or(|doc| doc.preview.is_some())
    {
        return;
    }
    let selection = view.selection;
    let range = selection[0].min(selection[1])..selection[0].max(selection[1]);
    let end = range.start + text.chars().count();
    let result = world
        .resource_mut::<Documents>()
        .0
        .get_mut(&path)
        .unwrap()
        .buffer
        .edit(Edit { range, text });
    match result {
        Ok(()) => {
            let rope = world.resource::<Documents>().0[&path].buffer.snapshot();
            select(world, owner, [end, end], rope.char_to_line(end));
        }
        Err(e) => status(world, owner, e),
    }
}

pub(super) fn capture(world: &mut World) {
    if !world.contains_resource::<Documents>() {
        return;
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        capture_one(world, owner);
    }
}

pub(super) fn capture_one(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    if view.draft {
        return;
    }
    if !view.had_input
        && world
            .entity(view.editor)
            .get_ref::<EditableText>()
            .is_some_and(|input| !input.is_changed())
    {
        return;
    }
    let Some(window) = &view.window else {
        return;
    };
    let Some(path) = view.path.clone() else {
        return;
    };
    let Some(input) = world.get::<EditableText>(view.editor) else {
        return;
    };
    if input.is_composing() || window.clipped {
        return;
    }
    let value = input.value().to_string();
    let edit = Edit::between(&window.text, &value, window.start);
    if edit.is_none() && !view.had_input {
        return;
    }
    let bytes = [
        input.editor.raw_selection().anchor().index(),
        input.editor.raw_selection().focus().index(),
    ];
    let mut selection = bytes.map(|byte| {
        window.start
            + value[..floor_boundary(&value, byte.min(value.len()))]
                .chars()
                .count()
    });
    if view.extending {
        selection[0] = view.selection[0];
    }
    let revision = window.revision;
    let identity = window.identity;
    let mut documents = world.resource_mut::<Documents>();
    let Some(document) = documents.0.get_mut(&path) else {
        return;
    };
    if let Some(edit) = edit {
        if document.preview.is_some() {
            world.get_mut::<View>(owner).unwrap().window = None;
            return;
        }
        if document.buffer.revision() != revision || document.buffer.identity() != identity {
            world.get_mut::<View>(owner).unwrap().draft = true;
            status(
                world,
                owner,
                "The buffer changed during input; the visible draft is retained. Copy it before switching files.",
            );
            return;
        }
        if let Err(e) = document.buffer.edit(edit) {
            world.get_mut::<View>(owner).unwrap().draft = true;
            status(world, owner, e);
            return;
        }
    }
    let revision = document.buffer.revision();
    let anchors = selection.map(|position| document.buffer.anchor(position));
    let line = document
        .buffer
        .snapshot()
        .char_to_line(selection[1].min(document.buffer.snapshot().len_chars()));
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.selection = selection;
    view.anchors = anchors;
    view.had_input = false;
    view.discard = false;
    view.follow_caret = true;
    let viewport = view.viewport;
    let window = view.window.as_mut().unwrap();
    window.text = value;
    window.end = window.start + window.text.chars().count();
    window.revision = revision;
    let height = world
        .get::<ComputedNode>(viewport)
        .map_or(500.0, |node| node.size().y * node.inverse_scale_factor());
    let mut scroll = world.get_mut::<ScrollPosition>(viewport).unwrap();
    let y = line as f32 * LINE_HEIGHT;
    if y < scroll.y {
        scroll.y = y;
    } else if y + LINE_HEIGHT > scroll.y + height {
        scroll.y = (y + LINE_HEIGHT - height).max(0.0);
    }
}

fn floor_boundary(value: &str, mut index: usize) -> usize {
    while !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub(super) fn select(world: &mut World, owner: Entity, selection: [usize; 2], line: usize) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.selection = selection;
    view.anchors = [None, None];
    view.follow_caret = true;
    if let Some(window) = &mut view.window {
        window.revision = u64::MAX;
    }
    let (viewport, editor) = (view.viewport, view.editor);
    world.get_mut::<ScrollPosition>(viewport).unwrap().y =
        line.saturating_sub(3) as f32 * LINE_HEIGHT;
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(editor, FocusCause::Navigated);
    }
}

pub(super) fn follow_horizontal(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    if !view.follow_caret {
        return;
    }
    let (editor, viewport) = (view.editor, view.viewport);
    let Some(cursor) = world
        .get::<EditableText>(editor)
        .and_then(|input| input.editor.cursor_geometry(input.cursor_width))
    else {
        return;
    };
    let scale = world
        .get::<ComputedNode>(editor)
        .map_or(1.0, |node| node.inverse_scale_factor());
    let width = world
        .get::<ComputedNode>(viewport)
        .map_or(500.0, |node| node.size().x * node.inverse_scale_factor());
    let left = cursor.x0 as f32 * scale + 64.0;
    let right = cursor.x1 as f32 * scale + 64.0;
    let mut scroll = world.get_mut::<ScrollPosition>(viewport).unwrap();
    if left < scroll.x + 64.0 {
        scroll.x = (left - 64.0).max(0.0);
    } else if right + 16.0 > scroll.x + width {
        scroll.x = (right + 16.0 - width).max(0.0);
    }
    world.get_mut::<View>(owner).unwrap().follow_caret = false;
}

pub(super) fn widget_selection(world: &mut World, entity: Entity, bytes: [usize; 2]) {
    if !world.contains_resource::<FontCx>() || !world.contains_resource::<LayoutCx>() {
        return;
    }
    world.resource_scope(|world, mut fonts: Mut<FontCx>| {
        world.resource_scope(|world, mut layout: Mut<LayoutCx>| {
            if let Some(mut text) = world.get_mut::<EditableText>(entity) {
                text.editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .select_byte_range(bytes[0], bytes[1]);
            }
        });
    });
}
