use crate::{controls, theme::Typography};
use bevy::prelude::*;
use nucleus::visibility::{Bound, Command, Condition, Context, Data, Policy};

#[derive(Message, Clone)]
pub struct Request {
    pub id: String,
    pub command: Command,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    pub record_uid: String,
    pub data: Data,
}

impl Selection {
    pub fn valid(&self) -> bool {
        self.record_uid.is_empty() || nucleus::valid_uid(&self.record_uid, "r")
    }
}

pub fn selection(world: &World, entity: Entity) -> Option<Selection> {
    world.get::<Panel>(entity).map(|panel| Selection {
        record_uid: panel.record.clone(),
        data: panel.data,
    })
}

#[derive(Clone, Default)]
struct Rule {
    organs: Vec<String>,
    lower: String,
    upper: String,
    lower_inclusive: bool,
    upper_inclusive: bool,
}

impl From<&Condition> for Rule {
    fn from(condition: &Condition) -> Self {
        Self {
            organs: condition.organs.clone(),
            lower: condition
                .lower
                .map_or_else(String::new, |bound| bound.value.to_string()),
            upper: condition
                .upper
                .map_or_else(String::new, |bound| bound.value.to_string()),
            lower_inclusive: condition.lower.is_none_or(|bound| bound.inclusive),
            upper_inclusive: condition.upper.is_none_or(|bound| bound.inclusive),
        }
    }
}

impl Rule {
    fn condition(&self) -> Result<Condition, String> {
        let bound = |value: &str, inclusive| {
            if value.trim().is_empty() {
                Ok(None)
            } else {
                value
                    .trim()
                    .parse::<u32>()
                    .map(|value| Some(Bound { value, inclusive }))
                    .map_err(|_| {
                        "Proximity bounds must be whole numbers from 0 to 4294967295".to_string()
                    })
            }
        };
        Ok(Condition {
            organs: self.organs.clone(),
            lower: bound(&self.lower, self.lower_inclusive)?,
            upper: bound(&self.upper, self.upper_inclusive)?,
        })
    }
}

#[derive(Component)]
pub struct Panel {
    record: String,
    data: Data,
    context: Option<Context>,
    include: Vec<Rule>,
    exclude: Vec<Rule>,
    fields: Vec<(bool, usize, bool, Entity)>,
    pending: Option<(String, std::time::Instant)>,
    notice: String,
    choosing_record: bool,
    choosing_organs: Option<(bool, usize)>,
    show_credits: bool,
}

#[derive(Clone)]
enum Intent {
    ChooseRecord,
    Record(String),
    Data(Data),
    Add(bool),
    Remove(bool, usize),
    Organ(bool, usize, String),
    ChooseOrgans(bool, usize),
    Inclusive(bool, usize, bool),
    Save,
    Refresh,
    Preview,
    Credits,
}

#[derive(Component, Clone)]
struct Button {
    panel: Entity,
    intent: Intent,
}

pub struct VisibilityUiPlugin;

impl Plugin for VisibilityUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Request>().add_systems(Update, maintain);
    }
}

pub fn mount(world: &mut World, parent: Entity, record: &str, data: Data) -> Entity {
    let panel = world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
        ))
        .id();
    let mut state = Panel {
        record: record.into(),
        data,
        context: None,
        include: vec![],
        exclude: vec![],
        fields: vec![],
        pending: None,
        notice: "Choose a Record and the data whose visibility you want to control".into(),
        choosing_record: record.is_empty(),
        choosing_organs: None,
        show_credits: false,
    };
    request(
        world,
        &mut state,
        Command::Context {
            record_uid: record.into(),
            data,
        },
    );
    world.entity_mut(panel).insert(state);
    draw(world, panel);
    panel
}

fn request(world: &mut World, panel: &mut Panel, command: Command) {
    if !world.contains_resource::<Messages<Request>>() {
        return;
    }
    let id = nucleus::new_uid("visibility-ui");
    world.write_message(Request {
        id: id.clone(),
        command,
    });
    panel.pending = Some((id, std::time::Instant::now()));
    panel.notice = "Loading visibility…".into();
}

