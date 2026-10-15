use crate::native::{
    actions::Action,
    castle_feed::{self, Frame},
    protein_area::{RecordBinding, Source},
    shader_effects,
};
use bevy::{math::DVec2, prelude::*, text::EditableText};

#[cfg(test)]
mod tests;

#[derive(Component)]
pub struct ShaderCastle {
    slug: Entity,
    editor: Option<Entity>,
    selection: Option<RecordBinding>,
    drafts: Vec<shader_effects::SavedEditor>,
    create_requested: bool,
}

#[derive(Component)]
pub(crate) struct ShaderFeed;

pub struct ShaderCastlePlugin;
impl Plugin for ShaderCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update
                .after(crate::native::protein_area::UpdateProteinAreas)
                .before(shader_effects::UpdateShaderEffects),
        );
    }
}

fn config(slug: &str, source: Source) -> crate::native::protein_area::Config {
    let mut config = crate::native::full_record::config(slug, source);
    config.enabled = !slug.is_empty();
    config.group_with_source = false;
    config.shader_effects = false;
    config.width = 880.0;
    config.max_height = Some(620.0);
    config.bindings.retain(|binding| binding.property == "head");
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
        Vec2::new(920.0, 900.0),
        "Shader Castle",
        config(slug, Source::Local),
    );
    world
        .entity_mut(owner)
        .insert(crate::native::sand_store::SandCredits(
            crate::native::description::CREDITS,
        ));
    let header = world.get::<Frame>(owner).unwrap().header;
    let area = world.get::<Frame>(owner).unwrap().area;
    world.entity_mut(area).insert(ShaderFeed);
    crate::native::edit_mode::label(world, header, "Record slug", 14.0);
    let input = world
        .spawn(crate::native::sand::text_editor(
            slug,
            world.resource::<crate::native::theme::Typography>(),
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
    castle_feed::button(
        world,
        header,
        owner,
        "Create extension / Energy example",
        AddBall,
    );
    world.entity_mut(owner).insert(ShaderCastle {
        slug: input,
        editor: None,
        selection: None,
        drafts: vec![],
        create_requested: false,
    });
    crate::native::edit_mode::label(
        world,
        owner,
        "Edit the Record's WGSL extension below. Save / Apply publishes a validated effect. The Record description is unchanged; drafts are kept separately for up to eight selected Records.",
        12.0,
    );
    owner
}

fn remember(world: &mut World, owner: Entity) {
    let Some(castle) = world.get::<ShaderCastle>(owner) else {
        return;
    };
    let (Some(editor), Some(binding)) = (castle.editor, castle.selection.clone()) else {
        return;
    };
    if let Some(saved) = shader_effects::capture_editor(world, editor) {
        let mut castle = world.get_mut::<ShaderCastle>(owner).unwrap();
        castle.drafts.retain(|draft| !draft.matches(&binding));
        castle.drafts.push(saved);
        if castle.drafts.len() > 8 {
            castle.drafts.remove(0);
        }
    }
}

fn detach(world: &mut World, owner: Entity) {
    remember(world, owner);
    let mut castle = world.get_mut::<ShaderCastle>(owner).unwrap();
    let editor = castle.editor.take();
    castle.selection = None;
    castle.create_requested = false;
    if let Some(editor) = editor {
        world.despawn(editor);
    }
}

#[derive(Clone)]
struct Select;
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
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
        let area = world.get::<Frame>(owner).unwrap().area;
        let source = world
            .get::<crate::native::area::InfluenceArea>(area)
            .and_then(|area| area.protein.as_ref())
            .map_or(Source::Local, |config| config.source.clone());
        detach(world, owner);
        world
            .get_mut::<crate::native::area::InfluenceArea>(area)
            .unwrap()
            .protein = Some(config(slug, source));
    }
}

