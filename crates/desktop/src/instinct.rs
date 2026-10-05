mod habit_ui;
pub(crate) mod import_ui;
mod persistence;
pub(crate) mod practice;
mod reader;
pub(crate) mod tests;

use crate::{actions::Action, canvas::CanvasItem, workspace::WorkspaceMember};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(crate) use persistence::{SavedInstinct, snapshot};

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::SYMBOLS,
    crate::credits::DEJAVU,
    crate::credits::FONTIQUE,
    crate::credits::Attribution {
        name: "pulldown-cmark",
        author: "Raph Levien and contributors",
        license: include_str!("../licenses/pulldown-cmark-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "mermaid-rs-renderer",
        author: "1jehuang and contributors",
        license: include_str!("../licenses/mermaid-rs-renderer-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/resvg-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "AccessKit",
        author: "The AccessKit contributors",
        license: crate::credits::ACCESSKIT_LICENSE,
    },
    crate::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    crate::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
];

#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instinct {
    pub page: Option<String>,
}

fn teaching_text(body: &str) -> &str {
    let body = body.trim();
    body.strip_prefix("\"\"\"")
        .and_then(|body| body.strip_suffix("\"\"\""))
        .map_or(body, str::trim)
}

impl Instinct {
    pub(crate) fn valid(&self) -> bool {
        self.page.as_ref().is_none_or(|page| {
            !page.is_empty() && page.len() <= 256 && !page.chars().any(char::is_control)
        })
    }
}

#[derive(Component)]
pub struct SeedInstinct;

#[derive(Clone)]
struct Page {
    id: String,
    title: String,
    sections: Vec<Section>,
}

#[derive(Clone)]
struct Entry {
    id: String,
    uid: String,
    page: String,
    title: String,
    depth: usize,
    chapter: String,
}

#[derive(Clone)]
struct Section {
    slug: Option<String>,
    uid: String,
    title: String,
    body: String,
}

pub struct InstinctPlugin;
impl Plugin for InstinctPlugin {
    fn build(&self, app: &mut App) {
        if cfg!(feature = "instinct") {
            app.init_resource::<crate::practice_cells::persistence::Restoration>()
                .add_systems(
                    Update,
                    crate::practice_cells::persistence::restore
                        .after(crate::workspace::PrepareWorkspaces)
                        .run_if(crate::practice_cells::persistence::needed),
                );
        }
        app.init_resource::<practice::Learned>()
            .add_systems(
                PreUpdate,
                practice::emergency
                    .after(bevy::input::InputSystems)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch)
                    .run_if(practice::active),
            )
            .add_systems(
                PreUpdate,
                practice::refresh_input
                    .after(bevy::input::InputSystems)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch)
                    .before(bevy::picking::PickingSystems::Hover)
                    .run_if(practice::advancing.and_then(practice::input_changed)),
            )
            .add_systems(
                PreUpdate,
                practice::guard_focus
                    .before(bevy::input_focus::InputFocusSystems::Dispatch)
                    .run_if(practice::advancing),
            )
            .add_systems(
                Update,
                practice::update
                    .after(crate::protein_area::UpdateProteinAreas)
                    .run_if(practice::active),
            )
            .add_systems(
                Update,
                import_ui::receive
                    .after(crate::cell_bridge::ReceiveCell)
                    .run_if(import_ui::active),
            );
        app.init_resource::<habit_ui::Subscriptions>().add_systems(
            Update,
            habit_ui::tick
                .after(crate::cell_bridge::ReceiveCell)
                .run_if(habit_ui::active),
        );
        app.add_systems(
            PostUpdate,
            (reader::scroll_to_record, reader::reveal_selection)
                .chain()
                .after(bevy::ui::UiSystems::PostLayout),
        );
    }
}

#[derive(Resource)]
struct Book(Arc<[Page]>, Arc<[Entry]>, Option<String>);

