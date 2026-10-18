#[cfg(test)]
mod tests;
mod ui;

use crate::native::{actions::Action, sand_panel as panel};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const SOURCES: [protein::Source; 4] = [
    protein::Source::Lingua,
    protein::Source::Concept,
    protein::Source::Record,
    protein::Source::Assertion,
];
const PAGE: usize = 30;

#[derive(Component)]
pub struct OntologySand {
    editor: Entity,
    list: Entity,
    status: Entity,
    tab: usize,
    page: usize,
    name: Entity,
    language: Entity,
    definition: Entity,
    package: Entity,
    context: Entity,
    feature: String,
    knowledge_sources: Vec<Value>,
    knowledge_selected: Vec<String>,
    knowledge_results: Vec<Value>,
    knowledge_job: Option<String>,
    knowledge_poll: std::time::Instant,
    knowledge_page: usize,
    knowledge_filter: String,
    knowledge_item: Option<Value>,
    knowledge_claim: Option<Value>,
    knowledge_availability: String,
    knowledge_preview: Option<Value>,
    knowledge_review: Option<Value>,
    knowledge_fields: Vec<nucleus::knowledge::UpdateField>,
    knowledge_cached: Vec<Value>,
    knowledge_cache_bytes: u64,
    knowledge_file: Option<(String, String)>,
    output: Option<Value>,
    package_page: usize,
    definition_page: usize,
    preferred: bool,
    personal: bool,
    term: String,
    choices: [String; 7],
    choice_pages: [usize; 7],
    choosers: Vec<(Entity, usize)>,
    rows: [Vec<Value>; 4],
    ids: [String; 4],
    requested: [bool; 4],
    ready: [bool; 4],
    pending: Option<String>,
    dirty: bool,
}

#[derive(Resource, Default)]
struct Subscriptions(HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>);

pub struct OntologyPlugin;
impl Plugin for OntologyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Subscriptions>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                update.after(crate::native::cell_bridge::ReceiveCell),
            );
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    world.init_resource::<Subscriptions>();
    ui::populate(world, sand);
    sand
}

