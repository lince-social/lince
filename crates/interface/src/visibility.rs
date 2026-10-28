use crate::{controls, theme::Typography};
use bevy::prelude::*;
use nucleus::visibility::{Bound, Command, Condition, Context, Data, Policy};

#[derive(Message, Clone)]
pub struct Request {
    pub owner: Entity,
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
    persons: Vec<String>,
    lower: String,
    upper: String,
    lower_inclusive: bool,
    upper_inclusive: bool,
    radius: String,
}

impl From<&Condition> for Rule {
    fn from(condition: &Condition) -> Self {
        Self {
            organs: condition.organs.clone(),
            persons: condition.persons.clone(),
            lower: condition
                .lower
                .map_or_else(String::new, |bound| bound.value.to_string()),
            upper: condition
                .upper
                .map_or_else(String::new, |bound| bound.value.to_string()),
            lower_inclusive: condition.lower.is_none_or(|bound| bound.inclusive),
            upper_inclusive: condition.upper.is_none_or(|bound| bound.inclusive),
            radius: condition
                .radius_metres
                .map_or_else(String::new, |radius| radius.to_string()),
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
            persons: self.persons.clone(),
            lower: bound(&self.lower, self.lower_inclusive)?,
            upper: bound(&self.upper, self.upper_inclusive)?,
            radius_metres: if self.radius.trim().is_empty() {
                None
            } else {
                Some(
                    self.radius
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| "Area radius must be a whole number of metres".to_string())?,
                )
            },
        })
    }
}

