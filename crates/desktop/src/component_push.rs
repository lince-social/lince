use bevy::{math::DVec2, prelude::*};
use cell::ServerMessage;
use nucleus::component::{ComponentState, Presentation, RecordMode};
use serde::{Deserialize, Serialize};

pub(crate) mod composition;

use crate::{
    area::{AreaShape, InfluenceArea},
    canvas::{CanvasItem, CanvasView},
    cell_bridge::{CellMessage, ReceiveCell},
    container::BoxRoot,
    workspace::{WorkspaceMember, Workspaces},
};

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Placed {
    pub area: String,
    pub slot: String,
    pub component: ComponentState,
}

impl Placed {
    pub(crate) fn valid(&self) -> bool {
        self.area.len() == 32
            && self.area.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !self.slot.is_empty()
            && self.slot.len() <= 300
            && self.component.validate().is_ok()
    }
}

pub struct ComponentPushPlugin;

impl Plugin for ComponentPushPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CellMessage>()
            .add_systems(Update, receive.after(ReceiveCell));
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let presentations: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in presentations {
        composition::receive(world, &message);
        crate::canvas_host::composition::receive(world, &message);
        if let ServerMessage::PresentComponent { presentation } = message {
            if let Err(message) = present(world, presentation) {
                crate::notifications::report(world, "interface::components", &message);
            }
        }
    }
}

pub(crate) fn present(world: &mut World, presentation: Presentation) -> Result<Entity, String> {
    presentation.component.validate()?;
    if presentation.slot.is_empty() || presentation.slot.len() > 300 {
        return Err("Invalid component slot".into());
    }
    let (root, active) = world
        .query_filtered::<(Entity, &Workspaces), With<BoxRoot>>()
        .iter(world)
        .next()
        .map(|(root, spaces)| (root, spaces.active))
        .ok_or("No workspace is open")?;
    let area = world
        .query::<(Entity, &InfluenceArea, &ChildOf)>()
        .iter(world)
        .find(|(_, area, parent)| area.backend_components && parent.parent() == root)
        .map(|(entity, _, _)| entity);
    let area = match area {
        Some(area) => area,
        None => {
            let center = world
                .get::<CanvasView>(root)
                .map_or(DVec2::ZERO, |view| view.center);
            let mut area = InfluenceArea::new(rectangle(), center, DVec2::new(2200.0, 1600.0));
            area.name = "Backend components".into();
            area.backend_components = true;
            area.attraction_enabled = false;
            area.changes_enabled = false;
            crate::area::spawn_area(world, root, active, area)
                .ok_or("Could not create the component area")?
        }
    };
    let area_id = world.get::<InfluenceArea>(area).unwrap().id.clone();
    let workspace = world.get::<WorkspaceMember>(area).unwrap().0;
    let center = world.get::<CanvasItem>(area).unwrap().position;
    let existing = world
        .query::<(Entity, &Placed, &CanvasItem)>()
        .iter(world)
        .find(|(_, placed, _)| placed.area == area_id && placed.slot == presentation.slot)
        .map(|(entity, placed, item)| (entity, placed.component.clone(), item.position));
    if let Some((entity, component, _)) = &existing
        && *component == presentation.component
    {
        start_calls(world, *entity, &presentation.component)?;
        return Ok(*entity);
    }
    let count = world
        .query::<&Placed>()
        .iter(world)
        .filter(|placed| placed.area == area_id)
        .count();
    let position = existing.as_ref().map_or(
        center + DVec2::new((count % 2) as f64 * 1040.0, (count / 2) as f64 * 740.0),
        |(_, _, position)| *position,
    );
    let entity = spawn(world, root, workspace, position, &presentation.component)?;
    let size = world.get::<CanvasItem>(entity).unwrap().size.as_dvec2();
    let extent = ((position - center).abs() + size * 0.5 + DVec2::splat(40.0)) * 2.0;
    let size = DVec2::from_array(world.get::<InfluenceArea>(area).unwrap().size).max(extent);
    world.get_mut::<InfluenceArea>(area).unwrap().size = size.to_array();
    world.get_mut::<CanvasItem>(area).unwrap().size = size.as_vec2();
    if let Some((old, _, _)) = existing {
        world.despawn(old);
    }
    world.entity_mut(entity).insert(Placed {
        area: area_id,
        slot: presentation.slot,
        component: presentation.component.clone(),
    });
    start_calls(world, entity, &presentation.component)?;
    Ok(entity)
}