pub fn set_target(world: &mut World, entity: Entity, record: &str, data: Data) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    panel.record = record.into();
    panel.data = data;
    panel.context = None;
    panel.include.clear();
    panel.exclude.clear();
    panel.fields.clear();
    panel.choosing_record = record.is_empty();
    panel.choosing_organs = None;
    request(
        world,
        &mut panel,
        Command::Context {
            record_uid: record.into(),
            data,
        },
    );
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

fn label(world: &mut World, parent: Entity, caption: impl Into<String>) {
    let font = world.resource::<Typography>().text(15.0);
    world.spawn((
        ChildOf(parent),
        Text::new(caption),
        font,
        crate::style::text(crate::tokens::Token::Ink),
    ));
}

fn button(world: &mut World, parent: Entity, panel: Entity, caption: &str, intent: Intent) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Button { panel, intent },
            Node {
                min_height: px(36),
                padding: UiRect::all(px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::style::border(crate::tokens::Token::Accent),
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(caption);
    }
    label(world, entity, caption);
    world.entity_mut(entity).observe(
        |activate: On<bevy::ui_widgets::Activate>,
         buttons: Query<&Button>,
         mut commands: Commands| {
            if let Ok(button) = buttons.get(activate.entity) {
                let button = button.clone();
                commands.queue(move |world: &mut World| apply(world, button.panel, button.intent));
            }
        },
    );
}

fn input(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    label(world, parent, caption);
    let bundle = controls::single_line_editor(value, world.resource::<Typography>(), 0, 10);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(caption);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            bundle,
            bevy::a11y::AccessibilityNode::from(accessibility),
        ))
        .id()
}

fn policy(panel: &Panel) -> Result<Policy, String> {
    let policy = Policy {
        include: panel
            .include
            .iter()
            .map(Rule::condition)
            .collect::<Result<_, _>>()?,
        exclude: panel
            .exclude
            .iter()
            .map(Rule::condition)
            .collect::<Result<_, _>>()?,
    };
    policy.validate().map_err(str::to_owned)?;
    Ok(policy)
}

fn read_fields(world: &World, panel: &mut Panel) {
    for &(exclude, index, upper, entity) in &panel.fields {
        if let Some(text) = world.get::<bevy::text::EditableText>(entity) {
            let rules = if exclude {
                &mut panel.exclude
            } else {
                &mut panel.include
            };
            if let Some(rule) = rules.get_mut(index) {
                if upper {
                    rule.upper = text.value().to_string();
                } else {
                    rule.lower = text.value().to_string();
                }
            }
        }
    }
}