#[derive(Clone)]
struct AddBall;
impl Action for AddBall {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        &[lince_interface::practice::Operation::AddShaderExample]
    }
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        if let Some(mut castle) = world.get_mut::<ShaderCastle>(owner) {
            castle.create_requested = castle.selection.is_some();
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
        let config = world
            .get::<crate::native::area::InfluenceArea>(area)
            .and_then(|area| area.protein.clone());
        let live = crate::native::protein_area::calendar_status(world, area).to_string();
        let selected = crate::native::protein_area::calendar_feed(world, area)
            .and_then(|(rows, _)| rows.first())
            .and_then(|row| row["uid"].as_str())
            .map(|uid| RecordBinding {
                area,
                uid: uid.into(),
                source: config
                    .as_ref()
                    .map_or(Source::Local, |config| config.source.clone()),
            });
        let previous = world.get::<ShaderCastle>(owner).unwrap().selection.clone();
        let changed = match (&previous, &selected) {
            (Some(a), Some(b)) => a.uid != b.uid || a.source != b.source,
            (None, None) => false,
            _ => true,
        };
        if changed {
            detach(world, owner);
            if let Some(binding) = selected.clone() {
                let saved = world
                    .get::<ShaderCastle>(owner)
                    .unwrap()
                    .drafts
                    .iter()
                    .find(|draft| draft.matches(&binding))
                    .cloned();
                let root = world.get::<ChildOf>(owner).unwrap().parent();
                let editor =
                    shader_effects::embed_record(world, root, viewport, binding.clone(), saved);
                let mut castle = world.get_mut::<ShaderCastle>(owner).unwrap();
                castle.editor = Some(editor);
                castle.selection = Some(binding);
            }
        }
        for card in castle_feed::cards(world, area) {
            world.entity_mut(card).insert(castle_feed::HiddenCard);
        }
        let castle = world.get::<ShaderCastle>(owner).unwrap();
        let request = castle.create_requested;
        let editor = castle.editor;
        if request
            && let Some(editor) = editor
            && shader_effects::create_example(world, editor)
        {
            world
                .get_mut::<ShaderCastle>(owner)
                .unwrap()
                .create_requested = false;
        }
        let message = if config.as_ref().is_none_or(|config| !config.enabled) {
            "Enter a Record slug and press Open".into()
        } else if selected.is_none() && live == "Live" {
            "No Record with that slug".into()
        } else {
            live
        };
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != message
        {
            text.0 = message;
        }
    }
}

#[derive(Clone)]
struct Create;
impl Action for Create {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(workspace) = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return;
        };
        let position = world
            .get::<crate::native::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        spawn(world, root, workspace, position, "");
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::native::sand_store::castle_entry(
        world,
        root,
        parent,
        "Shader Castle",
        "Edit and preview a Record's typed WGSL extension, rules and input mappings.",
        Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, ""),
    );
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedShader {
    frame: castle_feed::Saved,
    drafts: Vec<shader_effects::SavedEditor>,
}

impl SavedShader {
    pub(crate) fn workspace(&self) -> u64 {
        self.frame.workspace
    }
    pub fn valid(&self) -> bool {
        self.frame.valid()
            && self.drafts.len() <= 8
            && self.drafts.iter().all(shader_effects::SavedEditor::valid)
    }
    pub fn restore(self, world: &mut World, root: Entity) {
        let slug = self.frame.config.draft.query["where"][0]["all"][0]["slug_eq"]
            .as_str()
            .unwrap_or_default();
        let owner = spawn(
            world,
            root,
            self.frame.workspace,
            DVec2::from_array(self.frame.position),
            slug,
        );
        self.frame.apply(world, owner);
        world.get_mut::<ShaderCastle>(owner).unwrap().drafts = self.drafts;
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedShader> {
    let owners: Vec<_> = world
        .query_filtered::<(Entity, &ChildOf), With<ShaderCastle>>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .map(|(owner, _)| owner)
        .collect();
    owners
        .into_iter()
        .filter_map(|owner| {
            remember(world, owner);
            Some(SavedShader {
                frame: castle_feed::Saved::capture(world, owner)?,
                drafts: world.get::<ShaderCastle>(owner)?.drafts.clone(),
            })
        })
        .collect()
}

pub(crate) fn prepare_source(world: &mut World, owner: Entity, source: Source) {
    detach(world, owner);
    let area = world.get::<Frame>(owner).unwrap().area;
    if let Some(config) = world
        .get_mut::<crate::native::area::InfluenceArea>(area)
        .unwrap()
        .protein
        .as_mut()
    {
        config.source = source;
    }
}

pub(crate) fn add_example(world: &mut World, owner: Entity) {
    if !example_visible(world, owner) {
        AddBall.apply(world, owner);
    }
}

pub(crate) fn example_visible(world: &mut World, owner: Entity) -> bool {
    world
        .get::<ShaderCastle>(owner)
        .and_then(|castle| castle.editor)
        .is_some_and(|editor| shader_effects::example_visible(world, editor))
}
