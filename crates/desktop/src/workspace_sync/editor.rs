use super::*;
use engine::workspace_sync::RecordEdit;

#[derive(Component)]
struct Fields {
    record: Entity,
    head: Entity,
    body: Entity,
    slug: Entity,
    unit: Entity,
    namespace: Entity,
    extension: Entity,
    predicate: Entity,
    object: Entity,
    assertion: Entity,
    quantity: Entity,
    action: Entity,
}

#[derive(Clone, Copy)]
enum Edit {
    Head,
    Body,
    Slug,
    Unit,
    Extension,
    Assert,
    Retract,
    Value,
    Refine,
    Identity,
    Restore,
    Button,
}

impl Action for Edit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(fields) = world.get::<Fields>(owner) else {
            return;
        };
        let read = |entity| panel::value(world, entity).unwrap_or_default();
        let optional = |entity| {
            let value = read(entity);
            (!value.trim().is_empty()).then(|| value.trim().to_owned())
        };
        let record = selected_element(world, owner)
            .and_then(|element| {
                element
                    .component
                    .records()
                    .first()
                    .map(|record| (*record).to_owned())
            })
            .unwrap_or_else(|| read(fields.record).trim().to_owned());
        let edit = match self {
            Self::Head => RecordEdit::Text {
                head: Some(read(fields.head)),
                body: None,
            },
            Self::Body => RecordEdit::Text {
                head: None,
                body: Some(read(fields.body)),
            },
            Self::Slug => RecordEdit::Slug {
                slug: optional(fields.slug),
            },
            Self::Unit => RecordEdit::Unit {
                unit: optional(fields.unit),
            },
            Self::Extension => {
                let value = match serde_json::from_str(&read(fields.extension)) {
                    Ok(value) => value,
                    Err(error) => {
                        status(world, owner, &error.to_string());
                        return;
                    }
                };
                RecordEdit::Extension {
                    namespace: read(fields.namespace).trim().into(),
                    value,
                }
            }
            Self::Assert => RecordEdit::Assert {
                predicate: read(fields.predicate).trim().into(),
                object: optional(fields.object),
                quantity: optional(fields.quantity),
                unit: optional(fields.unit),
            },
            Self::Retract => RecordEdit::Retract {
                assertion: read(fields.assertion).trim().into(),
            },
            Self::Value => RecordEdit::AssertionValue {
                assertion: read(fields.assertion).trim().into(),
                quantity: optional(fields.quantity),
                unit: optional(fields.unit),
            },
            Self::Refine => RecordEdit::Refine {
                predicate: read(fields.predicate).trim().into(),
                object: read(fields.object).trim().into(),
            },
            Self::Identity => RecordEdit::Identity {
                predicate: optional(fields.predicate),
            },
            Self::Restore => {
                let slug = optional(fields.slug);
                propose(
                    world,
                    owner,
                    Change::RestoreRecord {
                        record: read(fields.record).trim().into(),
                        slug,
                        placement: nucleus::new_uid("placement"),
                        geometry: Geometry {
                            position: [200.0, 0.0],
                            size: [300.0, 160.0],
                        },
                    },
                );
                return;
            }
            Self::Button => {
                let action = match serde_json::from_str::<Value>(&read(fields.action)) {
                    Ok(action) => action,
                    Err(error) => {
                        status(world, owner, &error.to_string());
                        return;
                    }
                };
                if let Err(error) = engine::workspace_sync::adapters::action_change(&action) {
                    status(world, owner, &error.to_string());
                    return;
                }
                let label = read(fields.head);
                propose(
                    world,
                    owner,
                    Change::Add {
                        element: Element {
                            id: nucleus::new_uid("placement"),
                            geometry: Geometry {
                                position: [0.0, 0.0],
                                size: [300.0, 100.0],
                            },
                            component: LayoutComponent::Builtin {
                                state: ComponentState::Button { label, action },
                            },
                        },
                    },
                );
                return;
            }
        };
        propose(
            world,
            owner,
            Change::EditRecord {
                record,
                edits: vec![edit],
            },
        );
    }
}