#[derive(Clone)]
enum Command {
    Tab(usize),
    Choose(usize, String),
    ChoicePage(usize, bool),
    Page(bool),
    Refresh,
    Create,
    Rename,
    Delete,
    Include(bool),
    Parent(bool),
    Assert,
    Identity,
    Retract(String),
    Term(String),
    AddTerm,
    EditTerm,
    RetireTerm,
    Preferred,
    Personal,
    Language,
    Context,
    PreferTerm,
    UseWording,
    HideTerm(bool),
    RestoreOriginal,
    ResetTerminology,
    InspectTerminology,
    InspectMeaning(u32),
    ExportTranslations,
    PreviewTranslations,
    ImportTranslations,
    Feature(String),
    PreviewFeature(bool, bool),
    ApplyFeature,
    ReviseDefinition,
    Publish,
    Withdraw,
    InspectPublication,
    PreviewPackage,
    Adopt,
    AdoptPackage,
    History(u32),
    PackagePage(bool),
    DefinitionPage(bool),
    InspectPublicationAt(String),
    KnowledgeSources,
    KnowledgeSource(String),
    KnowledgeSearch(u32),
    KnowledgeRead(String, String),
    KnowledgePoll,
    KnowledgeCancel,
    KnowledgeClear,
    KnowledgeInstall,
    KnowledgePage(bool),
    KnowledgeClaim(Option<usize>),
    KnowledgeMap(nucleus::knowledge::MeaningKind),
    KnowledgeReference,
    KnowledgeInspectSaved,
    KnowledgePreviewSave,
    KnowledgeSave,
    KnowledgeExport,
    KnowledgeImport,
    KnowledgeReview,
    KnowledgeAccept(bool),
    KnowledgeField(nucleus::knowledge::UpdateField),
    KnowledgeReplace,
    KnowledgeRemove(String),
    KnowledgeDump(String),
    KnowledgeCached(String, String),
    KnowledgeLoadFile,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<OntologySand>(owner) else {
            return;
        };
        let status = view.status;
        if view.pending.is_some() {
            panel::status(world, status, "Wait for the current change to finish");
            return;
        }
        match self {
            Self::KnowledgeClaim(index) => {
                let claim = index.and_then(|index| {
                    world
                        .get::<OntologySand>(owner)
                        .unwrap()
                        .knowledge_item
                        .as_ref()
                        .and_then(|item| item["claims"].get(index))
                        .cloned()
                });
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.knowledge_claim = claim;
                view.knowledge_page = 0;
                view.dirty = true;
                return;
            }
            Self::KnowledgeLoadFile => {
                if world.get::<KnowledgeFile>(owner).is_some() {
                    return;
                }
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let wake = world
                    .get_resource::<crate::native::wake::WakeSignal>()
                    .cloned();
                world.entity_mut(owner).insert(KnowledgeFile(receiver));
                std::thread::spawn(move || {
                    use std::io::Read;
                    let result = (|| {
                        let Some(path) = rfd::FileDialog::new()
                            .add_filter("Knowledge archives", &["json", "jsonl", "xml"])
                            .pick_file()
                        else {
                            return Ok(None);
                        };
                        let mut text = String::new();
                        std::fs::File::open(&path)
                            .map_err(|error| error.to_string())?
                            .take((store::knowledge::BUNDLE_LIMIT + 1) as u64)
                            .read_to_string(&mut text)
                            .map_err(|error| error.to_string())?;
                        if text.len() > store::knowledge::BUNDLE_LIMIT {
                            return Err("Choose a knowledge file of at most 16 MiB".into());
                        }
                        Ok(Some((
                            format!("Loaded {} ({} bytes)", path.display(), text.len()),
                            text,
                        )))
                    })();
                    let _ = sender.send(result);
                    if let Some(wake) = wake {
                        wake.ring();
                    }
                });
                return;
            }
            Self::KnowledgeField(field) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                if view.knowledge_fields.contains(field) {
                    view.knowledge_fields.retain(|known| known != field);
                } else {
                    view.knowledge_fields.push(*field);
                }
                view.dirty = true;
                return;
            }
            Self::KnowledgeSource(source) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                if view.knowledge_selected.contains(source) {
                    view.knowledge_selected.retain(|known| known != source);
                } else if view.knowledge_selected.len() < 8 {
                    view.knowledge_selected.push(source.clone());
                }
                view.dirty = true;
            }
            Self::KnowledgePage(next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.knowledge_page = if *next {
                    view.knowledge_page.saturating_add(1)
                } else {
                    view.knowledge_page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Feature(feature) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.feature = feature.clone();
                view.output = None;
                view.dirty = true;
            }
            Self::PackagePage(next) | Self::DefinitionPage(next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                let page = if matches!(self, Self::PackagePage(_)) {
                    &mut view.package_page
                } else {
                    &mut view.definition_page
                };
                *page = if *next {
                    page.saturating_add(1)
                } else {
                    page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Preferred | Self::Personal => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                if matches!(self, Self::Preferred) {
                    view.preferred = !view.preferred;
                } else {
                    view.personal = !view.personal;
                }
                view.dirty = true;
            }
            Self::Term(uid) => {
                let term = view.rows[1]
                    .iter()
                    .flat_map(|row| row["terms"].as_array().into_iter().flatten())
                    .find(|term| term["uid"] == *uid)
                    .cloned();
                if let Some(term) = term {
                    let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                    view.term = uid.clone();
                    view.preferred = term["preferred"].as_bool().unwrap_or(false);
                    view.personal = term["owner_uid"].is_string();
                    view.dirty = true;
                    let fields = [
                        (view.name, term["name"].as_str().unwrap_or("").to_owned()),
                        (
                            view.language,
                            term["language"].as_str().unwrap_or("").to_owned(),
                        ),
                    ];
                    for (field, text) in fields {
                        if let Some(mut field) = world.get_mut::<bevy::text::EditableText>(field) {
                            field.editor.set_text(&text);
                        }
                    }
                }
            }
            Self::ChoicePage(slot, next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                let page = &mut view.choice_pages[*slot];
                *page = if *next {
                    page.saturating_add(1)
                } else {
                    page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Tab(tab) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.tab = *tab;
                view.page = 0;
                view.output = None;
                view.dirty = true;
                ui::editor(world, owner);
            }
            Self::Choose(slot, uid) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.choices[*slot] = uid.clone();
                if *slot == 1 {
                    view.term.clear();
                    view.page = 0;
                    view.definition_page = 0;
                }
                if *slot != 6 {
                    view.output = None;
                }
                view.dirty = true;
                if *slot == view.tab && *slot < 2 {
                    let name = view.name;
                    let value = view.rows[*slot]
                        .iter()
                        .find(|row| row["uid"] == *uid)
                        .and_then(|row| row["name"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(name) {
                        text.editor.set_text(&value);
                    }
                }
                if *slot == 1 && world.get::<OntologySand>(owner).unwrap().tab == 3 {
                    let view = world.get::<OntologySand>(owner).unwrap();
                    let field = view.definition;
                    let definition = view.rows[1]
                        .iter()
                        .find(|row| row["uid"] == *uid)
                        .and_then(|row| row["definitions"].as_array())
                        .and_then(|versions| versions.last())
                        .and_then(|version| version["definition"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    if let Some(mut field) = world.get_mut::<bevy::text::EditableText>(field) {
                        field.editor.set_text(&definition);
                    }
                }
            }
            Self::Page(next) => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.page = if *next {
                    view.page.saturating_add(1)
                } else {
                    view.page.saturating_sub(1)
                };
                view.dirty = true;
            }
            Self::Refresh => {
                let mut view = world.get_mut::<OntologySand>(owner).unwrap();
                view.requested = [false; 4];
                view.ready = [false; 4];
                panel::status(world, status, "Refreshing…");
            }
            _ => {
                let result = mutation(world, owner, self).and_then(|action| {
                    let id = nucleus::new_uid("ontology-action");
                    crate::native::practice_cells::send(
                        world,
                        owner,
                        ClientMessage::Act {
                            id: id.clone(),
                            action,
                        },
                    )?;
                    world.get_mut::<OntologySand>(owner).unwrap().pending = Some(id);
                    Ok(())
                });
                panel::status(
                    world,
                    status,
                    result.err().unwrap_or_else(|| "Saving…".into()),
                );
            }
        }
    }
}

fn selected(view: &OntologySand, slot: usize) -> Result<String, String> {
    let source = match slot {
        0 => 0,
        1 | 2 => 1,
        3 | 4 | 6 => 2,
        _ => return Err("Invalid selection".into()),
    };
    let uid = &view.choices[slot];
    view.rows[source]
        .iter()
        .any(|row| row["uid"].as_str() == Some(uid))
        .then(|| uid.clone())
        .ok_or_else(|| "Choose an existing entry first".into())
}

#[derive(Component)]
struct KnowledgeFile(tokio::sync::oneshot::Receiver<Result<Option<(String, String)>, String>>);

fn knowledge_package(world: &World, view: &OntologySand) -> Result<String, String> {
    let text = panel::value(world, view.package)?;
    Ok(view
        .knowledge_file
        .as_ref()
        .filter(|(label, _)| *label == text)
        .map_or(text.clone(), |(_, raw)| raw.clone()))
}

fn mutation(
    world: &World,
    owner: Entity,
    command: &Command,
) -> Result<engine::actions::Action, String> {
    use engine::actions::Action as Backend;
    let view = world
        .get::<OntologySand>(owner)
        .ok_or("Ontology is closed")?;
    if crate::native::laboratory::blocked(world, owner)
        && !matches!(
            command,
            Command::KnowledgeSources
                | Command::KnowledgeSearch(_)
                | Command::KnowledgeRead(..)
                | Command::KnowledgePoll
                | Command::KnowledgeCancel
                | Command::KnowledgeExport
                | Command::KnowledgeReview
                | Command::KnowledgeInspectSaved
                | Command::KnowledgeCached(..)
                | Command::InspectMeaning(_)
                | Command::InspectTerminology
                | Command::InspectPublication
                | Command::InspectPublicationAt(_)
                | Command::PreviewPackage
                | Command::PreviewTranslations
                | Command::History(_)
        )
    {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    if !view.ready.iter().all(|ready| *ready) {
        return Err("Wait for the ontology to load, or refresh after an error".into());
    }
    let name = || {
        let name = panel::value(world, view.name)?.trim().to_owned();
        if name.is_empty() || name.chars().count() > 256 {
            Err("Enter a name of 1–256 characters".to_owned())
        } else {
            Ok(name)
        }
    };
    Ok(match command {
        Command::KnowledgeCached(source, item) => Backend::Knowledge {
            request: nucleus::knowledge::Command::ReadCached {
                source: source.clone(),
                item: item.clone(),
            },
        },
        Command::KnowledgeExport => Backend::Knowledge {
            request: nucleus::knowledge::Command::Export {
                record: selected(view, 3)?,
            },
        },
        Command::KnowledgeImport => Backend::Knowledge {
            request: nucleus::knowledge::Command::Import {
                package: knowledge_package(world, view)?,
                organ: selected(view, 6)?,
            },
        },
        Command::KnowledgeReview => Backend::Knowledge {
            request: nucleus::knowledge::Command::ReviewUpdate {
                record: selected(view, 3)?,
            },
        },
        Command::KnowledgeAccept(fork) => Backend::Knowledge {
            request: nucleus::knowledge::Command::ApplyUpdate {
                review: view
                    .knowledge_review
                    .as_ref()
                    .and_then(|review| review["review_uid"].as_str())
                    .ok_or("Review a source update first")?
                    .into(),
                fields: view.knowledge_fields.clone(),
                fork: *fork,
            },
        },
        Command::KnowledgeReplace => Backend::Knowledge {
            request: nucleus::knowledge::Command::ReplaceArchive {
                source: view
                    .knowledge_selected
                    .iter()
                    .find(|source| source.starts_with("ks_"))
                    .ok_or("Select the offline source to replace")?
                    .clone(),
                package: knowledge_package(world, view)?,
            },
        },
        Command::KnowledgeRemove(source) => Backend::Knowledge {
            request: nucleus::knowledge::Command::RemoveSource {
                source: source.clone(),
            },
        },
        Command::KnowledgeDump(format) => Backend::Knowledge {
            request: nucleus::knowledge::Command::ImportDump {
                format: format.clone(),
                text: knowledge_package(world, view)?,
                language: panel::value(world, view.definition)?,
            },
        },
        Command::KnowledgeInspectSaved => Backend::Knowledge {
            request: nucleus::knowledge::Command::InspectSaved {
                record: selected(view, 3)?,
            },
        },
        Command::KnowledgeReference | Command::KnowledgePreviewSave => {
            let item = view
                .knowledge_item
                .as_ref()
                .ok_or("Read a source item first")?;
            let source = item["source"]
                .as_str()
                .ok_or("Choose a source item")?
                .to_owned();
            let item = item["id"]
                .as_str()
                .ok_or("Choose a source item")?
                .to_owned();
            let organ = selected(view, 6)?;
            Backend::Knowledge {
                request: if matches!(command, Command::KnowledgeReference) {
                    nucleus::knowledge::Command::Reference {
                        source,
                        item,
                        organ,
                    }
                } else {
                    nucleus::knowledge::Command::PreviewSave {
                        source,
                        item,
                        organ,
                        selection: {
                            let selected = panel::value(world, view.context)?;
                            let ids = selected
                                .split(',')
                                .map(str::trim)
                                .filter(|id| !id.is_empty())
                                .map(str::to_owned)
                                .collect::<Vec<_>>();
                            nucleus::knowledge::Selection {
                                optional_dependencies: ids.clone(),
                                optional_assets: ids,
                            }
                        },
                    }
                },
            }
        }
        Command::KnowledgeSave => Backend::Knowledge {
            request: nucleus::knowledge::Command::Save {
                preview: view
                    .knowledge_preview
                    .as_ref()
                    .and_then(|preview| preview["preview_uid"].as_str())
                    .ok_or("Review a complete save preview first")?
                    .into(),
            },
        },
        Command::KnowledgeMap(kind) => {
            let item = view
                .knowledge_item
                .as_ref()
                .ok_or("Read the source item before mapping a meaning")?;
            Backend::Knowledge {
                request: nucleus::knowledge::Command::MapMeaning {
                    source: item["source"]
                        .as_str()
                        .ok_or("Choose a source item")?
                        .into(),
                    external: panel::value(world, view.language)?,
                    kind: *kind,
                    concept: selected(view, 1)?,
                },
            }
        }
        Command::KnowledgeSources => Backend::Knowledge {
            request: nucleus::knowledge::Command::Sources,
        },
        Command::KnowledgeSearch(offset) => Backend::Knowledge {
            request: nucleus::knowledge::Command::Search {
                request_id: nucleus::new_uid("kr"),
                sources: view.knowledge_selected.clone(),
                text: panel::value(world, view.name)?,
                offset: *offset,
            },
        },
        Command::KnowledgeRead(source, item) => Backend::Knowledge {
            request: nucleus::knowledge::Command::Read {
                request_id: nucleus::new_uid("kr"),
                source: source.clone(),
                item: item.clone(),
            },
        },
        Command::KnowledgePoll => Backend::Knowledge {
            request: nucleus::knowledge::Command::Poll {
                request_id: view
                    .knowledge_job
                    .clone()
                    .ok_or("No knowledge request is running")?,
            },
        },
        Command::KnowledgeCancel => Backend::Knowledge {
            request: nucleus::knowledge::Command::Cancel {
                request_id: view
                    .knowledge_job
                    .clone()
                    .ok_or("No knowledge request is running")?,
            },
        },
        Command::KnowledgeClear => Backend::Knowledge {
            request: nucleus::knowledge::Command::ClearCache { source: None },
        },
        Command::KnowledgeInstall => Backend::Knowledge {
            request: nucleus::knowledge::Command::InstallArchive {
                package: knowledge_package(world, view)?,
            },
        },
        Command::InspectMeaning(offset) => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::InspectMeaning {
                concept: selected(view, 1)?,
                offset: *offset,
            },
        },
        Command::ExportTranslations => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::ExportTranslations {
                vocabulary: selected(view, 0)?,
            },
        },
        Command::PreviewTranslations => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::PreviewTranslations {
                package: panel::value(world, view.package)?,
            },
        },
        Command::ImportTranslations => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::ImportTranslations {
                package: panel::value(world, view.package)?,
            },
        },
        Command::PreviewFeature(enabled, write) => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::PreviewFeature {
                feature: view.feature.clone(),
                concept: selected(view, 1)?,
                enabled: *enabled,
                write: *write,
            },
        },
        Command::ApplyFeature => {
            let preview = view
                .output
                .as_ref()
                .and_then(|output| output.get("feature_preview"))
                .ok_or("Preview the feature change first")?;
            let concept = selected(view, 1)?;
            if preview["before"]["feature"] != view.feature || preview["concept_uid"] != concept {
                return Err("Preview the selected feature and Concept before applying".into());
            }
            if preview["can_apply"] != true {
                return Err("Choose a compatible meaning before applying".into());
            }
            Backend::Vocabulary {
                request: nucleus::vocabulary::Command::BindFeature {
                    feature: view.feature.clone(),
                    concept,
                    enabled: preview["enabled"]
                        .as_bool()
                        .ok_or("Review a complete preview")?,
                    write: preview["write"]
                        .as_bool()
                        .ok_or("Review a complete preview")?,
                    expected_revision: preview["before"]["revision"]
                        .as_i64()
                        .ok_or("Refresh the feature preview")?,
                },
            }
        }
        Command::UseWording => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::UseWording {
                concept: selected(view, 1)?,
                wording: name()?,
            },
        },
        Command::Context => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::SetTerminologyContext {
                context: panel::value(world, view.context)?,
            },
        },
        Command::InspectTerminology => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::InspectTerminology,
        },
        Command::PreferTerm => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::PreferTerm {
                concept: selected(view, 1)?,
                term: view.term.clone(),
            },
        },
        Command::HideTerm(hidden) => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::HideTerm {
                term: view.term.clone(),
                hidden: *hidden,
            },
        },
        Command::RestoreOriginal => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::RestoreOriginal {
                concept: selected(view, 1)?,
            },
        },
        Command::ResetTerminology => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::ResetTerminology {
                concept: selected(view, 1)?,
            },
        },
        Command::InspectPublicationAt(publication) => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::InspectPublication {
                publication: publication.clone(),
            },
        },
        Command::ReviseDefinition => {
            let concept = selected(view, 1)?;
            let version = view.rows[1]
                .iter()
                .find(|row| row["uid"] == concept)
                .and_then(|row| row["definitions"].as_array())
                .and_then(|versions| versions.last())
                .and_then(|version| version["revision"].as_i64())
                .ok_or("Inspect the definition before revising it")?;
            Backend::Vocabulary {
                request: nucleus::vocabulary::Command::ReviseDefinition {
                    concept,
                    expected_revision: version,
                    definition: panel::value(world, view.definition)?,
                },
            }
        }
        Command::Publish => {
            let vocabulary = selected(view, 0)?;
            let expected_revision = view.rows[0]
                .iter()
                .find(|row| row["uid"] == vocabulary)
                .and_then(|row| row["revision"].as_i64())
                .ok_or("Refresh the selected Vocabulary")?;
            Backend::Vocabulary {
                request: nucleus::vocabulary::Command::Publish {
                    vocabulary,
                    expected_revision,
                },
            }
        }
        Command::Withdraw | Command::InspectPublication | Command::Adopt => {
            let vocabulary = selected(view, 0)?;
            let publication = view.rows[0]
                .iter()
                .find(|row| row["uid"] == vocabulary)
                .and_then(|row| row["publications"].as_array())
                .and_then(|publications| {
                    publications
                        .iter()
                        .rev()
                        .find(|publication| publication["withdrawn"] == false)
                })
                .and_then(|publication| publication["uid"].as_str())
                .ok_or("Choose a published Vocabulary")?
                .to_owned();
            Backend::Vocabulary {
                request: match command {
                    Command::Withdraw => nucleus::vocabulary::Command::Withdraw { publication },
                    Command::InspectPublication => {
                        nucleus::vocabulary::Command::InspectPublication { publication }
                    }
                    _ => nucleus::vocabulary::Command::Adopt {
                        publication,
                        organ: selected(view, 6)?,
                    },
                },
            }
        }
        Command::PreviewPackage => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::PreviewPackage {
                package: panel::value(world, view.package)?,
            },
        },
        Command::AdoptPackage => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::AdoptPackage {
                package: panel::value(world, view.package)?,
                organ: selected(view, 6)?,
            },
        },
        Command::History(offset) => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::RecordHistory {
                record: selected(view, 3)?,
                offset: *offset,
            },
        },
        Command::Create if view.tab == 3 => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::CreateConcept {
                vocabulary: selected(view, 0)?,
                name: name()?,
                language: panel::value(world, view.language)?.trim().into(),
                definition: panel::value(world, view.definition)?.trim().into(),
                derived_from: None,
            },
        },
        Command::AddTerm => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::AddTerm {
                concept: selected(view, 1)?,
                name: name()?,
                language: panel::value(world, view.language)?.trim().into(),
                preferred: view.preferred,
                personal: view.personal,
            },
        },
        Command::EditTerm | Command::RetireTerm => {
            let term = view.rows[1]
                .iter()
                .find(|row| row["uid"] == selected(view, 1).unwrap_or_default())
                .and_then(|row| row["terms"].as_array())
                .and_then(|terms| terms.iter().find(|term| term["uid"] == view.term))
                .ok_or("Choose a term of the selected Concept")?;
            let revision = term["revision"]
                .as_i64()
                .ok_or("Refresh the selected term")?;
            Backend::Vocabulary {
                request: if matches!(command, Command::EditTerm) {
                    nucleus::vocabulary::Command::EditTerm {
                        term: view.term.clone(),
                        expected_revision: revision,
                        name: name()?,
                        language: panel::value(world, view.language)?.trim().into(),
                        preferred: view.preferred,
                    }
                } else {
                    nucleus::vocabulary::Command::RetireTerm {
                        term: view.term.clone(),
                        expected_revision: revision,
                    }
                },
            }
        }
        Command::Language => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::SetLanguage {
                language: panel::value(world, view.language)?.trim().into(),
            },
        },
        Command::Create if view.tab == 0 => Backend::Vocabulary {
            request: nucleus::vocabulary::Command::CreateVocabulary {
                name: name()?,
                organ: selected(view, 6)?,
            },
        },
        Command::Create => Backend::CreateConcept {
            lingua: selected(view, 0)?,
            name: name()?,
            parents: vec![],
        },
        Command::Rename if view.tab == 0 => Backend::RenameLingua {
            lingua: selected(view, 0)?,
            name: name()?,
        },
        Command::Rename => Backend::RenameConcept {
            concept: selected(view, 1)?,
            name: name()?,
        },
        Command::Delete if view.tab == 0 => {
            let lingua = selected(view, 0)?;
            if lingua == "g_local" {
                return Err("The local Lingua is permanent".into());
            }
            Backend::DeleteLingua { lingua }
        }
        Command::Delete => Backend::DeleteConcept {
            concept: selected(view, 1)?,
        },
        Command::Include(true) => Backend::AdoptConcept {
            lingua: selected(view, 0)?,
            concept: selected(view, 1)?,
        },
        Command::Include(false) => Backend::RemoveConceptFromLingua {
            lingua: selected(view, 0)?,
            concept: selected(view, 1)?,
        },
        Command::Parent(true) => Backend::AddConceptParent {
            concept: selected(view, 1)?,
            parent: selected(view, 2)?,
        },
        Command::Parent(false) => Backend::RemoveConceptParent {
            concept: selected(view, 1)?,
            parent: selected(view, 2)?,
        },
        Command::Assert => Backend::AssertRecord {
            subject: selected(view, 3)?,
            predicate: selected(view, 1)?,
            object: if view.choices[4].is_empty() {
                None
            } else {
                Some(selected(view, 4)?)
            },
            quantity: None,
            unit: None,
        },
        Command::Identity => Backend::SetIdentity {
            subject: selected(view, 3)?,
            predicate: Some(selected(view, 1)?),
        },
        Command::Retract(uid) => {
            if !view.rows[3].iter().any(|row| row["uid"] == *uid) {
                return Err("This assertion is no longer available".into());
            }
            Backend::RetractAssertion {
                assertion: uid.clone(),
            }
        }
        _ => return Err("Choose an operation".into()),
    })
}

