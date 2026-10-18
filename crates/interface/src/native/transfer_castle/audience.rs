use super::*;
use crate::native::sand_panel::{button, field, row};
use cell::{ClientMessage, ServerMessage};
use nucleus::visibility::{
    Context, Data, Policy,
    plan::{self, Definition, Review, Saved, Stage},
};

#[path = "audience_tests.rs"]
pub(super) mod tests;

#[derive(Clone, PartialEq, Eq)]
struct Scope {
    source: String,
    person: String,
    revision: u64,
}

fn delay_label(millis: u64) -> String {
    [
        (86_400_000, "d"),
        (3_600_000, "h"),
        (60_000, "m"),
        (1000, "s"),
        (1, "ms"),
    ]
    .into_iter()
    .find(|(unit, _)| millis % unit == 0)
    .map(|(unit, label)| format!("{}{label}", millis / unit))
    .unwrap_or_default()
}

fn delay_millis(text: &str) -> Result<u64, String> {
    [
        ("ms", 1),
        ("s", 1000),
        ("m", 60_000),
        ("h", 3_600_000),
        ("d", 86_400_000),
    ]
    .into_iter()
    .find_map(|(unit, multiplier)| {
        text.strip_suffix(unit)
            .and_then(|number| number.trim().parse::<u64>().ok())
            .and_then(|value| value.checked_mul(multiplier))
    })
    .filter(|value| *value > 0)
    .ok_or_else(|| "Use a positive delay such as 30s, 10m or 1h".into())
}

fn scope(world: &World, owner: Entity) -> Option<Scope> {
    let castle = world.get::<TransferCastle>(owner)?;
    let view = world.get::<View>(owner)?;
    let source = view
        .rows
        .iter()
        .find(|record| text(record, "uid") == castle.selected)?;
    let person = super::person(world, owner);
    (!person.is_empty()).then(|| Scope {
        source: castle.selected.clone(),
        person,
        revision: source["revision"].as_u64().unwrap_or(0),
    })
}

#[derive(Clone)]
enum Pending {
    Context,
    Inspect,
    Review(Definition),
    Mutation,
}

#[derive(Component)]
struct Panel {
    scope: Scope,
    definition: Definition,
    context: Option<Context>,
    plans: Vec<Saved>,
    review: Option<(Review, String)>,
    pending: HashMap<String, Pending>,
    fields: Vec<(String, Entity)>,
    stages: Vec<(Entity, Option<Entity>)>,
    exclusions: Option<Entity>,
    notice: String,
    next_refresh: std::time::Instant,
}

#[derive(Clone)]
enum Intent {
    Open,
    Add,
    Remove(usize),
    Direction(nucleus::social::Direction),
    Review,
    Enable,
    Load(Saved),
    Pause(Saved),
    Stop(Saved),
}

fn send(
    world: &World,
    owner: Entity,
    panel: &mut Panel,
    action: engine::actions::Action,
    pending: Pending,
) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Audience publication is read-only in Laboratory".into());
    }
    let id = nucleus::new_uid("audience-ui");
    crate::native::practice_cells::send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    panel.pending.insert(id, pending);
    Ok(())
}

fn collect(world: &mut World, panel: &mut Panel) -> Result<(), String> {
    let mut definition = panel.definition.clone();
    for (key, entity) in &panel.fields {
        let Some(input) = world.get::<bevy::text::EditableText>(*entity) else {
            continue;
        };
        let value = input.value().to_string().trim().to_owned();
        match key.as_str() {
            "title" => definition.projection.title = value,
            "text" => definition.projection.text = value,
            "quantity" => definition.projection.quantity = (!value.is_empty()).then_some(value),
            "unit" => definition.projection.unit = (!value.is_empty()).then_some(value),
            "concept" => definition.projection.concept = (!value.is_empty()).then_some(value),
            "area" => definition.projection.area = value,
            "availability" => definition.projection.availability = value,
            "expiry" => {
                definition.expires_at_ms = chrono::DateTime::parse_from_rfc3339(&value)
                    .map_err(|_| "Choose an expiry date/time with timezone")?
                    .timestamp_millis()
            }
            _ => {}
        }
    }
    for (index, (editor, delay)) in panel.stages.iter().enumerate() {
        if world.get_entity(*editor).is_err() {
            continue;
        }
        definition.stages[index].include =
            crate::visibility::edited_policy(world, *editor)?.include;
        if let Some(input) = delay.and_then(|entity| world.get::<bevy::text::EditableText>(entity))
        {
            definition.stages[index].after_ms = delay_millis(input.value().to_string().trim())?;
        }
    }
    if let Some(editor) = panel
        .exclusions
        .filter(|entity| world.get_entity(*entity).is_ok())
    {
        definition.exclude = crate::visibility::edited_policy(world, editor)?.exclude;
    }
    if definition != panel.definition {
        panel.review = None;
    }
    panel.definition = definition;
    Ok(())
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let current = scope(world, owner);
    let Some(mut panel) = world.entity_mut(owner).take::<Panel>() else {
        return;
    };
    if current.as_ref() != Some(&panel.scope) {
        return;
    }
    if let Err(error) = collect(world, &mut panel) {
        panel.review = None;
        panel.notice = error;
    }
    world.entity_mut(owner).insert(panel);
}

