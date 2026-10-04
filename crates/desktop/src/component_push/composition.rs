use super::*;
use nucleus::component::{Composition, Document};

#[derive(Component)]
pub(crate) struct Generated {
    pub composition: Composition,
    parts: Vec<Entity>,
    status: Entity,
    pending: Option<String>,
}

#[derive(Component)]
pub(crate) struct GeneratedCanvas;

#[derive(Resource, Default)]
struct PendingInteractions(std::collections::BTreeMap<String, Entity>);

#[derive(Component)]
struct Events(Vec<(String, engine::actions::Action)>);

pub(crate) fn bind_events(world: &mut World, entity: Entity, bindings: &[nucleus::component::EventBinding]) -> Result<(), String> {
    let actions = bindings.iter().map(|binding| Ok((binding.event.clone(), serde_json::from_value::<engine::actions::Action>(binding.action.clone()).map_err(|error| error.to_string())?))).collect::<Result<Vec<_>, String>>()?;
    if !actions.is_empty() { world.entity_mut(entity).insert((crate::scoped_events::EventListener(actions.iter().map(|(name, _)| name.clone()).collect()), Events(actions))).observe(event); }
    Ok(())
}

pub(super) fn parts(world: &World, entity: Entity) -> Vec<Entity> {
    world
        .get::<Generated>(entity)
        .map_or_else(Vec::new, |generated| generated.parts.clone())
}

fn event(
    event: On<crate::scoped_events::SandEvent>,
    bindings: Query<&Events>,
    mut commands: Commands,
) {
    let Ok(bindings) = bindings.get(event.entity) else {
        return;
    };
    let target = event.entity;
    let actions: Vec<_> = bindings
        .0
        .iter()
        .filter(|(name, _)| *name == event.name)
        .map(|(_, action)| action.clone())
        .collect();
    commands.queue(move |world: &mut World| {
        for action in actions {
            crate::actions::Action::apply(&Invoke(action), world, target);
        }
    });
}

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    composition: Composition,
) -> Result<Entity, String> {
    composition.validate()?;
    let mut area = InfluenceArea::new(rectangle(), position, DVec2::new(1000.0, 900.0));
    area.name = composition.name.clone();
    area.immunity = crate::area_effects::Immunity::Isolation;
    area.attraction_enabled = false;
    area.changes_enabled = false;
    let host = crate::area::spawn_area(world, root, workspace, area)
        .ok_or("No room for another Fiote balloon")?;
    if let Err(error) = populate(world, host, composition) {
        world.despawn(host);
        return Err(error);
    }
    Ok(host)
}