fn query(source: protein::Source) -> protein::Protein {
    protein::Protein {
        source,
        filter: vec![],
        fields: (source == protein::Source::Record)
            .then(|| ["uid", "head", "slug", "kind"].map(str::to_owned).into()),
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    let view = world.get::<OntologySand>(owner).unwrap();
    let status = view.status;
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if view.ids.contains(id) =>
        {
            let index = view
                .ids
                .iter()
                .position(|candidate| candidate == id)
                .unwrap();
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            view.rows[index] = rows.clone();
            if index == 2 && view.choices[6].is_empty() {
                view.choices[6] = rows
                    .iter()
                    .find(|row| row["slug"] == store::organs::LOCAL_ORGAN_SLUG)
                    .and_then(|row| row["uid"].as_str())
                    .unwrap_or("")
                    .to_owned();
            }
            view.ready[index] = true;
            view.dirty = true;
            if view.ready.iter().all(|ready| *ready) && view.pending.is_none() {
                panel::status(world, status, "Live");
            }
        }
        ServerMessage::ActionOk {
            id, warnings, data, ..
        } if view.pending.as_ref() == Some(id) => {
            let mut refresh = true;
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            if let Some(data) = data {
                if !data["preview_uid"].is_null() {
                    view.knowledge_preview = Some(data.clone());
                }
                if !data["review_uid"].is_null() {
                    view.knowledge_review = Some(data.clone());
                }
                if let Some(item) = data.get("item").filter(|item| !item.is_null()) {
                    view.knowledge_item = Some(item.clone());
                    view.knowledge_claim = None;
                    view.knowledge_availability = if data["cached"] == true {
                        "temporary content available offline"
                    } else if data["saved"] == true {
                        "saved for offline use"
                    } else {
                        "reference only"
                    }
                    .into();
                    view.knowledge_page = 0;
                }
                if let Some(uid) = data["record_uid"].as_str() {
                    view.choices[3] = uid.into();
                }
                if data["cleared"] == true {
                    view.knowledge_item = None;
                    view.knowledge_claim = None;
                    view.knowledge_job = None;
                    view.knowledge_results.clear();
                    view.knowledge_preview = None;
                    view.knowledge_review = None;
                    view.knowledge_cached.clear();
                    view.knowledge_file = None;
                    view.knowledge_cache_bytes = 0;
                    refresh = false;
                }
                if let Some(sources) = data["sources"].as_array() {
                    view.knowledge_sources = sources.clone();
                    view.knowledge_cached =
                        data["cached_items"].as_array().cloned().unwrap_or_default();
                    view.knowledge_cache_bytes = data["cache_bytes"].as_u64().unwrap_or(0);
                    refresh = false;
                }
                if let Some(request) = data["request_id"].as_str() {
                    refresh = false;
                    view.knowledge_job = (data["complete"] != true).then(|| request.to_owned());
                    view.knowledge_poll =
                        std::time::Instant::now() + std::time::Duration::from_millis(300);
                    if let Some(sources) = data["result"]["sources"].as_array() {
                        view.knowledge_results = sources.clone();
                        view.page = 0;
                    }
                    if !data["result"]["item"].is_null() {
                        view.knowledge_page = 0;
                        view.knowledge_item = Some(data["result"]["item"].clone());
                        view.knowledge_claim = None;
                        view.knowledge_availability = "cached for this Person".into();
                    }
                }
                view.output = Some(data.clone());
                view.package_page = 0;
                let package = view.package;
                let context = view.context;
                if let Some(text) = data["terminology"]["context"].as_str()
                    && let Some(mut field) = world.get_mut::<bevy::text::EditableText>(context)
                {
                    field.editor.set_text(text);
                }
                if !data["publication"].is_null() {
                    let text = data["publication"].to_string();
                    if let Some(mut field) = world.get_mut::<bevy::text::EditableText>(package) {
                        field.editor.set_text(&text);
                    }
                }
                if !data["translations"].is_null() {
                    let text = data["translations"].to_string();
                    if let Some(mut field) = world.get_mut::<bevy::text::EditableText>(package) {
                        field.editor.set_text(&text);
                    }
                }
                if !data["package"].is_null()
                    && let Some(mut field) = world.get_mut::<bevy::text::EditableText>(package)
                {
                    field.editor.set_text(&data["package"].to_string());
                }
            }
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            view.pending = None;
            view.dirty = true;
            if refresh {
                view.requested = [false; 4];
                view.ready = [false; 4];
            }
            panel::status(
                world,
                status,
                if let Some(status) = data.as_ref().and_then(|data| data["status"].as_str()) {
                    match status {
                        "loading" => "Loading knowledge…",
                        "complete" => "Knowledge loaded",
                        "cancelled" => "Knowledge loading cancelled",
                        "failed" => "Knowledge source unavailable",
                        _ => "Live",
                    }
                    .into()
                } else if data.as_ref().is_some_and(|data| data["cleared"] == true) {
                    "Temporary content cleared".into()
                } else if data.as_ref().is_some_and(|data| data["sources"].is_array()) {
                    "Live".into()
                } else if warnings.is_empty() {
                    "Saved".into()
                } else {
                    format!("Saved · {}", warnings.join(" · "))
                },
            );
        }
        ServerMessage::Error { id, message, .. }
            if view.ids.contains(id)
                || view.pending.as_ref() == Some(id)
                || id == crate::native::cell_bridge::CONNECTION =>
        {
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            if let Some(index) = view.ids.iter().position(|candidate| candidate == id) {
                view.ready[index] = false;
            }
            if id == crate::native::cell_bridge::CONNECTION {
                view.ready = [false; 4];
            }
            if view.pending.as_ref() == Some(id) || id == crate::native::cell_bridge::CONNECTION {
                view.pending = None;
                view.knowledge_job = None;
            }
            panel::status(world, status, message);
        }
        _ => {}
    }
}

