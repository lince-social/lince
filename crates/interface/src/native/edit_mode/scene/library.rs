//! Browse a local scene's published definitions and inspect exactly what a selected import needs.
use super::{Command, Documents, Form, Session, button, field, label, showoff_bracket, value};
use bevy::prelude::*;
use nucleus::canvas::scene::{DefinitionKind, Pin};
use std::{collections::BTreeMap, path::PathBuf};
use store::workspace_scene::library::{Catalog, Review, Status};

#[derive(Default)]
pub(super) struct State {
    expanded: bool,
    path: PathBuf,
    source: Option<Catalog>,
    reviewed: Option<(Pin, Review)>,
    applied: bool,
    show_pins: bool,
    local: Option<Pin>,
}

#[derive(Clone)]
pub(super) enum Action {
    Toggle,
    Browse,
    Example,
    Review(Pin),
    ShowPins,
    Import,
    Inspect(Pin),
    Place,
    Require,
    SetLocalOnly(bool),
}

fn short(pin: &Pin) -> &str {
    &pin.revision.as_str()[7..19]
}
fn reference(pin: &Pin) -> String {
    format!(
        "Source {}\nDefinition {}\nRevision {}",
        pin.document.as_str(),
        pin.part.as_str(),
        pin.revision.as_str()
    )
}
fn kind(intrinsic: &DefinitionKind) -> String {
    match intrinsic {
        DefinitionKind::Part => "Part".into(),
        DefinitionKind::Material { grade } => format!("Material · grade {grade}"),
        DefinitionKind::Unit { symbol } => format!("Unit · {symbol}"),
    }
}
fn details(world: &mut World, page: Entity, caption: &str) {
    let e = label(world, page, caption, 14.);
    world.entity_mut(e).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
}

fn choices(entries: Vec<(String, Pin)>, action: fn(Pin) -> Action) -> Vec<(String, Action)> {
    let mut counts = BTreeMap::new();
    for (caption, _) in &entries {
        *counts.entry(caption.as_str()).or_insert(0) += 1;
    }
    entries
        .iter()
        .map(|(caption, pin)| {
            let label = if counts[caption.as_str()] > 1 {
                format!(
                    "{caption} · {} · {} · {}",
                    pin.document.as_str(),
                    pin.part.as_str(),
                    pin.revision.as_str()
                )
            } else {
                caption.clone()
            };
            (label, action(pin.clone()))
        })
        .collect()
}

