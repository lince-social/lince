mod persistence;
#[cfg(test)]
mod tests;
pub(crate) mod ui;

use crate::{
    sound::{Command, dsp::Effects},
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
pub(crate) use persistence::{SavedRecorder, snapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct RecorderCastle {
    pub selected: String,
    pub name: String,
    pub effects: BTreeMap<String, Effects>,
}

impl RecorderCastle {
    pub fn valid(&self) -> bool {
        (self.selected.is_empty() || crate::sound::library::valid_path(&self.selected))
            && self.name.chars().count() <= 180
            && self.effects.len() <= 2000
            && self
                .effects
                .iter()
                .all(|(path, effects)| crate::sound::library::valid_path(path) && effects.valid())
    }

    fn effect(&self) -> Effects {
        self.effects
            .get(&self.selected)
            .copied()
            .unwrap_or_default()
    }
}

#[derive(Component)]
struct View {
    list: Entity,
    effects: Entity,
    status: Entity,
    transport: Entity,
    recording: bool,
    revision: u64,
}

pub struct RecorderCastlePlugin;
impl Plugin for RecorderCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (ui::refresh, cleanup)).add_systems(
            PostUpdate,
            ui::inputs
                .after(bevy::text::EditableTextSystems)
                .before(crate::actions::ApplyActions),
        );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: RecorderCastle,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(680.0, 800.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(14)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            castle,
        ))
        .id();
    crate::edit_mode::label(world, owner, "Recorder Castle", 24.0);
    crate::edit_mode::label(world, owner, "MIC INPUT  ·  TRACK FX  ·  WAV LIBRARY", 12.0);
    crate::edit_mode::label(world, owner, "Next take name", 13.0);
    let name = world.get::<RecorderCastle>(owner).unwrap().name.clone();
    let name = ui::input(world, owner, "Name your next recording", &name);
    world.entity_mut(name).insert(ui::Name(owner));
    let transport = ui::row(world, owner);
    let status = crate::edit_mode::label(
        world,
        owner,
        "Ready · saves in Lince/recordings · up to 120 seconds",
        13.0,
    );
    let scroll = ui::stack(world, owner);
    {
        let mut node = world.get_mut::<Node>(scroll).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.overflow = Overflow::scroll_y();
    }
    crate::scroll_sand::attach(world, scroll);
    let effects = ui::stack(world, scroll);
    let list = ui::stack(world, scroll);
    world.entity_mut(owner).insert(View {
        list,
        effects,
        status,
        transport,
        recording: false,
        revision: u64::MAX,
    });
    ui::transport(world, owner);
    ui::effects(world, owner);
    ui::list(world, owner);
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Recorder Castle",
        "Record sounds and shape the selected take with Track FX.",
        ui::Control::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, RecorderCastle::default()),
    );
}

pub(crate) fn status(world: &mut World, owner: Entity, message: &str) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        if let Some(mut text) = world.get_mut::<Text>(entity) {
            text.0 = message.into();
        }
    }
}

pub(crate) fn recording(world: &mut World, owner: Entity, recording: bool) {
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.recording = recording;
        ui::transport(world, owner);
    }
}

pub(crate) fn saved(world: &mut World, owner: Entity, path: String) {
    if let Some(mut castle) = world.get_mut::<RecorderCastle>(owner) {
        castle.selected = path;
        ui::effects(world, owner);
        ui::list(world, owner);
    }
}

fn cleanup(
    mut removed: RemovedComponents<RecorderCastle>,
    audio: Option<Res<crate::sound::Audio>>,
) {
    for owner in removed.read() {
        if let Some(audio) = &audio {
            let _ = audio.send(Command::Cancel(owner));
        }
    }
}