pub(super) fn mount(world: &mut World, owner: Entity) {
    label(
        world,
        owner,
        "Shared Record editor · select a Record or enter its UID. Each edit waits for review.",
    );
    let record = panel::field(world, owner, "Record UID (also used for restoration)", "");
    let head = panel::field(world, owner, "Record head / control caption", "");
    world.get_mut::<EditableText>(head).unwrap().max_characters = Some(65536);
    panel::button(world, owner, owner, "Propose head", Edit::Head);
    let body = panel::field(world, owner, "Record body", "");
    world.get_mut::<EditableText>(body).unwrap().allow_newlines = true;
    world.get_mut::<EditableText>(body).unwrap().visible_lines = Some(4.0);
    world.get_mut::<EditableText>(body).unwrap().max_characters = Some(131072);
    panel::button(world, owner, owner, "Propose body", Edit::Body);
    let slug = panel::field(world, owner, "Slug (empty clears it)", "");
    panel::button(world, owner, owner, "Propose slug", Edit::Slug);
    let unit = panel::field(world, owner, "Unit Concept UID (empty clears it)", "");
    panel::button(world, owner, owner, "Propose unit", Edit::Unit);
    let namespace = panel::field(world, owner, "Custom extension namespace", "");
    let extension = panel::field(
        world,
        owner,
        "Complete extension fields (JSON object)",
        "{}",
    );
    world
        .get_mut::<EditableText>(extension)
        .unwrap()
        .max_characters = Some(65536);
    panel::button(
        world,
        owner,
        owner,
        "Propose replacing extension fields",
        Edit::Extension,
    );
    let predicate = panel::field(world, owner, "Assertion / identity Concept UID", "");
    let object = panel::field(
        world,
        owner,
        "Assertion target Record UID (empty = unary)",
        "",
    );
    let assertion = panel::field(world, owner, "Existing Assertion UID", "");
    let quantity = panel::field(
        world,
        owner,
        "Assertion quantity (empty = unquantified)",
        "",
    );
    for (caption, action) in [
        ("Propose assertion", Edit::Assert),
        ("Propose retraction", Edit::Retract),
        ("Propose assertion quantity / unit", Edit::Value),
        ("Refine unary assertion to a Record link", Edit::Refine),
        ("Propose identity (empty Concept clears it)", Edit::Identity),
        ("Propose restoring Record", Edit::Restore),
    ] {
        panel::button(world, owner, owner, caption, action);
    }
    let action = panel::field(
        world,
        owner,
        "Declared control Action",
        "{\"action\":\"workspace-record\",\"record\":\"\",\"changes\":{\"quantity\":\"+=1\"}}",
    );
    world
        .get_mut::<EditableText>(action)
        .unwrap()
        .max_characters = Some(65536);
    panel::button(
        world,
        owner,
        owner,
        "Propose control using this caption and Action",
        Edit::Button,
    );
    world.entity_mut(owner).insert(Fields {
        record,
        head,
        body,
        slug,
        unit,
        namespace,
        extension,
        predicate,
        object,
        assertion,
        quantity,
        action,
    });
}

#[derive(Clone)]
struct Invoke(Change);

#[derive(Component)]
struct RecordLabel {
    owner: Entity,
    record: String,
}

impl Action for Invoke {
    fn apply(&self, world: &mut World, owner: Entity) {
        propose(world, owner, self.0.clone());
    }
}

pub(super) fn presentation(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    element: &Element,
    suspended: bool,
) {
    fn draw(
        world: &mut World,
        owner: Entity,
        parent: Entity,
        component: &LayoutComponent,
        size: [f64; 2],
    ) {
        match component {
            LayoutComponent::Builtin {
                state: ComponentState::Text { text },
            } => {
                label(world, parent, text);
            }
            LayoutComponent::Builtin {
                state: ComponentState::Record { record, .. },
            } => {
                let field = label(world, parent, record);
                world.entity_mut(field).insert(RecordLabel {
                    owner,
                    record: record.clone(),
                });
            }
            LayoutComponent::Composition { composition } => {
                label(world, parent, &composition.name);
                for part in &composition.parts {
                    let container = world
                        .spawn((
                            ChildOf(parent),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px((size[0] / 2.0 + part.geometry.position[0]
                                    - part.geometry.size[0] / 2.0)
                                    as f32),
                                top: px((size[1] / 2.0 + part.geometry.position[1]
                                    - part.geometry.size[1] / 2.0)
                                    as f32),
                                width: px(part.geometry.size[0] as f32),
                                height: px(part.geometry.size[1] as f32),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                        ))
                        .id();
                    draw(world, owner, container, &part.component, part.geometry.size);
                }
            }
            _ => {}
        }
    }
    draw(
        world,
        owner,
        parent,
        &element.component,
        element.geometry.size,
    );
    if suspended {
        label(
            world,
            parent,
            "Controls suspended · review this configuration under the current policy",
        );
        return;
    }
    for (path, event, action) in engine::workspace_sync::adapters::controls(&element.component) {
        let caption = event.clone().unwrap_or_else(|| {
            if let LayoutComponent::Builtin {
                state: ComponentState::Button { label, .. },
            } = &element.component
            {
                label.clone()
            } else {
                format!("Use {}", path.join(" / "))
            }
        });
        panel::button(
            world,
            parent,
            owner,
            &caption,
            Invoke(Change::Invoke {
                element: element.id.clone(),
                path,
                event,
                action,
            }),
        );
    }
}

pub(super) fn records(world: &mut World, owner: Entity, rows: &[Value]) {
    let labels = world
        .query::<(Entity, &RecordLabel)>()
        .iter(world)
        .filter(|(_, label)| label.owner == owner)
        .map(|(entity, label)| (entity, label.record.clone()))
        .collect::<Vec<_>>();
    for (entity, record) in labels {
        if let Some(row) = rows.iter().find(|row| row["uid"] == record) {
            world.get_mut::<Text>(entity).unwrap().0 = format!(
                "{}\n{}\nQuantity: {}",
                row["head"].as_str().unwrap_or_default(),
                row["body"].as_str().unwrap_or_default(),
                row["quantity"]
            );
        }
    }
}