pub(crate) fn inspect_sample(world: &mut World, owner: Entity) {
    Command::Tab(1).apply(world, owner);
}

pub(crate) fn sample_visible(world: &World, owner: Entity) -> bool {
    world.get::<OntologySand>(owner).is_some_and(|view| {
        view.tab == 1
            && view.ready[1]
            && view.rows[1].iter().any(|row| {
                row["name"] == "practice-note"
                    && row["parents"]
                        .as_array()
                        .is_some_and(|parents| !parents.is_empty())
            })
    })
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<OntologySand>>()
        .iter(world)
        .collect();
    let active: HashSet<_> = owners
        .iter()
        .flat_map(|owner| world.get::<OntologySand>(*owner).unwrap().ids.clone())
        .collect();
    let stale: Vec<_> = world
        .resource::<Subscriptions>()
        .0
        .keys()
        .filter(|id| !active.contains(*id))
        .cloned()
        .collect();
    for id in stale {
        let sender = world.resource::<Subscriptions>().0[&id].clone();
        if sender
            .try_send(ClientMessage::Unsubscribe { id: id.clone() })
            .is_ok()
            || sender.is_closed()
        {
            world.resource_mut::<Subscriptions>().0.remove(&id);
        }
    }
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::native::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for owner in owners {
        for message in &messages {
            receive(world, owner, message);
        }
        for (index, source) in SOURCES.iter().enumerate() {
            let view = world.get::<OntologySand>(owner).unwrap();
            if !view.requested[index] && !crate::native::laboratory::blocked(world, owner) {
                let id = view.ids[index].clone();
                if let Some(sender) = crate::native::practice_cells::sender(world, owner)
                    && sender
                        .try_send(ClientMessage::Subscribe {
                            id: id.clone(),
                            protein: query(*source),
                        })
                        .is_ok()
                {
                    world.resource_mut::<Subscriptions>().0.insert(id, sender);
                    world.get_mut::<OntologySand>(owner).unwrap().requested[index] = true;
                }
            }
        }
        if let Some(mut file) = world.get_mut::<KnowledgeFile>(owner)
            && let Ok(result) = file.0.try_recv()
        {
            world.entity_mut(owner).remove::<KnowledgeFile>();
            let view = world.get::<OntologySand>(owner).unwrap();
            let (package, status) = (view.package, view.status);
            match result {
                Ok(Some((label, text))) => {
                    if let Some(mut field) = world.get_mut::<bevy::text::EditableText>(package) {
                        field.editor.set_text(&label);
                    }
                    world.get_mut::<OntologySand>(owner).unwrap().knowledge_file =
                        Some((label, text));
                    panel::status(world, status, "Knowledge file ready to inspect or import");
                }
                Ok(None) => {}
                Err(error) => panel::status(world, status, error),
            }
        }
        let view = world.get::<OntologySand>(owner).unwrap();
        let poll = view.tab == 5
            && view.knowledge_job.is_some()
            && view.pending.is_none()
            && view.knowledge_poll <= std::time::Instant::now()
            && view.ready.iter().all(|ready| *ready);
        let filter = (view.tab == 5).then(|| panel::value(world, view.name).unwrap_or_default());
        if let Some(filter) = filter
            && filter != view.knowledge_filter
        {
            let mut view = world.get_mut::<OntologySand>(owner).unwrap();
            view.knowledge_filter = filter;
            view.dirty = true;
        }
        if poll {
            Command::KnowledgePoll.apply(world, owner);
        }
        if world.get::<OntologySand>(owner).unwrap().dirty {
            ui::render(world, owner);
        }
    }
}
