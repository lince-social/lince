mod persistence;
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
}

#[derive(Clone)]
struct Section {
    uid: String,
    title: String,
    body: String,
    tutorial: bool,
}

pub struct InstinctPlugin;
impl Plugin for InstinctPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (reader::scroll_to_record, reader::reveal_selection)
                .chain()
                .after(bevy::ui::UiSystems::PostLayout),
        );
    }
}

#[derive(Resource)]
struct Book(Arc<[Page]>, Arc<[Entry]>);

impl Default for Book {
    fn default() -> Self {
        let records = engine::instinct::records();
        Self(
            records
                .iter()
                .filter(|record| record.is_entry())
                .map(|entry| Page {
                    id: entry
                        .slug
                        .clone()
                        .unwrap_or_else(|| entry.projection.uid.clone()),
                    title: entry.head.clone(),
                    sections: records
                        .iter()
                        .filter(|record| record.entry_uid(&records) == entry.projection.uid)
                        .map(|record| Section {
                            uid: record.projection.uid.clone(),
                            title: record.head.clone(),
                            body: record.body.clone(),
                            tutorial: record.slug.as_deref() == Some("areas-of-influence"),
                        })
                        .collect(),
                })
                .collect(),
            records
                .iter()
                .map(|record| {
                    let parent = records
                        .iter()
                        .find(|entry| entry.projection.uid == record.entry_uid(&records))
                        .unwrap();
                    Entry {
                        id: record
                            .slug
                            .clone()
                            .unwrap_or_else(|| record.projection.uid.clone()),
                        uid: record.projection.uid.clone(),
                        page: parent
                            .slug
                            .clone()
                            .unwrap_or_else(|| parent.projection.uid.clone()),
                        title: record.head.clone(),
                        depth: record.path(&records).len().saturating_sub(1),
                    }
                })
                .collect(),
        )
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
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        match self {
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
