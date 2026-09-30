use super::*;
use crate::actions::Action;
use lince_editor::{Edit, Resolution};

#[derive(Clone)]
pub(super) enum Control {
    Create,
    Save,
    SaveAs,
    ReplaceFile,
    CancelReplacement,
    Tools,
    Connect,
    Disconnect,
    Complete,
    Format,
    FormatCommand,
    LintCommand,
    ChooseCompletion(usize),
    Diagnostic(usize),
    Undo(bool),
    Refresh,
    Close,
    SaveClose,
    DiscardClose,
    CancelClose,
    Discard,
    Recover,
    Tab(PathBuf),
    Find,
    FindPrevious,
    Replace,
    ReplaceAll,
    Indent(bool),
    SearchPanel,
    Settings,
    Autosave,
    MatchCase,
    WholeWord,
    ProjectSearchSetting,
    ProjectSearch,
    OpenMatch(lince_editor::project_search::Match),
    Focus(u8),
    CycleTab(bool),
    Open,
    Go,
    Resolve(bool),
    ReviewDisk,
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            if let Some((root, workspace, position)) = crate::file_explorer::location(world, owner)
            {
                spawn(world, root, workspace, position, Ide::default());
            }
            return;
        }
        if world.get::<View>(owner).is_none() {
            return;
        }
        match self {
            Self::Tools
            | Self::Connect
            | Self::Disconnect
            | Self::Complete
            | Self::Format
            | Self::FormatCommand
            | Self::LintCommand
            | Self::ChooseCompletion(_)
            | Self::Diagnostic(_) => {
                tools::action(world, owner, self);
                return;
            }
            Self::ReplaceFile | Self::CancelReplacement => {
                save_as::confirm(world, owner, matches!(self, Self::ReplaceFile));
                return;
            }
            Self::SaveClose => {
                closing::apply(world, owner, true);
                return;
            }
            Self::DiscardClose => {
                closing::apply(world, owner, false);
                return;
            }
            Self::CancelClose => {
                closing::cancel(world, owner);
                return;
            }
            Self::Settings => {
                let panel = world.get::<View>(owner).unwrap().settings_panel;
                let mut node = world.get_mut::<Node>(panel).unwrap();
                node.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
                return;
            }
            Self::Autosave
            | Self::MatchCase
            | Self::WholeWord
            | Self::SearchPanel
            | Self::ProjectSearchSetting => {
                let mut ide = world.get_mut::<Ide>(owner).unwrap();
                match self {
                    Self::Autosave => {
                        ide.settings.autosave_seconds = match ide.settings.autosave_seconds {
                            0 => 2,
                            2 => 5,
                            _ => 0,
                        }
                    }
                    Self::MatchCase => {
                        ide.settings.search.case_sensitive = !ide.settings.search.case_sensitive
                    }
                    Self::WholeWord => {
                        ide.settings.search.whole_word = !ide.settings.search.whole_word
                    }
                    Self::ProjectSearchSetting => {
                        ide.settings.project_search = !ide.settings.project_search
                    }
                    _ => ide.settings.search_visible = !ide.settings.search_visible,
                }
                settings::render(world, owner);
                return;
            }
            Self::Focus(field) => {
                if *field == 4 {
                    world.get_mut::<Ide>(owner).unwrap().settings.project_search = true;
                }
                if *field != 0 {
                    world.get_mut::<Ide>(owner).unwrap().settings.search_visible = true;
                    settings::render(world, owner);
                }
                let view = world.get::<View>(owner).unwrap();
                let entity = match field {
                    1 | 4 => view.find,
                    2 => view.replace,
                    3 => view.line,
                    _ => view.editor,
                };
                if let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() {
                    focus.set(entity, bevy::input_focus::FocusCause::Navigated);
                }
                return;
            }
            Self::CycleTab(backwards) => {
                editing::capture_one(world, owner);
                if world.get::<View>(owner).unwrap().draft {
                    return;
                }
                let mut ide = world.get_mut::<Ide>(owner).unwrap();
                if !ide.paths.is_empty() {
                    let index = ide
                        .paths
                        .iter()
                        .position(|path| Some(path) == ide.active.as_ref())
                        .unwrap_or(0);
                    ide.active = Some(
                        ide.paths[(index + if *backwards { ide.paths.len() - 1 } else { 1 })
                            % ide.paths.len()]
                        .clone(),
                    );
                }
                return;
            }
            Self::Open => {
                let result = crate::file_explorer::worker::run(
                    world,
                    || {
                        rfd::FileDialog::new().pick_file().map(|path| {
                            let scope = Scope::open(path.parent().ok_or("Choose a file")?)?;
                            Ok::<_, String>((scope, path))
                        })
                    },
                    move |world, result| match result {
                        Some(Ok((scope, path))) => open(world, owner, scope, path),
                        Some(Err(e)) => status(world, owner, e),
                        None => {}
                    },
                );
                if let Err(e) = result {
                    status(world, owner, e);
                }
                return;
            }
            Self::ProjectSearch => {
                project::run(world, owner);
                return;
            }
            Self::OpenMatch(row) => {
                project::open_match(world, owner, row.clone());
                return;
            }
            _ => {}
        }
        editing::capture_one(world, owner);
        if world.get::<View>(owner).unwrap().draft {
            if matches!(self, Self::Discard) {
                let mut view = world.get_mut::<View>(owner).unwrap();
                if view.discard {
                    view.draft = false;
                    view.discard = false;
                    view.window = None;
                    view.notice.clear();
                } else {
                    view.discard = true;
                    view.notice =
                        "Press Discard… again to discard the retained visible draft".into();
                }
            }
            return;
        }
        if let Self::Tab(path) = self {
            world.get_mut::<Ide>(owner).unwrap().active = Some(path.clone());
            status(world, owner, "");
            return;
        }
        if matches!(self, Self::Recover) {
            recovery::retry(world);
            let paths: Vec<_> = world
                .resource::<Documents>()
                .0
                .iter()
                .filter(|(_, doc)| doc.buffer.is_dirty())
                .map(|(path, _)| path.clone())
                .collect();
            let mut ide = world.get_mut::<Ide>(owner).unwrap();
            for path in paths {
                if ide.paths.len() < 16 && !ide.paths.contains(&path) {
                    ide.paths.push(path);
                }
            }
            ide.active = ide.paths.first().cloned();
            return;
        }
        let Some(path) = world.get::<Ide>(owner).unwrap().active.clone() else {
            return;
        };
        if !world.resource::<Documents>().0.contains_key(&path) {
            if matches!(self, Self::Close | Self::Discard) {
                close_tab(world, owner, &path);
            }
            return;
        }
        if world.resource::<Documents>().0[&path].preview.is_some()
            && matches!(
                self,
                Self::Save
                    | Self::SaveAs
                    | Self::Undo(_)
                    | Self::Replace
                    | Self::ReplaceAll
                    | Self::Indent(_)
            )
        {
            status(
                world,
                owner,
                "This is a read-only preview; the original file is unchanged",
            );
            return;
        }
        match self {
            Self::ReviewDisk => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.review = !view.review;
            }
            Self::Save => save(world, owner, path),
            Self::SaveAs => save_as(world, owner, path),
            Self::Refresh => {
                world
                    .resource_mut::<Documents>()
                    .0
                    .get_mut(&path)
                    .unwrap()
                    .refresh = true;
            }
            Self::Undo(redo) => {
                let result = world
                    .resource_mut::<Documents>()
                    .0
                    .get_mut(&path)
                    .unwrap()
                    .buffer
                    .undo(*redo);
                if let Err(e) = result {
                    status(world, owner, e);
                }
            }
            Self::Resolve(disk) => {
                let result = world
                    .resource_mut::<Documents>()
                    .0
                    .get_mut(&path)
                    .unwrap()
                    .buffer
                    .resolve(if *disk {
                        Resolution::Disk
                    } else {
                        Resolution::Local
                    });
                if let Err(e) = result {
                    status(world, owner, e);
                }
            }
            Self::Close | Self::Discard => {
                let document = &world.resource::<Documents>().0[&path];
                if document.saving.is_some() || document.moving {
                    status(world, owner, "Wait for this save to finish");
                    return;
                }
                let dirty = document.buffer.is_dirty()
                    || document.buffer.conflict().is_some()
                    || document.file.is_none();
                if matches!(self, Self::Close)
                    && world
                        .query::<(Entity, &Ide)>()
                        .iter(world)
                        .any(|(other, ide)| other != owner && ide.paths.contains(&path))
                {
                    close_tab(world, owner, &path);
                    return;
                }
                if dirty {
                    if matches!(self, Self::Close) {
                        closing::show(world, owner, path.clone());
                        return;
                    }
                    if !matches!(self, Self::Discard) || !world.get::<View>(owner).unwrap().discard
                    {
                        world.get_mut::<View>(owner).unwrap().discard =
                            matches!(self, Self::Discard);
                        status(
                            world,
                            owner,
                            "Unsaved edits. Save first, or press Discard… twice to discard this file in every IDE view.",
                        );
                        return;
                    }
                    let owners: Vec<_> = world
                        .query::<(Entity, &Ide)>()
                        .iter(world)
                        .filter(|(_, ide)| ide.paths.contains(&path))
                        .map(|(owner, _)| owner)
                        .collect();
                    for owner in owners {
                        close_tab(world, owner, &path);
                    }
                    world.resource_mut::<Documents>().0.remove(&path);
                } else {
                    close_tab(world, owner, &path);
                }
            }
            Self::Find => search::run(world, owner, false, false),
            Self::FindPrevious => search::run(world, owner, true, false),
            Self::ReplaceAll => search::run(world, owner, false, true),
            Self::Indent(outdent) => {
                let view = world.get::<View>(owner).unwrap();
                if world
                    .get::<EditableText>(view.editor)
                    .is_some_and(|input| input.is_composing())
                    || view.window.as_ref().is_none_or(|window| window.clipped)
                {
                    return;
                }
                let selection = view.selection;
                let mut docs = world.resource_mut::<Documents>();
                let buffer = &mut docs.0.get_mut(&path).unwrap().buffer;
                let (edits, selection) =
                    lince_editor::indent::changes(&buffer.snapshot(), selection, *outdent);
                match buffer.edit_batch(&edits) {
                    Ok(()) => {
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        view.selection = selection;
                        view.anchors = [None, None];
                        view.window = None;
                        view.follow_caret = true;
                    }
                    Err(e) => status(world, owner, e),
                }
            }
            Self::Replace => {
                let view = world.get::<View>(owner).unwrap();
                let selection = view.selection;
                let (Ok(needle), Ok(replacement)) = (
                    crate::sand_panel::value(world, view.find),
                    crate::sand_panel::value(world, view.replace),
                ) else {
                    return;
                };
                let options = world.get::<Ide>(owner).unwrap().settings.search;
                let mut docs = world.resource_mut::<Documents>();
                let buffer = &mut docs.0.get_mut(&path).unwrap().buffer;
                let range = selection[0].min(selection[1])..selection[0].max(selection[1]);
                if range.end <= buffer.snapshot().len_chars()
                    && lince_editor::search::Pattern::new(&needle, options).is_ok_and(|pattern| {
                        pattern.matches_range(&buffer.snapshot(), range.clone())
                    })
                    && !needle.is_empty()
                {
                    let end = range.start + replacement.chars().count();
                    match buffer.edit(Edit {
                        range,
                        text: replacement,
                    }) {
                        Ok(()) => {
                            let mut view = world.get_mut::<View>(owner).unwrap();
                            view.selection = [end, end];
                            view.anchors = [None, None];
                        }
                        Err(e) => status(world, owner, e),
                    }
                } else {
                    search::run(world, owner, false, false);
                }
            }
            Self::Go => {
                let view = world.get::<View>(owner).unwrap();
                let number = crate::sand_panel::value(world, view.line)
                    .ok()
                    .and_then(|n| n.parse::<usize>().ok());
                if let Some(number) = number.filter(|n| *n > 0) {
                    let rope = world.resource::<Documents>().0[&path].buffer.snapshot();
                    let line = (number - 1).min(rope.len_lines() - 1);
                    let position = rope.line_to_char(line);
                    editing::select(world, owner, [position, position], line);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn close_tab(world: &mut World, owner: Entity, path: &PathBuf) {
    if world.get::<View>(owner).unwrap().closing.as_ref() == Some(path) {
        closing::cancel(world, owner);
    }
    let mut ide = world.get_mut::<Ide>(owner).unwrap();
    ide.paths.retain(|p| p != path);
    if ide.active.as_ref() == Some(path) {
        ide.active = ide.paths.first().cloned();
    }
    world.get_mut::<View>(owner).unwrap().discard = false;
    status(world, owner, "");
}

pub(super) fn save(world: &mut World, owner: Entity, path: PathBuf) {
    let view = world.get::<View>(owner).unwrap();
    if world
        .get::<EditableText>(view.editor)
        .is_some_and(|edit| edit.is_composing())
    {
        status(world, owner, "Finish composing text before saving");
        return;
    }
    let document = &world.resource::<Documents>().0[&path];
    if document.preview.is_some() {
        return;
    }
    if !document.buffer.is_dirty() && document.file.is_some() {
        status(world, owner, "Saved");
        return;
    }
    if document.saving.is_some() || document.reading || document.moving {
        status(
            world,
            owner,
            "Waiting for file access; try saving again shortly",
        );
        return;
    }
    let point = match document.buffer.prepare_save() {
        Ok(point) => point,
        Err(e) => {
            status(world, owner, e);
            return;
        }
    };
    let revision = point.revision;
    let expected = document.disk.clone();
    let Some(file) = document.file.clone() else {
        status(
            world,
            owner,
            "The original location is unavailable. Use Save as…",
        );
        return;
    };
    let reply_path = path.clone();
    let result = crate::file_explorer::worker::run(
        world,
        move || {
            let point = point.encode();
            file.save(&expected, &point.text.to_string())
                .map(|snapshot| (point, snapshot))
        },
        move |world, result| {
            let mut documents = world.resource_mut::<Documents>();
            let Some(document) = documents.0.get_mut(&reply_path) else {
                return;
            };
            let Some(_) = document.saving.take() else {
                return;
            };
            match result {
                Ok((point, snapshot)) => {
                    let text = snapshot.text.clone();
                    document.disk = snapshot;
                    document.error = document.buffer.saved_with_text(point, text).err();
                    status(world, owner, "");
                }
                Err(e) => {
                    document.error = Some(e.clone());
                    document.refresh = true;
                    status(world, owner, e);
                }
            }
        },
    );
    match result {
        Ok(()) => {
            world
                .resource_mut::<Documents>()
                .0
                .get_mut(&path)
                .unwrap()
                .saving = Some(revision);
            status(world, owner, "Saving…");
        }
        Err(e) => status(world, owner, e),
    }
}

fn save_as(world: &mut World, owner: Entity, path: PathBuf) {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let result = crate::file_explorer::worker::run(
        world,
        move || rfd::FileDialog::new().set_file_name(name).save_file(),
        move |world, destination| {
            if let Some(destination) = destination {
                save_copy(world, owner, path, destination);
            }
        },
    );
    if let Err(e) = result {
        status(world, owner, e);
    }
}

pub(super) fn save_copy(world: &mut World, owner: Entity, path: PathBuf, destination: PathBuf) {
    save_as::inspect(world, owner, path, destination);
}