pub(super) fn controls(
    world: &mut World,
    root: Entity,
    workspace: u64,
    page: Entity,
) -> Option<Entity> {
    let session = &world.get::<Documents>(root)?.0[&workspace];
    let expanded = session.library.expanded;
    let toggle = if expanded {
        "Close definition library"
    } else {
        "Definition library"
    };
    if !expanded {
        button(world, page, toggle, Command::Library(Action::Toggle));
        return None;
    }
    let state = &session.library;
    let scene = session.scene.as_ref()?;
    let path = state.path.display().to_string();
    let mut source_buttons = Vec::new();
    let mut descriptions = Vec::new();
    if let Some(source) = &state.source {
        descriptions.push(format!(
            "Source catalogue {}\nPublished {}",
            source.document.as_str(),
            source.revision.as_str()
        ));
        for row in source.available() {
            let name = row.name.as_deref().expect("permitted catalogue entry");
            source_buttons.push((
                format!("Review {name} · {}", short(&row.pin)),
                row.pin.clone(),
            ));
        }
    }
    let mut allow_import = false;
    let show_pins = state.show_pins;
    let has_review = state.reviewed.is_some();
    if let Some((_, review)) = &state.reviewed {
        for item in &review.items {
            let status = match &item.status {
                Status::BringIn if state.applied => "Imported",
                Status::BringIn => "Bring in",
                Status::Reuse => "Reuse",
                Status::Unavailable(_) => "Unavailable",
            };
            let name = item.name.as_deref().unwrap_or("Required definition");
            let mut line = format!("{status} · {name} · {}", short(&item.pin));
            if let Some(i) = &item.intrinsic {
                line.push_str(&format!("\n{}", kind(i)));
            }
            if let Status::Unavailable(reason) = &item.status {
                line.push_str(&format!("\n{reason}"));
            }
            for other in &item.conflicts {
                line.push_str(&format!("\nConflict · keep both · {}", short(other)));
                if show_pins {
                    line.push_str(&format!("\n{}", reference(other)));
                }
            }
            if show_pins {
                line.push_str(&format!("\n{}", reference(&item.pin)));
            }
            descriptions.push(line);
        }
        if !review.complete() {
            descriptions
                .push("Incomplete import. The destination keeps its current definitions.".into());
        }
        allow_import = review.complete() && !state.applied;
    }
    let mut local_buttons = Vec::new();
    for p in scene.parts.iter() {
        let pin = p.pin(&scene.document).expect("validated scene definition");
        local_buttons.push((format!("Inspect {} · {}", p.name, short(&pin)), pin));
    }
    let mut selected_buttons: Vec<(&str, Action)> = Vec::new();
    let mut selected_details = Vec::new();
    if let Some((pin, p)) = state
        .local
        .as_ref()
        .and_then(|pin| scene.definition(pin).map(|p| (pin, p)))
    {
        selected_details.push(format!(
            "Document definition · {}\n{}\n{}",
            p.name,
            kind(&p.intrinsic),
            reference(pin)
        ));
        for dep in &p.requires {
            if let Some(child) = scene.definition(dep) {
                selected_details.push(format!(
                    "Requires {} · {} · {}",
                    child.name,
                    kind(&child.intrinsic),
                    short(dep)
                ));
            }
        }
        selected_buttons.push(("Place this definition", Action::Place));
        if session
            .selected
            .as_ref()
            .and_then(|h| scene.slot(h))
            .is_some_and(|s| s.source.is_none() && s.pin(&scene.document) != *pin)
        {
            selected_buttons.push(("Require for the selected placement", Action::Require));
        }
        let caption = if p.local_only {
            "Allow this definition to be imported"
        } else {
            "Keep this definition local"
        };
        selected_buttons.push((caption, Action::SetLocalOnly(!p.local_only)));
    }
    let sample = showoff_bracket(world, root, workspace).is_some();
    button(world, page, toggle, Command::Library(Action::Toggle));
    let input = field(world, page, "Source scene directory", &path);
    button(
        world,
        page,
        "Browse source definitions",
        Command::Library(Action::Browse),
    );
    if sample {
        button(
            world,
            page,
            "Browse the example parts library",
            Command::Library(Action::Example),
        );
    }
    for (caption, action) in choices(source_buttons, Action::Review) {
        button(world, page, &caption, Command::Library(action));
    }
    for line in descriptions {
        details(world, page, &line);
    }
    if has_review {
        let caption = if show_pins {
            "Hide source and revision details"
        } else {
            "Show source and revision details"
        };
        button(world, page, caption, Command::Library(Action::ShowPins));
    }
    if allow_import {
        button(
            world,
            page,
            "Import reviewed definitions",
            Command::Library(Action::Import),
        );
    }
    label(world, page, "Definitions in this document", 17.);
    for (caption, action) in choices(local_buttons, Action::Inspect) {
        button(world, page, &caption, Command::Library(action));
    }
    for line in selected_details {
        details(world, page, &line);
    }
    for (caption, action) in selected_buttons {
        button(world, page, caption, Command::Library(action));
    }
    Some(input)
}

fn forget_source(state: &mut State) {
    state.source = None;
    state.reviewed = None;
    state.applied = false;
}

fn browse(state: &mut State, path: PathBuf) -> Result<String, String> {
    state.path = path;
    forget_source(state);
    if state.path.as_os_str().is_empty() {
        return Err("Enter the source scene directory.".into());
    }
    let source = Catalog::open(&state.path).map_err(|e| format!("Source unavailable. {e}"))?;
    let count = source.available().count();
    state.source = Some(source);
    Ok(format!(
        "Choose a definition to review its requirements. {count} available."
    ))
}