pub(crate) fn populate(
    world: &mut World,
    host: Entity,
    composition: Composition,
) -> Result<(), String> {
    composition.validate()?;
    if world.get::<Generated>(host).is_some() {
        return Ok(());
    }
    validate_settings(&composition)?;
    let state = ComponentState::Composition {
        composition: composition.clone(),
    };
    state.visit(&mut |component| {
        if let ComponentState::Button { action, .. } = component {
            serde_json::from_value::<engine::actions::Action>(action.clone())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    state.visit(&mut |component| {
        if let ComponentState::Composition { composition } = component {
            for binding in composition.parts.iter().flat_map(|part| &part.events) {
                serde_json::from_value::<engine::actions::Action>(binding.action.clone())
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })?;
    populate_validated(world, host, composition)
}

fn validate_settings(composition: &Composition) -> Result<(), String> {
    for part in &composition.parts {
        if let ComponentState::Composition { composition } = &part.component {
            validate_settings(composition)?;
        }
        if part.settings.is_empty() {
            continue;
        }
        let settings: crate::sand_settings::Values = serde_json::from_value(
            serde_json::to_value(&part.settings).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if !matches!(part.component, ComponentState::Text { .. })
            || !settings.valid(&crate::sand_settings::text_definitions())
        {
            return Err(format!("Unsupported settings in {}", part.id));
        }
    }
    Ok(())
}

fn populate_validated(
    world: &mut World,
    host: Entity,
    composition: Composition,
) -> Result<(), String> {
    let mut minimum = DVec2::splat(f64::INFINITY);
    let mut maximum = DVec2::splat(f64::NEG_INFINITY);
    for part in &composition.parts {
        let position = DVec2::new(f64::from(part.position[0]), f64::from(part.position[1]));
        let size = DVec2::new(f64::from(part.size[0]), f64::from(part.size[1]));
        minimum = minimum.min(position - size * 0.5);
        maximum = maximum.max(position + size * 0.5);
    }
    let size = (maximum - minimum).max(DVec2::new(480.0, 320.0));
    world.entity_mut(host).insert(Node {
        width: px((size.x + 120.0) as f32),
        height: px((size.y + 210.0) as f32),
        ..Default::default()
    });
    let fiote = crate::fiote::spawn(world, host);
    world.get_mut::<Node>(fiote).unwrap().position_type = PositionType::Relative;
    world.get_mut::<Node>(fiote).unwrap().right = Val::Auto;
    world.get_mut::<Node>(fiote).unwrap().bottom = Val::Auto;
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
            OpenOrigin(origin.thread.clone()),
        );
    }
    let status = crate::edit_mode::label(
        world,
        bubble,
        "Changes use ordinary Actions. Canvas influence is isolated.",
        12.0,
    );
    crate::accessibility::status(world, status);
    let canvas = world
        .spawn((
            Node {
                width: px(size.x as f32),
                height: px(size.y as f32),
                overflow: Overflow::clip(),
                ..Default::default()
            },
            CanvasView {
                center: (minimum + maximum) * 0.5,
                zoom: 1.0,
            },
            Workspaces::default(),
            GeneratedCanvas,
            crate::scoped_events::IsolatedEvents,
            ChildOf(bubble),
        ))
        .id();
    let mut parts = Vec::new();
    for part in &composition.parts {
        let position = DVec2::new(f64::from(part.position[0]), f64::from(part.position[1]));
        match super::spawn(world, canvas, 1, position, &part.component) {
            Ok(entity) => {
                if let Some(text) = world
                    .get::<Children>(entity)
                    .into_iter()
                    .flatten()
                    .copied()
                    .find(|child| world.get::<crate::sand_text::SandText>(*child).is_some())
                {
                    for (id, value) in &part.settings {
                        let value = serde_json::from_value(value.clone())
                            .map_err(|error| error.to_string())?;
                        crate::sand_settings::set(world, text, id, Some(value));
                    }
                }
                let size = Vec2::new(part.size[0] as f32, part.size[1] as f32);
                world.get_mut::<CanvasItem>(entity).unwrap().size = size;
                if let Some(mut area) = world.get_mut::<InfluenceArea>(entity) {
                    area.size = size.as_dvec2().to_array();
                }
                parts.push(entity);
                if !part.events.is_empty() {
                    let actions = part
                        .events
                        .iter()
                        .map(|binding| {
                            Ok((
                                binding.event.clone(),
                                serde_json::from_value::<engine::actions::Action>(
                                    binding.action.clone(),
                                )
                                .map_err(|e| e.to_string())?,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    world
                        .entity_mut(entity)
                        .insert((
                            crate::scoped_events::EventListener(
                                actions.iter().map(|(name, _)| name.clone()).collect(),
                            ),
                            Events(actions),
                        ))
                        .observe(event);
                }
            }
            Err(error) => {
                world.despawn(fiote);
                return Err(error);
            }
        }
    }
    if let Some(mut item) = world.get_mut::<CanvasItem>(host) {
        item.size = (size + DVec2::new(120.0, 210.0)).as_vec2();
    }
    if let Some(mut area) = world.get_mut::<InfluenceArea>(host) {
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

#[derive(Clone)]
pub(super) struct Invoke(pub engine::actions::Action);
impl crate::actions::Action for Invoke {
    fn apply(&self, world: &mut World, owner: Entity) {
        let mut ancestor = owner;
        let mut status = None;
        for _ in 0..256 {
            if let Some(generated) = world.get::<Generated>(ancestor) {
                status = Some(generated.status);
                break;
            }
            if let Some(generated) = world.get::<crate::canvas_host::composition::Generated>(ancestor) { status = Some(generated.status); break; }
            let Some(parent) = world.get::<ChildOf>(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
        world.init_resource::<PendingInteractions>();
        let closed: Vec<_> = world
            .resource::<PendingInteractions>()
            .0
            .iter()
            .filter(|(_, entity)| world.get_entity(**entity).is_err())
            .map(|(id, _)| id.clone())
            .collect();
        for id in closed {
            world.resource_mut::<PendingInteractions>().0.remove(&id);
        }
        if world.resource::<PendingInteractions>().0.len() >= 128 {
            if let Some(status) = status {
                world.get_mut::<Text>(status).unwrap().0 =
                    "Wait for pending interactions to finish".into();
            }
            return;
        }
        let id = nucleus::new_uid("composition-action");
        match crate::sand_panel::send(
            world,
            cell::ClientMessage::Act {
                id: id.clone(),
                action: self.0.clone(),
            },
        ) {
            Ok(()) => {
                if let Some(status) = status {
                    world
                        .resource_mut::<PendingInteractions>()
                        .0
                        .insert(id, status);
                    world.get_mut::<Text>(status).unwrap().0 = "Applying…".into();
                }
            }
            Err(message) => {
                if let Some(status) = status {
                    world.get_mut::<Text>(status).unwrap().0 = message;
                } else {
                    crate::notifications::report(world, "interface::components", &message);
                }
            }
        }
    }
}

#[derive(Clone)]
struct Save(Entity);
impl crate::actions::Action for Save {
    fn apply(&self, world: &mut World, owner: Entity) {
        let result = (|| {
            let generated = world
                .get::<Generated>(owner)
                .ok_or("This composition is closed")?;
            if generated.pending.is_some() {
                return Err("A save is already pending".to_string());
            }
            let mut composition = capture(world, owner)
                .ok_or("A component was removed; close and regenerate this composition")?;
            composition.name = crate::sand_panel::value(world, self.0)?.trim().into();
            let body = Document::encode(composition.clone())?;
            let id = nucleus::new_uid("composition-save");
            crate::sand_panel::send(
                world,
                cell::ClientMessage::Act {
                    id: id.clone(),
                    action: engine::actions::Action::CreateCustomComponent {
                        head: composition.name,
                        body,
                    },
                },
            )?;
            world.get_mut::<Generated>(owner).unwrap().pending = Some(id);
            Ok::<_, String>("Saving component…".into())
        })();
        let status = world
            .get::<Generated>(owner)
            .map(|generated| generated.status);
        if let Some(status) = status {
            if let Some(mut text) = world.get_mut::<Text>(status) {
                text.0 = result.unwrap_or_else(|e| e);
            }
        }
    }
}

#[derive(Clone)]
struct Close;
impl crate::actions::Action for Close {
    fn apply(&self, world: &mut World, owner: Entity) {
        world.despawn(owner);
    }
}

#[derive(Clone)]
pub(crate) struct OpenOrigin(pub String);

impl crate::actions::Action for OpenOrigin {
    fn apply(&self, world: &mut World, _owner: Entity) {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .iter(world)
            .next();
        if let Some(root) = root {
            crate::thread_castle::open(world, root, &self.0, crate::protein_area::Source::Local);
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) {
    let interaction = match message {
        ServerMessage::ActionOk {
            id, data, warnings, ..
        } => Some((
            id,
            if warnings.is_empty() {
                data.as_ref()
                    .map_or_else(|| "Applied".into(), |data| format!("Applied: {data}"))
            } else {
                format!(
                    "{}{}",
                    data.as_ref()
                        .map_or_else(|| "Accepted. ".into(), |data| format!("{data}. ")),
                    warnings.join("; ")
                )
            },
        )),
        ServerMessage::Error { id, message, .. } => Some((id, format!("Refused: {message}"))),
        _ => None,
    };
    if let Some((id, value)) = interaction {
        if let Some(status) = world
            .get_resource_mut::<PendingInteractions>()
            .and_then(|mut pending| pending.0.remove(id))
        {
            if let Some(mut text) = world.get_mut::<Text>(status) {
                text.0 = value;
            }
        }
    }
    let result = match message {
        ServerMessage::ActionOk { id, .. } => {
            Some((id, "Saved to the component library".to_string()))
        }
        ServerMessage::Error { id, message, .. } => Some((id, message.clone())),
        _ => None,
    };
    if let Some((id, value)) = result {
        let targets: Vec<_> = world
            .query::<(Entity, &Generated)>()
            .iter(world)
            .filter(|(_, generated)| generated.pending.as_ref() == Some(id))
            .map(|(entity, generated)| (entity, generated.status))
            .collect();
        for (entity, status) in targets {
            world.get_mut::<Generated>(entity).unwrap().pending = None;
            if let Some(mut text) = world.get_mut::<Text>(status) {
                text.0 = value.clone();
            }
        }
    }
}

pub(crate) fn capture(world: &World, entity: Entity) -> Option<Composition> {
    let generated = world.get::<Generated>(entity)?;
    let mut composition = generated.composition.clone();
    for (part, entity) in composition.parts.iter_mut().zip(&generated.parts) {
        let item = world.get::<CanvasItem>(*entity)?;
        part.position = [
            item.position.x.round() as i32,
            item.position.y.round() as i32,
        ];
        part.size = [item.size.x.round() as u32, item.size.y.round() as u32];
        match &mut part.component {
            ComponentState::Text { text } => {
                let texts = crate::sand_text::snapshot(world, *entity);
                let saved = texts.first()?;
                *text = saved.text.clone();
                part.settings =
                    serde_json::from_value(serde_json::to_value(&saved.area.settings.0).ok()?)
                        .ok()?;
            }
            ComponentState::Karma { search } => {
                *search = world
                    .get::<crate::karma_castle::KarmaCastle>(*entity)?
                    .search
                    .clone()
            }
            ComponentState::Frequency { search } => {
                *search = world
                    .get::<crate::frequency_castle::FrequencyCastle>(*entity)?
                    .search
                    .clone()
            }
            ComponentState::Transfer { search } => {
                *search = world
                    .get::<crate::transfer_castle::TransferCastle>(*entity)?
                    .search
                    .clone()
            }
            ComponentState::Composition { composition } => *composition = capture(world, *entity)?,
            ComponentState::Area { immunity, strength } => {
                let area = world.get::<InfluenceArea>(*entity)?;
                *strength = area.strength.round() as i32;
                *immunity = match area.immunity {
                    crate::area_effects::Immunity::None => nucleus::component::Immunity::None,
                    crate::area_effects::Immunity::External => {
                        nucleus::component::Immunity::External
                    }
                    crate::area_effects::Immunity::Internal => {
                        nucleus::component::Immunity::Internal
                    }
                    crate::area_effects::Immunity::All => nucleus::component::Immunity::All,
                    crate::area_effects::Immunity::Containment => {
                        nucleus::component::Immunity::Containment
                    }
                    crate::area_effects::Immunity::Isolation => {
                        nucleus::component::Immunity::Isolation
                    }
                };
            }
            _ => {}
        }
    }
    Some(composition)
}