impl crate::native::actions::Action for Intent {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(current) = scope(world, owner) else {
            return;
        };
        capture(world, owner);
        if matches!(self, Self::Open) {
            let definition = Definition {
                uid: nucleus::new_uid("r"),
                projection_uid: nucleus::new_uid("r"),
                controller_uid: current.person.clone(),
                source_uid: current.source.clone(),
                source_revision: current.revision,
                projection: nucleus::social::PostDraft {
                    title: "Request".into(),
                    ..Default::default()
                },
                stages: vec![Stage {
                    after_ms: 0,
                    include: vec![],
                }],
                exclude: vec![],
                expires_at_ms: nucleus::execution::now()
                    .timestamp_millis()
                    .saturating_add(3_600_000),
            };
            let mut panel = Panel {
                scope: current,
                definition,
                context: None,
                plans: vec![],
                review: None,
                pending: HashMap::new(),
                fields: vec![],
                stages: vec![],
                exclusions: None,
                notice: "Choose public fields and audiences. Opening publishes nothing.".into(),
                next_refresh: std::time::Instant::now() + std::time::Duration::from_secs(5),
            };
            let source = panel.scope.source.clone();
            let person = panel.scope.person.clone();
            let result = send(
                world,
                owner,
                &mut panel,
                engine::actions::Action::Visibility {
                    request: nucleus::visibility::Command::Context {
                        record_uid: source.clone(),
                        data: Data::Record,
                    },
                },
                Pending::Context,
            )
            .and_then(|_| {
                send(
                    world,
                    owner,
                    &mut panel,
                    engine::actions::Action::Audience {
                        request: plan::Command::Inspect {
                            source_uid: source,
                            controller_uid: person,
                        },
                    },
                    Pending::Inspect,
                )
            });
            if let Err(error) = result {
                panel.notice = error;
            }
            world.entity_mut(owner).insert(panel);
        } else if let Some(mut panel) = world.entity_mut(owner).take::<Panel>() {
            let result = (|| -> Result<(), String> {
                collect(world, &mut panel)?;
                match self {
                    Self::Direction(direction) => {
                        panel.definition.projection.direction = *direction;
                        panel.review = None;
                    }
                    Self::Add if panel.definition.stages.len() < plan::MAX_STAGES => {
                        panel.definition.stages.push(Stage {
                            after_ms: 600_000,
                            include: vec![],
                        });
                        panel.review = None;
                    }
                    Self::Remove(index) if *index > 0 && *index < panel.definition.stages.len() => {
                        panel.definition.stages.remove(*index);
                        panel.stages.clear();
                        panel.review = None;
                    }
                    Self::Load(saved) => {
                        panel.definition = saved.review.definition.clone();
                        panel.definition.source_revision = panel.scope.revision;
                        panel.fields.clear();
                        panel.stages.clear();
                        panel.exclusions = None;
                        panel.review = None;
                        panel.notice = "Review current terms and replies before resuming".into();
                    }
                    Self::Review => {
                        if !panel.pending.is_empty() {
                            return Err("Wait for the pending audience request".into());
                        }
                        panel
                            .definition
                            .validate(nucleus::execution::now().timestamp_millis())?;
                        let definition = panel.definition.clone();
                        send(
                            world,
                            owner,
                            &mut panel,
                            engine::actions::Action::Audience {
                                request: plan::Command::Review {
                                    definition: definition.clone(),
                                },
                            },
                            Pending::Review(definition),
                        )?;
                    }
                    Self::Enable => {
                        if !panel.pending.is_empty() {
                            return Err("Wait for the pending audience request".into());
                        }
                        let (review, commitment) = panel
                            .review
                            .clone()
                            .filter(|(review, _)| review.definition == panel.definition)
                            .ok_or("Review the current fields, stages and exclusions first")?;
                        panel
                            .definition
                            .validate(nucleus::execution::now().timestamp_millis())?;
                        send(
                            world,
                            owner,
                            &mut panel,
                            engine::actions::Action::Audience {
                                request: plan::Command::Enable {
                                    review: Box::new(review),
                                    commitment,
                                },
                            },
                            Pending::Mutation,
                        )?;
                        panel.review = None;
                    }
                    Self::Pause(saved) | Self::Stop(saved) => {
                        let request = if matches!(self, Self::Stop(_)) {
                            plan::Command::Stop {
                                plan_uid: saved.review.definition.uid.clone(),
                                controller_uid: panel.scope.person.clone(),
                                expected_revision: saved.revision,
                            }
                        } else {
                            plan::Command::Pause {
                                plan_uid: saved.review.definition.uid.clone(),
                                controller_uid: panel.scope.person.clone(),
                                expected_revision: saved.revision,
                            }
                        };
                        send(
                            world,
                            owner,
                            &mut panel,
                            engine::actions::Action::Audience { request },
                            Pending::Mutation,
                        )?;
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                panel.review = None;
                panel.notice = error;
            }
            world.entity_mut(owner).insert(panel);
        }
        ui::render(world, owner);
    }
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, transfer: &Value) {
    if !capability(transfer, "edit_terms")
        && !array(transfer, "parties")
            .iter()
            .any(|party| text(party, "actor") == super::person(world, owner))
    {
        return;
    }
    button(world, parent, owner, "Audience expansion", Intent::Open);
    let Some(mut panel) = world.entity_mut(owner).take::<Panel>() else {
        return;
    };
    if scope(world, owner).as_ref() != Some(&panel.scope) {
        return;
    }
    panel.fields.clear();
    panel.stages.clear();
    panel.exclusions = None;
    let body = ui::stack(world, parent);
    crate::native::edit_mode::label(
        world,
        body,
        "Audience expansion · replies observed locally only; network coverage is incomplete",
        14.0,
    );
    crate::native::edit_mode::label(world, body, &panel.notice, 13.0);
    for saved in &panel.plans {
        crate::native::edit_mode::label(
            world,
            body,
            &format!(
                "{} · stage {} · {:?} · {}",
                saved.review.definition.projection.title,
                saved.stage.map_or(0, |stage| stage + 1),
                saved.status,
                saved.reason
            ),
            13.0,
        );
        let actions = row(world, body);
        button(
            world,
            actions,
            owner,
            "Review / resume this plan",
            Intent::Load(saved.clone()),
        );
        button(
            world,
            actions,
            owner,
            "Pause audience expansion",
            Intent::Pause(saved.clone()),
        );
        button(
            world,
            actions,
            owner,
            "Stop and withdraw publication",
            Intent::Stop(saved.clone()),
        );
    }
    if let Some(context) = &panel.context {
        let direction = row(world, body);
        button(
            world,
            direction,
            owner,
            "Public Need",
            Intent::Direction(nucleus::social::Direction::Need),
        );
        button(
            world,
            direction,
            owner,
            "Public Contribution",
            Intent::Direction(nucleus::social::Direction::Contribution),
        );
        let definition = &panel.definition;
        let expiry = chrono::DateTime::from_timestamp_millis(definition.expires_at_ms)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default();
        for (key, caption, value) in [
            (
                "title",
                "Public title",
                definition.projection.title.as_str(),
            ),
            (
                "text",
                "Public description",
                definition.projection.text.as_str(),
            ),
            (
                "quantity",
                "Public quantity (optional)",
                definition.projection.quantity.as_deref().unwrap_or(""),
            ),
            (
                "unit",
                "Public unit label (optional)",
                definition.projection.unit.as_deref().unwrap_or(""),
            ),
            (
                "concept",
                "Public activity label (optional)",
                definition.projection.concept.as_deref().unwrap_or(""),
            ),
            (
                "area",
                "Public approximate area label",
                definition.projection.area.as_str(),
            ),
            (
                "availability",
                "Public availability",
                definition.projection.availability.as_str(),
            ),
            (
                "expiry",
                "Publication expiry (date/time with timezone)",
                expiry.as_str(),
            ),
        ] {
            panel
                .fields
                .push((key.into(), field(world, body, caption, value)));
        }
        crate::native::edit_mode::label(
            world,
            body,
            "Common exclusions apply to every stage. Delivered copies cannot be recalled.",
            13.0,
        );
        panel.exclusions = Some(crate::visibility::mount_policy_draft(
            world,
            body,
            context.clone(),
            Policy {
                include: vec![],
                exclude: definition.exclude.clone(),
            },
            true,
        ));
        for (index, stage) in definition.stages.iter().enumerate() {
            crate::native::edit_mode::label(
                world,
                body,
                &format!(
                    "Stage {}{}",
                    index + 1,
                    if index == 0 {
                        " · on Enable"
                    } else {
                        " · after locally observed timeout"
                    }
                ),
                14.0,
            );
            let delay = (index > 0).then(|| {
                field(
                    world,
                    body,
                    "Delay since previous stage (for example 30s or 10m)",
                    &delay_label(stage.after_ms),
                )
            });
            let editor = crate::visibility::mount_policy_draft(
                world,
                body,
                context.clone(),
                Policy {
                    include: stage.include.clone(),
                    exclude: definition.exclude.clone(),
                },
                false,
            );
            panel.stages.push((editor, delay));
            if index > 0 {
                button(world, body, owner, "Remove stage", Intent::Remove(index));
            }
        }
        let actions = row(world, body);
        if definition.stages.len() < plan::MAX_STAGES {
            button(world, actions, owner, "Add audience stage", Intent::Add);
        }
        button(
            world,
            actions,
            owner,
            "Review publication and all stages",
            Intent::Review,
        );
        if panel.review.is_some() {
            button(
                world,
                actions,
                owner,
                "Enable reviewed stages",
                Intent::Enable,
            );
        }
    }
    world.entity_mut(owner).insert(panel);
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let (id, data, error) = match message {
        ServerMessage::ActionOk { id, data, .. } => (id, data.as_ref(), None),
        ServerMessage::Error { id, message, .. } => (id, None, Some(message.as_str())),
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| panel.pending.contains_key(id))
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    capture(world, owner);
    let Some(mut panel) = world.entity_mut(owner).take::<Panel>() else {
        ui::render(world, owner);
        return true;
    };
    let Some(pending) = panel.pending.remove(id) else {
        world.entity_mut(owner).insert(panel);
        return true;
    };
    if let Some(error) = error {
        panel.review = None;
        panel.notice = error.into();
    } else if let Some(data) = data {
        let result = (|| -> Result<(), String> {
            match pending {
                Pending::Context => {
                    panel.context = Some(
                        serde_json::from_value(data.clone()).map_err(|error| error.to_string())?,
                    )
                }
                Pending::Inspect => {
                    let plans: Vec<Saved> = serde_json::from_value(data["plans"].clone())
                        .map_err(|error| error.to_string())?;
                    if plans.iter().any(|saved| {
                        saved.review.definition.controller_uid != panel.scope.person
                            || saved.review.definition.source_uid != panel.scope.source
                    }) {
                        return Err("Audience status belongs to another viewer or Transfer".into());
                    }
                    if panel.review.as_ref().is_some_and(|(review, _)| {
                        review.expected_revision
                            != plans
                                .iter()
                                .find(|saved| saved.review.definition.uid == review.definition.uid)
                                .map_or(0, |saved| saved.revision)
                    }) {
                        panel.review = None;
                    }
                    panel.plans = plans;
                }
                Pending::Review(definition) => {
                    let review: Review = serde_json::from_value(data["review"].clone())
                        .map_err(|error| error.to_string())?;
                    let commitment = data["commitment"]
                        .as_str()
                        .ok_or("Audience review is missing its commitment")?
                        .to_owned();
                    if definition != panel.definition
                        || review.definition != definition
                        || review.commitment()? != commitment
                    {
                        return Err(
                            "The draft changed; review the current fields and stages".into()
                        );
                    }
                    panel.review = Some((review, commitment));
                    panel.notice="Public fields, every audience, exclusions, delays and expiry reviewed. Enable to publish.".into();
                }
                Pending::Mutation => {
                    let saved: Saved = serde_json::from_value(data["plan"].clone())
                        .map_err(|error| error.to_string())?;
                    if saved.review.definition.controller_uid != panel.scope.person
                        || saved.review.definition.source_uid != panel.scope.source
                    {
                        return Err(
                            "Audience response belongs to another viewer or Transfer".into()
                        );
                    }
                    panel
                        .plans
                        .retain(|old| old.review.definition.uid != saved.review.definition.uid);
                    panel.plans.push(saved);
                    panel.review = None;
                    panel.notice="Audience plan saved. Replies observed locally only; network coverage is incomplete.".into();
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            panel.review = None;
            panel.notice = error;
        }
    }
    world.entity_mut(owner).insert(panel);
    ui::render(world, owner);
    true
}

pub(super) fn maintain(world: &mut World, owner: Entity) {
    capture(world, owner);
    if crate::native::laboratory::blocked(world, owner) {
        return;
    }
    let Some(mut panel) = world.entity_mut(owner).take::<Panel>() else {
        return;
    };
    if panel.pending.is_empty() && std::time::Instant::now() >= panel.next_refresh {
        let request = plan::Command::Inspect {
            source_uid: panel.scope.source.clone(),
            controller_uid: panel.scope.person.clone(),
        };
        if let Err(error) = send(
            world,
            owner,
            &mut panel,
            engine::actions::Action::Audience { request },
            Pending::Inspect,
        ) {
            panel.notice = error;
        }
        panel.next_refresh = std::time::Instant::now() + std::time::Duration::from_secs(5);
    }
    world.entity_mut(owner).insert(panel);
}