fn draw(world: &mut World, entity: Entity) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    read_fields(world, &mut panel);
    controls::clear(world, entity);
    panel.fields.clear();
    label(world, entity, "Visibility");
    label(
        world,
        entity,
        "Any matching include allows access. Any matching exclusion denies it. Lower proximity means a closer Organ.",
    );
    button(world, entity, entity, "Choose Record", Intent::ChooseRecord);
    if let Some(context) = &panel.context {
        if let Some(record) = context
            .records
            .iter()
            .find(|record| record.uid == panel.record)
        {
            label(world, entity, format!("Record: {}", record.label));
        }
        if panel.choosing_record {
            for record in &context.records {
                button(
                    world,
                    entity,
                    entity,
                    &record.label,
                    Intent::Record(record.uid.clone()),
                );
            }
        }
    }
    let row = controls::row(world, entity);
    for (data, caption) in [
        (Data::Record, "Record data"),
        (Data::Place, "Saved place"),
        (Data::LiveLocation, "Live location"),
    ] {
        button(
            world,
            row,
            entity,
            &format!("{}{}", if data == panel.data { "✓ " } else { "" }, caption),
            Intent::Data(data),
        );
    }
    label(
        world,
        entity,
        match panel.data {
            Data::Record => {
                "This policy bounds future Record sharing. Existing roles, grants, and sync settings still apply."
            }
            Data::Place => {
                "A saved place is retained context. Withholding it also withholds ordinary replication of its Record. Allowing replication can disclose earlier saved places in that Record's retained history."
            }
            Data::LiveLocation => {
                "Live coordinates expire. Named Person recipients can also view them, unless their Organ is excluded. The controller retains access."
            }
        },
    );
    for exclude in [false, true] {
        label(
            world,
            entity,
            if exclude {
                "Exclude conditions"
            } else {
                "Include conditions"
            },
        );
        let rules = if exclude {
            panel.exclude.clone()
        } else {
            panel.include.clone()
        };
        if rules.is_empty() {
            label(
                world,
                entity,
                if exclude {
                    "No excluded Organs"
                } else {
                    "No additional Organ access"
                },
            );
        }
        for (index, rule) in rules.iter().enumerate() {
            let block = controls::column(world, entity);
            label(
                world,
                block,
                format!(
                    "{} {} · all selected criteria must match",
                    if exclude { "Exclude" } else { "Include" },
                    index + 1
                ),
            );
            label(
                world,
                block,
                if rule.organs.is_empty() {
                    "Any known authenticated Organ"
                } else {
                    "Selected Organs only"
                },
            );
            button(
                world,
                block,
                entity,
                "Choose Organs for this condition",
                Intent::ChooseOrgans(exclude, index),
            );
            if let Some(context) = &panel.context {
                for organ in &context.organs {
                    if panel.choosing_organs != Some((exclude, index)) {
                        if rule.organs.contains(&organ.uid) {
                            label(world, block, &organ.label);
                        }
                        continue;
                    }
                    button(
                        world,
                        block,
                        entity,
                        &format!(
                            "{}{}",
                            if rule.organs.contains(&organ.uid) {
                                "✓ "
                            } else {
                                ""
                            },
                            organ.label
                        ),
                        Intent::Organ(exclude, index, organ.uid.clone()),
                    );
                }
            }
            let lower = input(
                world,
                block,
                "Lower proximity bound (empty = no lower limit)",
                &rule.lower,
            );
            panel.fields.push((exclude, index, false, lower));
            button(
                world,
                block,
                entity,
                if rule.lower_inclusive {
                    "Lower: at least (≥)"
                } else {
                    "Lower: greater than (>)"
                },
                Intent::Inclusive(exclude, index, false),
            );
            let upper = input(
                world,
                block,
                "Upper proximity bound (empty = no upper limit)",
                &rule.upper,
            );
            panel.fields.push((exclude, index, true, upper));
            button(
                world,
                block,
                entity,
                if rule.upper_inclusive {
                    "Upper: at most (≤)"
                } else {
                    "Upper: less than (<)"
                },
                Intent::Inclusive(exclude, index, true),
            );
            button(
                world,
                block,
                entity,
                "Remove condition",
                Intent::Remove(exclude, index),
            );
        }
        button(
            world,
            entity,
            entity,
            if exclude {
                "Add exclusion"
            } else {
                "Add inclusion"
            },
            Intent::Add(exclude),
        );
    }
    let row = controls::row(world, entity);
    button(world, row, entity, "Preview rules", Intent::Preview);
    button(world, row, entity, "Save visibility", Intent::Save);
    button(world, row, entity, "Reload saved rules", Intent::Refresh);
    label(
        world,
        entity,
        "Preview of these rules · the controller keeps their own access",
    );
    match policy(&panel) {
        Ok(policy) => {
            if let Some(context) = &panel.context {
                for organ in &context.organs {
                    let decision = policy.decide(&organ.uid, organ.proximity);
                    let allowed = organ.proximity.is_some() && decision.allowed;
                    let numbered = |values: &[usize]| {
                        values
                            .iter()
                            .map(|value| (value + 1).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    label(
                        world,
                        entity,
                        format!(
                            "{} · {} · proximity {} · include [{}], exclude [{}]",
                            organ.label,
                            if allowed {
                                "Allowed by these rules"
                            } else {
                                "Denied by these rules"
                            },
                            organ
                                .proximity
                                .map_or_else(|| "unavailable".into(), |value| value.to_string()),
                            numbered(&decision.included_by),
                            numbered(&decision.excluded_by)
                        ),
                    );
                }
            }
        }
        Err(error) => label(world, entity, error),
    }
    label(world, entity, &panel.notice);
    button(
        world,
        entity,
        entity,
        "Credits and licenses",
        Intent::Credits,
    );
    if panel.show_credits {
        label(
            world,
            entity,
            "Accessibility · AccessKit contributors · MIT",
        );
        label(world, entity, include_str!("../licenses/accesskit-MIT.txt"));
    }
    world.entity_mut(entity).insert(panel);
}

fn apply(world: &mut World, entity: Entity, intent: Intent) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    read_fields(world, &mut panel);
    if panel.pending.is_some() {
        world.entity_mut(entity).insert(panel);
        return;
    }
    panel.fields.clear();
    fn rules(panel: &mut Panel, exclude: bool) -> &mut Vec<Rule> {
        if exclude {
            &mut panel.exclude
        } else {
            &mut panel.include
        }
    }
    match intent {
        Intent::ChooseRecord => panel.choosing_record = !panel.choosing_record,
        Intent::Record(record) => {
            panel.record = record;
            panel.context = None;
            panel.include.clear();
            panel.exclude.clear();
            panel.choosing_record = false;
            panel.choosing_organs = None;
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data: panel.data,
            };
            request(world, &mut panel, command);
        }
        Intent::Data(data) => {
            panel.data = data;
            panel.context = None;
            panel.include.clear();
            panel.exclude.clear();
            panel.choosing_organs = None;
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data,
            };
            request(world, &mut panel, command);
        }
        Intent::Add(exclude) => {
            if panel.include.len() + panel.exclude.len() < nucleus::visibility::MAX_CONDITIONS {
                rules(&mut panel, exclude).push(Rule {
                    lower_inclusive: true,
                    upper_inclusive: true,
                    ..Default::default()
                });
            } else {
                panel.notice = "At most 64 conditions are allowed".into();
            }
        }
        Intent::Remove(exclude, index) => {
            panel.choosing_organs = None;
            if index < rules(&mut panel, exclude).len() {
                rules(&mut panel, exclude).remove(index);
            }
        }
        Intent::Organ(exclude, index, organ) => {
            if let Some(rule) = rules(&mut panel, exclude).get_mut(index) {
                if rule.organs.contains(&organ) {
                    rule.organs.retain(|uid| uid != &organ);
                } else if rule.organs.len() < nucleus::visibility::MAX_ORGANS_PER_CONDITION {
                    rule.organs.push(organ);
                }
            }
        }
        Intent::ChooseOrgans(exclude, index) => {
            panel.choosing_organs = if panel.choosing_organs == Some((exclude, index)) {
                None
            } else {
                Some((exclude, index))
            };
        }
        Intent::Inclusive(exclude, index, upper) => {
            if let Some(rule) = rules(&mut panel, exclude).get_mut(index) {
                if upper {
                    rule.upper_inclusive = !rule.upper_inclusive;
                } else {
                    rule.lower_inclusive = !rule.lower_inclusive;
                }
            }
        }
        Intent::Save => match policy(&panel) {
            Ok(policy) => {
                if let Some(context) = panel.context.as_ref().filter(|context| {
                    !panel.record.is_empty()
                        && context.record_uid == panel.record
                        && context.data == panel.data
                }) {
                    let expected_revision =
                        context.saved.as_ref().map_or(0, |saved| saved.revision);
                    let command = Command::Save {
                        record_uid: panel.record.clone(),
                        data: panel.data,
                        policy,
                        expected_revision,
                    };
                    request(world, &mut panel, command);
                } else {
                    panel.notice = "Load this Record's visibility before saving".into();
                }
            }
            Err(error) => panel.notice = error,
        },
        Intent::Refresh => {
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data: panel.data,
            };
            request(world, &mut panel, command);
        }
        Intent::Preview => {
            panel.notice =
                "Preview reflects the edited rules. Save to apply them to future deliveries.".into()
        }
        Intent::Credits => panel.show_credits = !panel.show_credits,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

