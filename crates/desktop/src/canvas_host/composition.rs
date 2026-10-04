use super::*;
use crate::actions::Action;

#[derive(Component)]
pub(crate) struct Generated {
    pub composition: api::Composition,
    parts: Vec<Entity>,
    pub(crate) status: Entity,
    pending: Option<String>,
}

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    composition: api::Composition,
) -> Result<Entity, String> {
    let mut area = crate::area::InfluenceArea::new(
        crate::component_push::rectangle(),
        position,
        DVec2::new(1000.0, 900.0),
    );
    area.name = composition.name.clone();
    area.immunity = crate::area_effects::Immunity::Isolation;
    area.attraction_enabled = false;
    area.changes_enabled = false;
    let host = crate::area::spawn_area(world, root, workspace, area)
        .ok_or("No room for another Fiote balloon.")?;
    if let Err(error) = populate(world, host, composition) {
        world.despawn(host);
        return Err(error);
    }
    Ok(host)
}

pub(crate) fn populate(
    world: &mut World,
    host: Entity,
    composition: api::Composition,
) -> Result<(), String> {
    if world.get::<Generated>(host).is_some() {
        return Ok(());
    }
    ContentKind::Composition {
        composition: composition.clone(),
    }
    .validate(&registry(), false)?;
    let mut minimum = DVec2::splat(f64::INFINITY);
    let mut maximum = DVec2::splat(f64::NEG_INFINITY);
    for part in &composition.parts {
        let position = DVec2::from_array(part.geometry.position);
        let size = DVec2::from_array(part.geometry.size);
        minimum = minimum.min(position - size * 0.5);
        maximum = maximum.max(position + size * 0.5);
    }
    let size = (maximum - minimum).max(DVec2::new(480.0, 320.0));
    let fiote = crate::fiote::spawn(world, host);
    let mut node = world.get_mut::<Node>(fiote).unwrap();
    node.position_type = PositionType::Relative;
    node.right = Val::Auto;
    node.bottom = Val::Auto;
    let bubble = world.get::<crate::fiote::Fiote>(fiote).unwrap().bubble;
    crate::edit_mode::label(world, bubble, &composition.name, 18.0);
    let name = crate::sand_panel::field(world, bubble, "Component name", &composition.name);
    let controls = crate::sand_panel::row(world, bubble);
    crate::sand_panel::button(world, controls, host, "Save component", Save(name));
    crate::sand_panel::button(world, controls, host, "Close", Close);
    if let Some(origin) = &composition.origin {
        crate::sand_panel::button(
            world,
            controls,
            host,
            "Open originating conversation",
            crate::component_push::composition::OpenOrigin(origin.thread.clone()),
        );
    }
    let status = crate::edit_mode::label(
        world,
        bubble,
        "Canvas interactions are isolated. Authorized Actions use shared data.",
        12.0,
    );
    crate::accessibility::status(world, status);
    let canvas = world
        .spawn((
            Node {
                width: px(size.x as f32),
                height: px(size.y as f32),
                overflow: Overflow::clip(),
                ..default()
            },
            CanvasView {
                center: (minimum + maximum) * 0.5,
                zoom: 1.0,
            },
            Workspaces::default(),
            crate::component_push::composition::GeneratedCanvas,
            crate::scoped_events::IsolatedEvents,
            ChildOf(bubble),
        ))
        .id();
    let mut parts = Vec::new();
    for part in &composition.parts {
        let entity = match super::spawn(
            world,
            canvas,
            1,
            DVec2::from_array(part.geometry.position),
            &part.component,
        ) {
            Ok(entity) => entity,
            Err(error) => {
                world.despawn(fiote);
                return Err(error);
            }
        };
        let size = DVec2::from_array(part.geometry.size);
        world.get_mut::<CanvasItem>(entity).unwrap().size = size.as_vec2();
        if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(entity) {
            area.size = size.to_array();
        }
        crate::component_push::composition::bind_events(world, entity, &part.events)?;
        parts.push(entity);
    }
    world.get_mut::<CanvasItem>(host).unwrap().size = (size + DVec2::new(120.0, 210.0)).as_vec2();
    if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(host) {
        area.immunity = crate::area_effects::Immunity::Isolation;
        area.attraction_enabled = false;
        area.changes_enabled = false;
        area.size = (size + DVec2::new(120.0, 210.0)).to_array();
    }
    world.entity_mut(host).insert(Generated {
        composition,
        parts,
        status,
        pending: None,
    });
    Ok(())
}

pub(crate) fn capture(world: &World, host: Entity) -> Option<api::Composition> {
    let generated = world.get::<Generated>(host)?;
    let mut composition = generated.composition.clone();
    for (part, entity) in composition.parts.iter_mut().zip(&generated.parts) {
        let item = world.get::<CanvasItem>(*entity)?;
        part.geometry = api::Geometry {
            position: item.position.to_array(),
            size: item.size.as_dvec2().to_array(),
        };
        part.component = super::capture_content(world, *entity);
    }
    Some(composition)
}

#[derive(Clone)]
struct Save(Entity);
impl Action for Save {
    fn apply(&self, world: &mut World, host: Entity) {
        let result = (|| {
            let generated = world
                .get::<Generated>(host)
                .ok_or("The composition is closed.")?;
            if generated.pending.is_some() {
                return Err("Wait for the current save.".into());
            }
            let mut composition = capture(world, host).ok_or("A composition part was removed.")?;
            composition.name = crate::sand_panel::value(world, self.0)?.trim().into();
            let head = composition.name.clone();
            let body =
                api::Document::encode(head.clone(), ContentKind::Composition { composition })?;
            let id = nucleus::new_uid("canvas-component-save");
            crate::sand_panel::send(
                world,
                cell::ClientMessage::Act {
                    id: id.clone(),
                    action: engine::actions::Action::CreateCustomComponent { head, body },
                },
            )?;
            world.get_mut::<Generated>(host).unwrap().pending = Some(id);
            Ok::<_, String>("Saving component…".into())
        })();
        if let Some(status) = world
            .get::<Generated>(host)
            .map(|generated| generated.status)
        {
            crate::sand_panel::status(world, status, result.unwrap_or_else(|error| error));
        }
    }
}
#[derive(Clone)]
struct Close;
impl Action for Close {
    fn apply(&self, world: &mut World, host: Entity) {
        world.despawn(host);
    }
}

pub(crate) fn receive(world: &mut World, message: &cell::ServerMessage) {
    let (id, result) = match message {
        cell::ServerMessage::ActionOk { id, created, .. } => (
            id,
            format!("Saved component {}", created.as_deref().unwrap_or_default()),
        ),
        cell::ServerMessage::Error { id, message, .. } => (id, message.clone()),
        _ => return,
    };
    let hosts: Vec<_> = world
        .query::<(Entity, &Generated)>()
        .iter(world)
        .filter(|(_, generated)| generated.pending.as_deref() == Some(id))
        .map(|(entity, _)| entity)
        .collect();
    for host in hosts {
        let mut generated = world.get_mut::<Generated>(host).unwrap();
        generated.pending = None;
        let status = generated.status;
        crate::sand_panel::status(world, status, &result);
    }
}
