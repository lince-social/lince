use crate::{
    actions::Action,
    castle_feed::{self, Frame},
    protein_area::Source,
};
use bevy::{math::DVec2, prelude::*, text::EditableText};

#[cfg(test)]
mod tests;

#[derive(Component)]
pub struct ShaderCastle {
    slug: Entity,
}

#[derive(Component)]
pub(crate) struct ShaderFeed;

pub struct ShaderCastlePlugin;
impl Plugin for ShaderCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update.after(crate::protein_area::UpdateProteinAreas),
        );
    }
}

fn config(slug: &str) -> crate::protein_area::Config {
    let mut config = crate::full_record::config(slug, Source::Local);
    config.enabled = !slug.is_empty();
    config.group_with_source = false;
    config.width = 880.0;
    config.max_height = Some(620.0);
    config.bindings.retain(|binding| binding.property == "body");
    config.draft.name = "Shader".into();
    config
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    slug: &str,
) -> Entity {
    let owner = castle_feed::spawn(
        world,
        root,
        workspace,
        position,
        Vec2::new(920.0, 760.0),
        "Shader Castle",
        config(slug),
    );
    world
        .entity_mut(owner)
        .insert(crate::sand_store::SandCredits(crate::description::CREDITS));
    let frame = world.get::<Frame>(owner).unwrap();
    let (header, area) = (frame.header, frame.area);
    world.entity_mut(area).insert(ShaderFeed);
    crate::edit_mode::label(world, header, "Record slug", 14.0);
    let input = world
        .spawn(crate::sand::text_editor(
            slug,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .insert((
            Node {
                width: px(220),
                height: px(34),
                ..default()
            },
            ChildOf(header),
        ))
        .id();
    {
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.max_characters = Some(256);
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
    }
    castle_feed::button(world, header, owner, "Open", Select);
    castle_feed::button(world, header, owner, "Add glowing ball", AddBall);
    world.entity_mut(owner).insert(ShaderCastle { slug: input });
    let hint = crate::edit_mode::label(
        world,
        owner,
        "In a ```wgsl block, write fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32>. Inputs: size, time, delta, mouse (x, y, pressed, inside). Changes save to the Record.",
        12.0,
    );
    world.get_mut::<Node>(hint).unwrap().flex_shrink = 0.0;
    owner
}

#[derive(Clone)]
struct Select;
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let Some(castle) = world.get::<ShaderCastle>(owner) else {
            return;
        };
        let slug = world
            .get::<EditableText>(castle.slug)
            .unwrap()
            .value()
            .to_string();
        let slug = slug.trim().trim_start_matches(['@', '#']);
        let frame = world.get::<Frame>(owner).unwrap();
        let area = frame.area;
        world
            .get_mut::<crate::area::InfluenceArea>(area)
            .unwrap()
            .protein = Some(config(slug));
    }
}

#[derive(Clone)]
struct AddBall;
impl Action for AddBall {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        let Some(frame) = world.get::<Frame>(owner) else {
            return;
        };
        let area = frame.area;
        let editors: Vec<_> = world
            .query::<(
                Entity,
                &crate::protein_area::RecordBinding,
                &crate::record_binding::TextBinding,
            )>()
            .iter(world)
            .filter(|(_, record, binding)| record.area == area && binding.property == "body")
            .map(|(entity, _, _)| entity)
            .collect();
        for entity in editors {
            let mut text = world.get_mut::<EditableText>(entity).unwrap();
            let source = text.value().to_string();
            text.editor.set_text(&format!(
                "{source}\n\n```wgsl\n{}```\n",
                crate::description::SHADER_EXAMPLE
            ));
        }
    }
}

fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<ShaderCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let frame = world.get::<Frame>(owner).unwrap();
        let (area, status, viewport) = (frame.area, frame.status, frame.viewport);
        let enabled = world
            .get::<crate::area::InfluenceArea>(area)
            .and_then(|area| area.protein.as_ref())
            .is_some_and(|config| config.enabled);
        let live = crate::protein_area::calendar_status(world, area);
        let empty =
            crate::protein_area::calendar_feed(world, area).is_none_or(|(rows, _)| rows.is_empty());
        let saved_status = crate::protein_area::calendar_feed(world, area)
            .and_then(|(rows, _)| rows.first())
            .and_then(|row| row["uid"].as_str())
            .and_then(|uid| {
                crate::record_binding::status(
                    world,
                    &crate::protein_area::RecordBinding {
                        area,
                        uid: uid.into(),
                        source: Source::Local,
                    },
                )
            });
        let message = if !enabled {
            "Enter a Record slug and press Open"
        } else if live == "Live" && empty {
            "No Record with that slug"
        } else {
            live
        }
        .to_string();
        let message = if live == "Live" && !empty {
            saved_status.unwrap_or(message)
        } else {
            message
        };
        world.get_mut::<Text>(status).unwrap().0 = message;
        let size = world
            .get::<ComputedNode>(viewport)
            .map_or(Vec2::new(904.0, 640.0), |node| {
                node.size() * node.inverse_scale_factor()
            });
        for card in castle_feed::cards(world, area) {
            let mut item = world.get_mut::<crate::canvas::CanvasItem>(card).unwrap();
            item.position = DVec2::new(0.0, f64::from((item.size.y - size.y) * 0.5));
        }
    }
}

#[derive(Clone)]
struct Create;
impl Action for Create {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(workspace) = world
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return;
        };
        let position = world
            .get::<crate::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        spawn(world, root, workspace, position, "");
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Shader Castle",
        "Edit a Record description by slug with live WGSL previews.",
        Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, ""),
    );
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedShader(pub castle_feed::Saved);
impl SavedShader {
    pub fn valid(&self) -> bool {
        self.0.valid()
    }
    pub fn restore(self, world: &mut World, root: Entity) {
        let slug = self.0.config.draft.query["where"][0]["all"][0]["slug_eq"]
            .as_str()
            .unwrap_or_default();
        let owner = spawn(
            world,
            root,
            self.0.workspace,
            DVec2::from_array(self.0.position),
            slug,
        );
        self.0.apply(world, owner);
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedShader> {
    world
        .query_filtered::<(Entity, &ChildOf), With<ShaderCastle>>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .filter_map(|(owner, _)| castle_feed::Saved::capture(world, owner).map(SavedShader))
        .collect()
}
