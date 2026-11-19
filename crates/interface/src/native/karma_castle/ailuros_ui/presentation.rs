use super::*;
use crate::native::retained::{Children as RetainedChildren, Presentation};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Header,
    Editor,
    Audience,
    Device(String, usize),
}

enum Part<'a> {
    Header,
    Editor,
    Audience,
    Device(&'a Arc<Endpoint>, bool),
}

#[derive(Component, Default)]
struct Panel(RetainedChildren<Key>);

pub(super) fn render(world: &mut World, owner: Entity) {
    let _render = nucleus::performance::Span::new("ailuros-presentation");
    let state = world.get::<Devices>(owner).unwrap();
    let panel = state.panel;
    let source = state.rows.clone();
    let mut parts = vec![(Key::Header, Part::Header)];
    if state.editing {
        parts.push((Key::Editor, Part::Editor));
    }
    if !state.audience.is_empty() {
        parts.push((Key::Audience, Part::Audience));
    }
    let token = state.issued_token.as_ref().map(|(uid, _)| uid.as_str());
    let mut occurrences = HashMap::<&str, usize>::new();
    for row in source.iter() {
        let occurrence = occurrences.entry(&row.uid).or_default();
        parts.push((
            Key::Device(row.uid.clone(), *occurrence),
            Part::Device(row, token == Some(row.uid.as_str())),
        ));
        *occurrence += 1;
    }
    let mut children = world.entity_mut(panel).take::<Panel>().unwrap_or_default();
    children.0.reconcile(
        world,
        panel,
        parts,
        |world, parent, part| {
            let entity = ui::stack(world, parent);
            draw(world, owner, entity, part);
            nucleus::performance::add("ailuros-parts-created", 1);
            entity
        },
        |world, entity, part| draw(world, owner, entity, part),
    );
    world.entity_mut(panel).insert(children);
}

struct Device {
    row: Arc<Endpoint>,
    expired: bool,
    token: bool,
}

fn draw(world: &mut World, owner: Entity, panel: Entity, part: &Part<'_>) {
    match part {
        Part::Header => {
            if Presentation::matches(world, panel, |_: &()| true) {
                return;
            }
            world.entity_mut(panel).despawn_children();
            header(world, owner, panel);
            Presentation::remember(world, panel, ());
        }
        Part::Editor => editor(world, owner, panel),
        Part::Audience => {
            let audience = &world.get::<Devices>(owner).unwrap().audience;
            if Presentation::matches(world, panel, |old: &String| old == audience) {
                return;
            }
            let audience = audience.clone();
            world.entity_mut(panel).despawn_children();
            crate::native::edit_mode::label(
                world,
                panel,
                &format!("Server identity: {audience}"),
                12.0,
            );
            Presentation::remember(world, panel, audience);
        }
        Part::Device(row, token) => {
            let expired = row.expires_at <= chrono::Utc::now().timestamp();
            if Presentation::matches(world, panel, |old: &Device| {
                (Arc::ptr_eq(&old.row, row) || same_device(&old.row, row))
                    && old.expired == expired
                    && old.token == *token
            }) {
                if !Arc::ptr_eq(
                    &Presentation::<Device>::state(world, panel).unwrap().row,
                    row,
                ) {
                    Presentation::update_state(world, panel, |old: &mut Device| {
                        old.row = Arc::clone(row)
                    });
                }
                nucleus::performance::add("ailuros-devices-retained", 1);
                return;
            }
            world.entity_mut(panel).despawn_children();
            device(world, owner, panel, row, expired, *token);
            Presentation::remember(
                world,
                panel,
                Device {
                    row: Arc::clone(row),
                    expired,
                    token: *token,
                },
            );
            nucleus::performance::add("ailuros-devices-built", 1);
        }
    }
}

fn same_device(old: &Endpoint, row: &Endpoint) -> bool {
    old.uid == row.uid
        && old.name == row.name
        && old.signal_uid == row.signal_uid
        && old.enabled == row.enabled
        && old.revision == row.revision
        && old.credential_epoch == row.credential_epoch
        && old.cooldown_seconds == row.cooldown_seconds
        && old.minimum == row.minimum
        && old.maximum == row.maximum
        && old.expires_at == row.expires_at
        && old.last_sequence == row.last_sequence
        && old.last_received_at == row.last_received_at
}

fn header(world: &mut World, owner: Entity, panel: Entity) {
    crate::native::edit_mode::label(world, panel, "Ailuros Signals", 16.0);
    let line = ui::row(world, panel);
    crate::native::castle_feed::button(world, line, owner, "Enroll device", Control::New);
    crate::native::castle_feed::button(world, line, owner, "Refresh access", Control::Refresh);
    crate::native::edit_mode::label(
        world,
        panel,
        "Each device can update only its granted Signal. Access starts disabled. Use an access token or signed requests; private keys stay on the device.",
        13.0,
    );
}

struct Form {
    target: Option<String>,
    inputs: [Option<Entity>; 7],
    signal: Option<Entity>,
}