impl Default for Book {
    fn default() -> Self {
        let records = match engine::instinct::records() {
            Ok(records) => records,
            Err(error) => {
                warn!(%error, "could not load Instinct");
                return Self(Arc::from([]), Arc::from([]), Some(error.to_string()));
            }
        };
        let mut pages = Vec::new();
        let mut entries = Vec::new();
        for descriptor in lince_interface::handbook::PAGES {
            if let Some(record) = records
                .iter()
                .find(|record| record.slug.as_deref() == Some(descriptor.slug))
            {
                pages.push(Page {
                    id: descriptor.slug.into(),
                    title: record.head.clone(),
                    sections: vec![Section {
                        slug: record.slug.clone(),
                        uid: record.projection.uid.clone(),
                        title: record.head.clone(),
                        body: record.body.clone(),
                    }],
                });
                entries.push(Entry {
                    id: descriptor.slug.into(),
                    uid: record.projection.uid.clone(),
                    page: descriptor.slug.into(),
                    title: record.head.clone(),
                    depth: 0,
                    chapter: records
                        .iter()
                        .find(|record| record.slug.as_deref() == Some(descriptor.chapter))
                        .map_or_else(
                            || descriptor.chapter.to_owned(),
                            |record| record.head.clone(),
                        ),
                });
            }
        }
        for record in records.iter().filter(|record| {
            !lince_interface::handbook::PAGES
                .iter()
                .any(|page| record.slug.as_deref() == Some(page.slug))
                && !record
                    .slug
                    .as_deref()
                    .is_some_and(|slug| slug.starts_with("step-") || slug.starts_with("instinct-"))
        }) {
            let id = record
                .slug
                .clone()
                .unwrap_or_else(|| record.projection.uid.clone());
            pages.push(Page {
                id: id.clone(),
                title: record.head.clone(),
                sections: vec![Section {
                    slug: record.slug.clone(),
                    uid: record.projection.uid.clone(),
                    title: record.head.clone(),
                    body: record.body.clone(),
                }],
            });
            entries.push(Entry {
                id: id.clone(),
                uid: record.projection.uid.clone(),
                page: id,
                title: record.head.clone(),
                depth: 0,
                chapter: "Reference".into(),
            });
        }
        Self(pages.into(), entries.into(), None)
    }
}

pub(crate) fn follow(world: &mut World, owner: Entity, reference: &str) -> bool {
    let entry = world
        .get_resource::<Book>()
        .and_then(|book| {
            book.1
                .iter()
                .find(|entry| entry.id == reference || entry.uid == reference)
        })
        .cloned();
    let Some(entry) = entry else { return false };
    Command::Page(entry.id).apply(world, owner);
    world
        .entity_mut(owner)
        .insert(reader::ScrollToRecord(entry.uid));
    true
}

#[derive(Component, Default)]
struct View {
    body: Option<Entity>,
    article: Option<Entity>,
    nav: Option<Entity>,
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    instinct: Instinct,
) -> Entity {
    world.init_resource::<Book>();
    let entity = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(CREDITS),
            CanvasItem {
                position,
                size: Vec2::new(720.0, 520.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            if instinct.valid() {
                instinct
            } else {
                Instinct::default()
            },
            View::default(),
        ))
        .id();
    reader::render(world, entity);
    entity
}

#[derive(Clone)]
enum Command {
    Create,
    Page(String),
    Search,
    ClearSearch,
}

#[derive(Component, Default)]
struct Search {
    query: String,
    input: Option<Entity>,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        match self {
            Self::Search | Self::ClearSearch => {
                let input = world.get::<Search>(owner).and_then(|search| search.input);
                let query = if matches!(self, Self::ClearSearch) {
                    String::new()
                } else {
                    input
                        .and_then(|input| world.get::<bevy::text::EditableText>(input))
                        .map(|input| input.value().to_string())
                        .unwrap_or_default()
                };
                world
                    .entity_mut(owner)
                    .insert(Search { query, input: None });
                reader::render(world, owner);
            }
            Self::Create => {
                let Some(workspace) = world
                    .get::<crate::workspace::Workspaces>(owner)
                    .map(|w| w.active)
                else {
                    return;
                };
                let position = world
                    .get::<crate::canvas::CanvasView>(owner)
                    .map_or(DVec2::ZERO, |view| view.center);
                spawn(world, owner, workspace, position, Instinct::default());
            }
            Self::Page(page) => {
                if !world
                    .get_resource::<Book>()
                    .is_some_and(|book| book.1.iter().any(|entry| entry.id == *page))
                {
                    return;
                }
                let Some(mut instinct) = world.get_mut::<Instinct>(owner) else {
                    return;
                };
                instinct.page = Some(page.clone());
                world.entity_mut(owner).remove::<reader::ScrollToRecord>();
                reader::render(world, owner);
            }
        }
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    if !cfg!(feature = "instinct") {
        return;
    }
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Instinct",
        "Read Lince's guide and tutorials.",
        Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, Instinct::default()),
    );
}