pub(super) fn run(
    s: &mut Session,
    world: &World,
    form: Form,
    action: &Action,
) -> Result<String, String> {
    if matches!(action, Action::Toggle) {
        s.library.expanded = !s.library.expanded;
        return Ok("Use a source scene directory to bring in a definition.".into());
    }
    if matches!(action, Action::ShowPins) {
        s.library.show_pins = !s.library.show_pins;
        return Ok(if s.library.show_pins {
            "Showing source documents and full revisions."
        } else {
            "Showing the dependency summary."
        }
        .into());
    }
    let dir = PathBuf::from(
        value(
            world,
            form.source_directory
                .ok_or("Open the definition library.")?,
        )?
        .trim(),
    );
    if matches!(action, Action::Review(_) | Action::Import) && s.library.path != dir {
        s.library.path = dir;
        forget_source(&mut s.library);
        return Err("Browse the changed source directory first.".into());
    }
    let scene = s
        .scene
        .as_mut()
        .ok_or("Create or open a destination scene.")?;
    match action {
        Action::Toggle | Action::ShowPins => unreachable!(),
        Action::Example => {
            showoff_bracket(world, form.root, form.workspace)
                .ok_or("Open the Showoff workbench.")?;
            let path = crate::native::workspace_config::path(world, form.workspace)
                .ok_or("The Showoff needs a local directory.")?
                .with_file_name("showoff-parts.workspace");
            crate::native::laboratory::showoff::parts_library(&path).map_err(|e| e.to_string())?;
            browse(&mut s.library, path)
        }
        Action::Browse => browse(&mut s.library, dir),
        Action::Review(pin) => {
            let source = s
                .library
                .source
                .as_ref()
                .ok_or("Browse the source first.")?;
            let review = match source.preview(pin, scene) {
                Ok(review) => review,
                Err(e) => {
                    forget_source(&mut s.library);
                    return Err(e.to_string());
                }
            };
            let complete = review.complete();
            s.library.reviewed = Some((pin.clone(), review));
            s.library.applied = false;
            s.library.show_pins = false;
            Ok(if complete {
                "Review the definitions before importing."
            } else {
                "Incomplete import. A required definition is unavailable."
            }
            .into())
        }
        Action::Import => {
            let source = s
                .library
                .source
                .as_ref()
                .ok_or("Browse the source first.")?;
            let (pin, review) = s
                .library
                .reviewed
                .as_ref()
                .ok_or("Review a source definition first.")?;
            let before = scene.clone();
            let count = match source.import(pin, scene, review) {
                Ok(count) => count,
                Err(e) => {
                    forget_source(&mut s.library);
                    return Err(e.to_string());
                }
            };
            let reused = review.items.len() - count;
            s.library.local = Some(pin.clone());
            s.library.applied = true;
            if count > 0 {
                s.remember(before);
            }
            Ok(format!(
                "Imported {count} definitions. Reused {reused}. Existing placements kept their pins."
            ))
        }
        Action::Inspect(pin) => {
            scene.definition(pin).ok_or("Definition is unavailable.")?;
            s.library.local = Some(pin.clone());
            Ok("Inspecting the pinned document definition.".into())
        }
        Action::Place | Action::Require | Action::SetLocalOnly(_) => {
            let pin = s
                .library
                .local
                .as_ref()
                .ok_or("Inspect a definition first.")?;
            let before = scene.clone();
            let result = match action {
                Action::Place => {
                    s.selected = Some(scene.place_definition(pin)?);
                    "Placed a new occurrence at the local origin."
                }
                Action::Require => {
                    scene.require_definition(
                        s.selected.as_ref().ok_or("Select a placement first.")?,
                        pin,
                    )?;
                    "The selected placement now pins a local revision with this requirement."
                }
                Action::SetLocalOnly(local) => {
                    scene.set_local_only(pin, *local)?;
                    "Save the scene to publish the changed import permission."
                }
                _ => unreachable!(),
            };
            if *scene != before {
                s.remember(before);
            }
            Ok(result.into())
        }
    }
}