fn editor(world: &mut World, owner: Entity, panel: Entity) {
    let _editor = nucleus::performance::Span::new("ailuros-form-presentation");
    let state = world.get::<Devices>(owner).unwrap();
    if Presentation::matches(world, panel, |old: &Form| {
        old.target == state.target
            && old.inputs.iter().enumerate().all(|(index, field)| {
                field.is_none_or(|field| {
                    world.get::<EditableText>(field).is_some()
                        && world
                            .get::<Input>(field)
                            .is_some_and(|input| input.owner == owner && input.index == index)
                        && world
                            .get::<ChildOf>(field)
                            .is_some_and(|child| child.parent() == panel)
                })
            })
            && old.signal.is_none_or(|signal| {
                world.get::<Text>(signal).is_some()
                    && world
                        .get::<ChildOf>(signal)
                        .is_some_and(|child| child.parent() == panel)
            })
    }) {
        let old = Presentation::<Form>::state(world, panel).unwrap();
        let (inputs, signal) = (old.inputs, old.signal);
        for (index, field) in inputs.into_iter().enumerate() {
            let Some(field) = field else { continue };
            let value = &world.get::<Devices>(owner).unwrap().fields[index];
            if world.get::<EditableText>(field).unwrap().value() != value.as_str() {
                let value = value.clone();
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(&value);
                nucleus::performance::add("ailuros-editors-published", 1);
            }
        }
        if let Some(signal) = signal {
            let value = &world.get::<Devices>(owner).unwrap().fields[2];
            if &world.get::<Text>(signal).unwrap().0 != value {
                let value = value.clone();
                world.get_mut::<Text>(signal).unwrap().0 = value;
            }
        }
        return;
    }
    let target = state.target.clone();
    let new = target.is_none();
    let fields = state.fields.clone();
    let mut inputs = [None; 7];
    let mut signal = None;
    world.entity_mut(panel).despawn_children();
    for (index, label) in [
        (0, "Device name"),
        (
            1,
            "Optional device public key · P-256 compressed SEC1, lowercase hex",
        ),
        (
            2,
            if new {
                "Signal slug · existing or new"
            } else {
                "Granted Signal UID"
            },
        ),
        (3, "Minimum seconds between readings (1–86400)"),
        (4, "Minimum accepted value"),
        (5, "Maximum accepted value"),
        (6, "Access expiry · RFC3339 date and time"),
    ] {
        crate::native::edit_mode::label(world, panel, label, 13.0);
        if index == 2 && !new {
            signal = Some(crate::native::edit_mode::label(
                world,
                panel,
                &fields[index],
                13.0,
            ));
            continue;
        }
        let typography = world.resource::<crate::native::theme::Typography>();
        let bundle = crate::native::sand::text_editor(&fields[index], typography, 0);
        let entity = world
            .spawn(bundle)
            .insert((
                ChildOf(panel),
                Input { owner, index },
                Node {
                    width: percent(100),
                    min_height: px(30),
                    ..default()
                },
            ))
            .id();
        inputs[index] = Some(entity);
        let mut text = world.get_mut::<EditableText>(entity).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(128);
        let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
        accessible.set_label(label);
        world
            .entity_mut(entity)
            .insert(bevy::a11y::AccessibilityNode(accessible));
    }
    let line = ui::row(world, panel);
    crate::native::castle_feed::button(world, line, owner, "Save access", Control::Save);
    crate::native::castle_feed::button(world, line, owner, "Close editor", Control::Close);
    Presentation::remember(
        world,
        panel,
        Form {
            target,
            inputs,
            signal,
        },
    );
    nucleus::performance::add("ailuros-forms-built", 1);
}

fn device(
    world: &mut World,
    owner: Entity,
    panel: Entity,
    row: &Endpoint,
    expired: bool,
    token: bool,
) {
    let line = ui::row(world, panel);
    crate::native::edit_mode::label(
        world,
        line,
        &format!(
            "{} · {} · {}s cooldown · {} to {}",
            row.name,
            if !row.enabled {
                "disabled"
            } else if expired {
                "expired"
            } else {
                "enabled"
            },
            row.cooldown_seconds,
            row.minimum,
            row.maximum
        ),
        14.0,
    );
    let line = ui::row(world, panel);
    world.get_mut::<Node>(line).unwrap().flex_wrap = FlexWrap::Wrap;
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Edit access",
        Control::Edit(row.uid.clone()),
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Use for firmware setup",
        setup::Control::Select(row.uid.clone()),
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Copy setup",
        Control::CopySetup(row.uid.clone()),
    );
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Issue token",
        Control::IssueToken(row.uid.clone(), row.revision),
    );
    if token {
        crate::native::castle_feed::button(
            world,
            line,
            owner,
            "Copy issued token",
            Control::CopyToken(row.uid.clone()),
        );
    }
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        if row.enabled { "Disable" } else { "Enable" },
        Control::SetEnabled(row.uid.clone(), row.revision, !row.enabled),
    );
    crate::native::edit_mode::label(
        world,
        panel,
        &format!(
            "Device {} · Signal {} · access generation {} · sequence {}",
            row.uid, row.signal_uid, row.credential_epoch, row.last_sequence
        ),
        12.0,
    );
    crate::native::edit_mode::label(
        world,
        panel,
        &format!(
            "Last accepted reading: {} · expires: {}",
            row.last_received_at
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|at| at.to_rfc3339())
                .unwrap_or_else(|| "none".into()),
            chrono::DateTime::from_timestamp(row.expires_at, 0)
                .map(|at| at.to_rfc3339())
                .unwrap_or_default()
        ),
        12.0,
    );
}