#[derive(Component)]
pub struct Panel {
    draft: Option<bool>,
    owner: Entity,
    record: String,
    data: Data,
    context: Option<Context>,
    include: Vec<Rule>,
    exclude: Vec<Rule>,
    fields: Vec<(bool, usize, bool, Entity)>,
    radius_fields: Vec<(usize, Entity)>,
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
    Person(bool, usize, String),
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
        draft: None,
        owner: panel,
        record: record.into(),
        data,
        context: None,
        include: vec![],
        exclude: vec![],
        fields: vec![],
        radius_fields: vec![],
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

pub fn mount_policy_draft(
    world: &mut World,
    parent: Entity,
    mut context: Context,
    policy: Policy,
    exclusions: bool,
) -> Entity {
    context.saved = None;
    context.data = Data::Record;
    let entity = world
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
    world.entity_mut(entity).insert(Panel {
        draft: Some(exclusions),
        owner: entity,
        record: context.record_uid.clone(),
        data: Data::Record,
        context: Some(context),
        include: policy.include.iter().map(Rule::from).collect(),
        exclude: policy.exclude.iter().map(Rule::from).collect(),
        fields: vec![],
        radius_fields: vec![],
        pending: None,
        notice: "Draft only; review all stages before enabling".into(),
        choosing_record: false,
        choosing_organs: None,
        show_credits: false,
    });
    draw(world, entity);
    entity
}

pub fn edited_policy(world: &mut World, entity: Entity) -> Result<Policy, String> {
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .ok_or("Visibility draft unavailable")?;
    read_fields(world, &mut panel);
    let result = policy(&panel);
    world.entity_mut(entity).insert(panel);
    result
}

fn request(world: &mut World, panel: &mut Panel, command: Command) {
    if !world.contains_resource::<Messages<Request>>() {
        return;
    }
    let id = nucleus::new_uid("visibility-ui");
    world.write_message(Request {
        owner: panel.owner,
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
    panel.radius_fields.clear();
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
            controls::Editor {
                title: caption.into(),
                secret: false,
            },
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
    policy.validate_for(panel.data).map_err(str::to_owned)?;
    Ok(policy)
}

fn read_fields(world: &World, panel: &mut Panel) {
    for &(index, entity) in &panel.radius_fields {
        if let Some(text) = world.get::<bevy::text::EditableText>(entity)
            && let Some(rule) = panel.include.get_mut(index)
        {
            rule.radius = text.value().to_string();
        }
    }
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
    panel.radius_fields.clear();
    label(world, entity, "Visibility");
    label(
        world,
        entity,
        "Any matching include allows access. Any matching exclusion denies it. Lower proximity means a closer Organ.",
    );
    if panel.draft.is_none() {
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
    }
    label(
        world,
        entity,
        match panel.data {
            Data::Record => {
                "These rules bound Record access and future sharing. Existing roles, grants, and sync settings still apply."
            }
            Data::Place => {
                "Choose an exact point or an approximate area for each inclusion. Approximate access withholds exact Record replication and address text. Linked places also require their own permission."
            }
            Data::LiveLocation => {
                "Live coordinates expire. Choose an exact point or an approximate area for each inclusion. Person and Organ exclusions override named invitations. The controller retains exact access."
            }
        },
    );
    for exclude in [false, true] {
        if panel.draft.is_some_and(|side| side != exclude) {
            continue;
        }
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
                    if rule.persons.is_empty() {
                        "Any known authenticated Organ"
                    } else {
                        "Selected Persons on their authenticated devices"
                    }
                } else {
                    "Selected Organs only"
                },
            );
            button(
                world,
                block,
                entity,
                "Choose Organs and Persons for this condition",
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
                label(
                    world,
                    block,
                    "Persons (empty = no Person restriction; selected Organ and proximity criteria still apply)",
                );
                for person in &context.persons {
                    if panel.choosing_organs != Some((exclude, index)) {
                        if rule.persons.contains(&person.uid) {
                            label(world, block, format!("Person: {}", person.label));
                        }
                        continue;
                    }
                    button(
                        world,
                        block,
                        entity,
                        &format!(
                            "{}Person: {}",
                            if rule.persons.contains(&person.uid) {
                                "✓ "
                            } else {
                                ""
                            },
                            person.label
                        ),
                        Intent::Person(exclude, index, person.uid.clone()),
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
            if !exclude && panel.data != Data::Record {
                let radius = input(
                    world,
                    block,
                    "Area radius in metres (empty = exact point; 100–100000)",
                    &rule.radius,
                );
                panel.radius_fields.push((index, radius));
                label(
                    world,
                    block,
                    "Matching inclusions grant the most precise allowed view. The area center is generalized before sharing.",
                );
            }
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
    if panel.draft.is_none() {
        button(world, row, entity, "Save visibility", Intent::Save);
        button(world, row, entity, "Reload saved rules", Intent::Refresh);
    }
    label(
        world,
        entity,
        "Preview of these rules · existing roles and authentication also apply",
    );
    match policy(&panel) {
        Ok(policy) => {
            if let Some(context) = &panel.context {
                for organ in &context.organs {
                    let decision = policy.decide(&organ.uid, organ.proximity);
                    let allowed = !organ.blocked && organ.proximity.is_some() && decision.allowed;
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
                                decision.radius_metres.map_or_else(
                                    || "Allowed · exact point".to_string(),
                                    |radius| {
                                        format!("Allowed · approximate area, radius {radius} m")
                                    },
                                )
                            } else {
                                "Denied by these rules".to_string()
                            },
                            organ
                                .proximity
                                .map_or_else(|| "unavailable".into(), |value| value.to_string()),
                            numbered(&decision.included_by),
                            numbered(&decision.excluded_by)
                        ),
                    );
                }
                let people = context.persons.iter().filter(|person| {
                    policy
                        .include
                        .iter()
                        .chain(&policy.exclude)
                        .any(|rule| rule.persons.contains(&person.uid))
                });
                let mut previews = 0;
                for (person, organ) in people
                    .flat_map(|person| context.organs.iter().map(move |organ| (person, organ)))
                    .take(64)
                {
                    let decision =
                        policy.decide_principal(&organ.uid, Some(&person.uid), organ.proximity);
                    let named = decision
                        .included_by
                        .iter()
                        .any(|index| !policy.include[*index].persons.is_empty());
                    label(
                        world,
                        entity,
                        format!(
                            "Person {} via {} · {}",
                            person.label,
                            organ.label,
                            if !organ.blocked
                                && decision.allowed
                                && (organ.proximity.is_some() || named)
                            {
                                "Allowed by these rules"
                            } else {
                                "Denied by these rules"
                            }
                        ),
                    );
                    previews += 1;
                }
                if previews == 64 {
                    label(
                        world,
                        entity,
                        "Showing at most 64 Person/Organ combinations",
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
    if panel.pending.is_some()
        || panel.draft.is_some()
            && matches!(
                &intent,
                Intent::Save
                    | Intent::Refresh
                    | Intent::ChooseRecord
                    | Intent::Record(_)
                    | Intent::Data(_)
            )
    {
        world.entity_mut(entity).insert(panel);
        return;
    }
    panel.fields.clear();
    panel.radius_fields.clear();
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
        Intent::Person(exclude, index, person) => {
            if let Some(rule) = rules(&mut panel, exclude).get_mut(index) {
                if rule.persons.contains(&person) {
                    rule.persons.retain(|uid| uid != &person);
                } else if rule.persons.len() < nucleus::visibility::MAX_ORGANS_PER_CONDITION {
                    rule.persons.push(person);
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

#[cfg(feature = "native-presentation")]
pub(crate) fn laboratory_cases(cases: &mut Vec<crate::native::laboratory::Case>) {
    tests::laboratory_cases(cases);
}

#[cfg(any(test, feature = "native-presentation"))]
mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn location_editor_saves_user_area_size_and_rejects_invalid_sizes() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, Data::Place);
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
                record_uid: record,
                data: Data::Place,
                saved: None,
                organs: vec![],
                persons: vec![],
                records: vec![],
            })
            .unwrap()),
        );
        world.resource_mut::<Messages<Request>>().clear();
        apply(&mut world, panel, Intent::Add(false));
        let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
        world
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor_mut()
            .set_text("2500");
        apply(&mut world, panel, Intent::Save);
        let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
        assert!(
            matches!(&requests[0].command, Command::Save { data: Data::Place, policy, .. } if policy.include[0].radius_metres == Some(2500))
        );
        world.get_mut::<Panel>(panel).unwrap().pending = None;
        let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
        world
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor_mut()
            .set_text("99");
        apply(&mut world, panel, Intent::Save);
        assert!(world.resource::<Messages<Request>>().is_empty());
        assert!(world.get::<Panel>(panel).unwrap().notice.contains("100000"));
        let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
        world
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .editor_mut()
            .set_text("");
        apply(&mut world, panel, Intent::Save);
        let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
        assert!(
            matches!(&requests[0].command, Command::Save { policy, .. } if policy.include[0].radius_metres.is_none())
        );
    }

    #[cfg_attr(test, test)]
    fn editor_previews_composed_rules_and_saves_the_loaded_revision() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let record = nucleus::new_uid("r");
        let organ = nucleus::new_uid("r");
        let person = nucleus::new_uid("r");
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
                persons: vec![nucleus::location::Choice {
                    uid: person.clone(),
                    label: "Passenger Person".into(),
                    node_id: None,
                }],
                organs: vec![nucleus::visibility::Organ {
                    uid: organ.clone(),
                    label: "Passenger".into(),
                    proximity: Some(2),
                    blocked: false,
                }],
            })
            .unwrap()),
        );
        world.resource_mut::<Messages<Request>>().clear();
        apply(&mut world, panel, Intent::Add(false));
        apply(&mut world, panel, Intent::Organ(false, 0, organ.clone()));
        apply(&mut world, panel, Intent::Person(false, 0, person.clone()));
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
        assert_eq!(policy.include[0].persons, vec![person]);
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

    #[cfg(feature = "native-presentation")]
    crate::native::laboratory_cases! {
        location_editor_saves_user_area_size_and_rejects_invalid_sizes,
        editor_previews_composed_rules_and_saves_the_loaded_revision,
    }
}
