use bevy::{
    asset::{LoadState, UntypedAssetId},
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    render::renderer::RenderAdapterInfo,
    text::{EditableText, FontSource, TextLayoutInfo},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct GraphicsDevice {
    pub name: Option<String>,
    pub backend: Option<String>,
    pub device_type: Option<String>,
    pub driver: Option<String>,
    pub driver_info: Option<String>,
    pub vendor: Option<u32>,
    pub device: Option<u32>,
    pub status: String,
}

impl GraphicsDevice {
    pub fn read(world: &World) -> Self {
        let Some(info) = world.get_resource::<RenderAdapterInfo>() else {
            return Self {
                status: if world.contains_resource::<bevy::winit::WinitSettings>() {
                    "Graphics device not available"
                } else {
                    "Headless: no graphics device"
                }
                .into(),
                ..default()
            };
        };
        Self {
            name: Some(info.name.clone()),
            backend: Some(format!("{:?}", info.backend)),
            device_type: Some(format!("{:?}", info.device_type)),
            driver: Some(info.driver.clone()),
            driver_info: Some(info.driver_info.clone()),
            vendor: Some(info.vendor),
            device: Some(info.device),
            status: "Selected graphics device".into(),
        }
    }

    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!(
                "Graphics: {name} · {} · {}\nDriver: {} {}",
                self.backend.as_deref().unwrap_or("unknown"),
                self.device_type.as_deref().unwrap_or("unknown"),
                self.driver.as_deref().unwrap_or("unknown"),
                self.driver_info.as_deref().unwrap_or("")
            ),
            None => self.status.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartupIssue {
    pub entity: String,
    pub failed: bool,
    pub reason: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct StartupFailure(pub String);

#[derive(Clone, Debug, Default, Serialize)]
pub struct SandResources {
    pub entity: String,
    pub name: String,
    pub workspace: String,
    pub suspended: bool,
    pub entities: usize,
    pub ui_nodes: usize,
    pub text_bytes: usize,
    pub glyphs: usize,
    pub image_assets: usize,
    pub retained_image_bytes: usize,
    pub record_json_bytes: usize,
    pub event_sources: usize,
    pub pending_writes: usize,
    pub physics_bodies: usize,
    pub awake_bodies: usize,
    pub startup: Vec<StartupIssue>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ResourceSnapshot {
    pub graphics: GraphicsDevice,
    pub sands: Vec<SandResources>,
    pub unique_image_assets: usize,
    pub retained_image_bytes: usize,
}

#[derive(Clone, Copy, Default)]
pub enum ResourceSort {
    #[default]
    Entities,
    Text,
    Images,
    Physics,
}

impl ResourceSort {
    pub fn next(self) -> Self {
        match self {
            Self::Entities => Self::Text,
            Self::Text => Self::Images,
            Self::Images => Self::Physics,
            Self::Physics => Self::Entities,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Entities => "entities",
            Self::Text => "text bytes",
            Self::Images => "image bytes",
            Self::Physics => "awake physics bodies",
        }
    }
    fn value(self, sand: &SandResources) -> usize {
        match self {
            Self::Entities => sand.entities,
            Self::Text => sand.text_bytes,
            Self::Images => sand.retained_image_bytes,
            Self::Physics => sand.awake_bodies,
        }
    }
}

impl ResourceSnapshot {
    pub fn lines(&self, sort: ResourceSort, page: usize) -> Vec<String> {
        let mut rows: Vec<_> = self.sands.iter().collect();
        rows.sort_by(|a, b| {
            b.startup
                .iter()
                .any(|issue| issue.failed)
                .cmp(&a.startup.iter().any(|issue| issue.failed))
                .then_with(|| sort.value(b).cmp(&sort.value(a)))
                .then_with(|| a.entity.cmp(&b.entity))
        });
        let pages = rows.len().div_ceil(8).max(1);
        let mut lines = vec![
            format!(
                "Sands before Laboratory suspension · {} Sands · sorted by {} · page {} / {}",
                rows.len(),
                sort.label(),
                page % pages + 1,
                pages
            ),
            "Counts and retained payloads; CPU/GPU time and VRAM per Sand are not measured.".into(),
            format!(
                "Shared images: {} unique · {} retained bytes (shared references counted once here)",
                self.unique_image_assets, self.retained_image_bytes
            ),
        ];
        for row in rows.into_iter().skip(page % pages * 8).take(8) {
            lines.push(format!(
                "{} [{}] · {}{}",
                row.name,
                row.entity,
                row.workspace,
                if row.suspended {
                    " · already suspended"
                } else {
                    ""
                }
            ));
            lines.push(format!("{} entities · {} UI nodes · {} text bytes · {} glyphs · {} images / {} retained bytes · physics {} awake / {} bodies", row.entities, row.ui_nodes, row.text_bytes, row.glyphs, row.image_assets, row.retained_image_bytes, row.awake_bodies, row.physics_bodies));
            lines.push(format!(
                "{} event sources · {} pending writes · {} record JSON bytes",
                row.event_sources, row.pending_writes, row.record_json_bytes
            ));
            if row.startup.is_empty() {
                lines.push("No startup blocker recorded".into());
            } else {
                for issue in &row.startup {
                    lines.push(format!(
                        "{} [{}]: {}",
                        if issue.failed { "Failed" } else { "Waiting" },
                        issue.entity,
                        issue.reason
                    ));
                }
            }
        }
        lines
    }
}

pub(crate) fn asset_issue(
    world: &World,
    entity: Entity,
    id: UntypedAssetId,
    kind: &str,
    present: bool,
) -> Option<StartupIssue> {
    let state = world
        .get_resource::<AssetServer>()
        .and_then(|server| server.get_load_state(id));
    let (failed, reason) = match state {
        Some(LoadState::Failed(error)) => (true, format!("Cannot load {kind}: {error}")),
        Some(LoadState::NotLoaded | LoadState::Loading) => (false, format!("Loading {kind}")),
        _ if !present => (false, format!("{kind} asset is not available")),
        _ => return None,
    };
    Some(StartupIssue {
        entity: entity.to_string(),
        failed,
        reason,
    })
}

pub fn capture(world: &mut World) -> ResourceSnapshot {
    let roots: HashSet<_> = world
        .query_filtered::<Entity, (
            With<crate::canvas::CanvasItem>,
            Without<crate::area::InfluenceArea>,
            Allow<Disabled>,
        )>()
        .iter(world)
        .filter(|entity| !crate::inspection::excluded(world, *entity))
        .collect();
    let bodies = crate::physics::resource_usage(world);
    let mut unique_images = HashMap::new();
    let mut sands = Vec::with_capacity(roots.len());
    for root in &roots {
        let kind = world
            .get::<crate::sand_store::StoredSand>(*root)
            .map(|sand| sand.kind.name())
            .unwrap_or("Sand");
        let name = world
            .get::<Name>(*root)
            .map(|name| name.as_str().to_string())
            .unwrap_or_else(|| kind.into());
        let parent = world.get::<ChildOf>(*root).map(ChildOf::parent);
        let member = world
            .get::<crate::workspace::WorkspaceMember>(*root)
            .map(|member| member.0);
        let workspace = parent
            .and_then(|parent| world.get::<crate::workspace::Workspaces>(parent))
            .and_then(|spaces| spaces.entries.iter().find(|space| Some(space.id) == member))
            .map(|space| space.name.clone())
            .unwrap_or_else(|| "Unassigned workspace".into());
        let mut row = SandResources {
            entity: root.to_string(),
            name,
            workspace,
            suspended: super::suspended(world, *root),
            ..default()
        };
        let mut pending = vec![*root];
        let mut images = HashSet::new();
        while let Some(entity) = pending.pop() {
            if entity != *root && roots.contains(&entity) {
                continue;
            }
            row.entities += 1;
            row.ui_nodes += usize::from(world.get::<Node>(entity).is_some());
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            if let Some(text) = world.get::<EditableText>(entity) {
                row.text_bytes += text.value().to_string().len();
            } else if let Some(text) = world.get::<Text>(entity) {
                row.text_bytes += text.0.len();
            } else if let Some(text) = world.get::<TextSpan>(entity) {
                row.text_bytes += text.0.len();
            }
            if let Some(text) = world.get::<TextLayoutInfo>(entity) {
                row.glyphs += text.glyphs.len();
            }
            if let Some(record) = world.get::<crate::area::RecordProperties>(entity) {
                row.record_json_bytes += record.0.to_string().len();
            }
            row.event_sources += usize::from(
                world.get::<crate::actions::ActionButton>(entity).is_some()
                    || world.get::<crate::effect::SendBoxEvent>(entity).is_some()
                    || world.get::<crate::effect::HoverEvents>(entity).is_some(),
            );
            row.pending_writes += usize::from(
                world
                    .get::<crate::record_view::RecordEditor>(entity)
                    .is_some_and(|editor| editor.pending.is_some()),
            );
            if let Some((body, awake)) = bodies.get(&entity) {
                row.physics_bodies += body;
                row.awake_bodies += awake;
            }
            if let Some(failure) = world.get::<StartupFailure>(entity) {
                row.startup.push(StartupIssue {
                    entity: entity.to_string(),
                    failed: true,
                    reason: failure.0.clone(),
                });
            } else if world.get::<crate::castle::Pending>(entity).is_some() {
                row.startup.push(StartupIssue {
                    entity: entity.to_string(),
                    failed: false,
                    reason: "Sand initialization is pending".into(),
                });
            }
            if let Some(image) = world.get::<ImageNode>(entity) {
                let asset = world
                    .get_resource::<Assets<Image>>()
                    .and_then(|assets| assets.get(&image.image));
                if let Some(issue) = asset_issue(
                    world,
                    entity,
                    image.image.id().untyped(),
                    "image",
                    asset.is_some() || crate::castle::image_prepared(world, image.image.id()),
                ) {
                    row.startup.push(issue);
                }
                if images.insert(image.image.id()) {
                    row.image_assets += 1;
                    let bytes = asset
                        .and_then(|image| image.data.as_ref())
                        .map_or(0, Vec::len);
                    row.retained_image_bytes += bytes;
                    unique_images.insert(image.image.id(), bytes);
                }
            }
            if let Some(font) = world.get::<TextFont>(entity)
                && let FontSource::Handle(font) = &font.font
            {
                let present = world
                    .get_resource::<Assets<Font>>()
                    .is_some_and(|fonts| fonts.contains(font.id()));
                if let Some(issue) =
                    asset_issue(world, entity, font.id().untyped(), "font", present)
                {
                    row.startup.push(issue);
                }
            }
        }
        let mut ancestor = Some(*root);
        while let Some(entity) = ancestor {
            row.suspended |= world.get::<Disabled>(entity).is_some();
            if let Some(status) = world.get::<crate::castle::StartupStatus>(entity) {
                row.startup.extend(status.0.iter().cloned());
            }
            ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        row.startup
            .sort_by(|a, b| a.entity.cmp(&b.entity).then(a.reason.cmp(&b.reason)));
        row.startup.dedup();
        sands.push(row);
    }
    sands.sort_by(|a, b| a.entity.cmp(&b.entity));
    ResourceSnapshot {
        graphics: GraphicsDevice::read(world),
        sands,
        unique_image_assets: unique_images.len(),
        retained_image_bytes: unique_images.values().sum(),
    }
}