pub fn receive(world: &mut World, id: &str, result: Result<serde_json::Value, String>) {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return;
    };
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    panel.pending = None;
    match result.and_then(|value| {
        serde_json::from_value::<Context>(value).map_err(|error| error.to_string())
    }) {
        Ok(context) => {
            let saved = context
                .saved
                .as_ref()
                .map_or_else(Policy::default, |saved| saved.policy.clone());
            panel.record = context.record_uid.clone();
            panel.data = context.data;
            panel.include = saved.include.iter().map(Rule::from).collect();
            panel.exclude = saved.exclude.iter().map(Rule::from).collect();
            panel.fields.clear();
            panel.context = Some(context);
            panel.notice =
                "Saved rules loaded. Previously delivered data cannot be recalled.".into();
        }
        Err(error) => panel.notice = error,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

fn maintain(world: &mut World) {
    let timed_out: Vec<_> = world
        .query::<&Panel>()
        .iter(world)
        .filter_map(|panel| {
            panel
                .pending
                .as_ref()
                .filter(|(_, at)| at.elapsed().as_secs() >= 15)
                .map(|(id, _)| id.clone())
        })
        .collect();
    for id in timed_out {
        receive(
            world,
            &id,
            Err("Visibility request timed out; reload before saving".into()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_previews_composed_rules_and_saves_the_loaded_revision() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let record = nucleus::new_uid("r");
        let organ = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, Data::LiveLocation);
        let id = world
            .get::<Panel>(panel)
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .0
            .clone();
        receive(
            &mut world,
            &id,
            Ok(serde_json::to_value(Context {
                record_uid: record.clone(),
                data: Data::LiveLocation,
                saved: Some(nucleus::visibility::SavedPolicy {
                    controller_uid: String::new(),
                    revision: 4,
                    policy: Policy::default(),
                }),
                records: vec![],
                organs: vec![nucleus::visibility::Organ {
                    uid: organ.clone(),
                    label: "Passenger".into(),
                    proximity: Some(2),
                }],
            })
            .unwrap()),
        );
        world.resource_mut::<Messages<Request>>().clear();
        apply(&mut world, panel, Intent::Add(false));
        apply(&mut world, panel, Intent::Organ(false, 0, organ.clone()));
        let upper = world
            .get::<Panel>(panel)
            .unwrap()
            .fields
            .iter()
            .find(|(exclude, _, upper, _)| !exclude && *upper)
            .unwrap()
            .3;
        world
            .get_mut::<bevy::text::EditableText>(upper)
            .unwrap()
            .editor_mut()
            .set_text("3");
        apply(&mut world, panel, Intent::Inclusive(false, 0, true));
        apply(&mut world, panel, Intent::Add(true));
        apply(&mut world, panel, Intent::Organ(true, 0, organ.clone()));
        apply(&mut world, panel, Intent::Preview);
        let policy = policy(world.get::<Panel>(panel).unwrap()).unwrap();
        assert_eq!(
            policy.include[0].upper,
            Some(Bound {
                value: 3,
                inclusive: false
            })
        );
        assert!(!policy.decide(&organ, Some(2)).allowed);
        assert!(
            world
                .query::<&Text>()
                .iter(&world)
                .any(|text| text.0.contains("Denied by these rules")
                    && text.0.contains("exclude [1]"))
        );
        apply(&mut world, panel, Intent::Save);
        let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
        assert_eq!(requests.len(), 1);
        assert!(
            matches!(&requests[0].command, Command::Save { expected_revision: 4, policy: saved, .. } if *saved == policy)
        );
        receive(
            &mut world,
            &requests[0].id,
            Err("Visibility changed; refresh before saving".into()),
        );
        assert_eq!(
            super::policy(world.get::<Panel>(panel).unwrap()).unwrap(),
            policy
        );
        assert!(
            world
                .get::<Panel>(panel)
                .unwrap()
                .notice
                .contains("refresh")
        );
    }
}