fn start_calls(
    world: &mut World,
    entity: Entity,
    component: &ComponentState,
) -> Result<(), String> {
    match component {
        ComponentState::Record {
            record,
            start_call: Some(start),
            ..
        } => {
            #[cfg(feature = "native-media")]
            {
                crate::communication::calls::start_automatically(
                    world,
                    crate::protein_area::RecordBinding {
                        area: entity,
                        uid: record.clone(),
                        source: crate::protein_area::Source::Local,
                    },
                    start,
                )
            }
            #[cfg(not(feature = "native-media"))]
            {
                let _ = (world, entity, record, start);
                Err("Automatic calls require native media support".into())
            }
        }
        ComponentState::Composition { composition } => {
            let entities = composition::parts(world, entity);
            for (part, entity) in composition.parts.iter().zip(entities) {
                start_calls(world, entity, &part.component)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    component: &ComponentState,
) -> Result<Entity, String> {
    Ok(match component {
        ComponentState::Record { record, mode, .. } => {
            let mut config = crate::full_record::config(record, crate::protein_area::Source::Local);
            match mode {
                RecordMode::Full => {}
                RecordMode::Description => config
                    .bindings
                    .retain(|binding| matches!(binding.property.as_str(), "head" | "body")),
                RecordMode::Call => {
                    config.bindings = vec![
                        crate::protein_area::Binding::new("head"),
                        crate::protein_area::Binding::new("threads"),
                    ]
                }
            }
            let mut area = InfluenceArea::new(rectangle(), position, DVec2::new(520.0, 640.0));
            area.name = match mode {
                RecordMode::Call => "Call Castle",
                _ => "Record Castle",
            }
            .into();
            area.protein = Some(config);
            crate::area::spawn_area(world, root, workspace, area)
                .ok_or("Could not show the Record Castle")?
        }
        ComponentState::Text { text } => crate::sand_store::spawn_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Text,
            text,
            position,
        ),
        ComponentState::Karma { search } => crate::karma_castle::spawn(
            world,
            root,
            workspace,
            position,
            crate::karma_castle::KarmaCastle {
                search: search.clone(),
                ..Default::default()
            },
        ),
        ComponentState::Frequency { search } => crate::frequency_castle::spawn(
            world,
            root,
            workspace,
            position,
            crate::frequency_castle::FrequencyCastle {
                search: search.clone(),
                ..Default::default()
            },
        ),
        ComponentState::Transfer { search } => crate::transfer_castle::spawn(
            world,
            root,
            workspace,
            position,
            crate::transfer_castle::TransferCastle {
                search: search.clone(),
                ..Default::default()
            },
        ),
        ComponentState::Calendar => {
            crate::calendar::spawn(world, root, workspace, position, Default::default())
        }
        ComponentState::Area { immunity, strength } => {
            let mut area = InfluenceArea::new(rectangle(), position, DVec2::new(640.0, 480.0));
            area.immunity = match immunity {
                nucleus::component::Immunity::None => crate::area_effects::Immunity::None,
                nucleus::component::Immunity::External => crate::area_effects::Immunity::External,
                nucleus::component::Immunity::Internal => crate::area_effects::Immunity::Internal,
                nucleus::component::Immunity::All => crate::area_effects::Immunity::All,
                nucleus::component::Immunity::Containment => {
                    crate::area_effects::Immunity::Containment
                }
                nucleus::component::Immunity::Isolation => crate::area_effects::Immunity::Isolation,
            };
            area.strength = f64::from(*strength);
            crate::area::spawn_area(world, root, workspace, area)
                .ok_or("Could not create the composition area")?
        }
        ComponentState::Button { label, action } => {
            let action: engine::actions::Action =
                serde_json::from_value(action.clone()).map_err(|e| e.to_string())?;
            let entity = crate::sand_store::spawn_sand(
                world,
                root,
                workspace,
                crate::sand_store::SandKind::Square,
                "",
                position,
            );
            crate::sand_panel::button(world, entity, entity, label, composition::Invoke(action));
            entity
        }
        ComponentState::Composition { composition } => {
            composition::spawn(world, root, workspace, position, composition.clone())?
        }
    })
}

pub(crate) fn rectangle() -> AreaShape {
    AreaShape::Polygon(vec![
        [-0.5, -0.5],
        [0.5, -0.5],
        [0.5, 0.5],
        [-0.5, 0.5],
        [-0.5, -0.5],
    ])
}

#[cfg(test)]
mod tests;
