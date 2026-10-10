use crate::{controls, theme::Typography};
use bevy::prelude::*;
use nucleus::location::{
    Choice, Command, Context, DEFAULT_DURATION_SECONDS, Fix, Settings, SourceKind, Status, View,
};
use std::time::{Duration, Instant};

#[derive(Message, Clone)]
pub struct Request {
    pub id: String,
    pub command: Command,
}

#[derive(Message, Clone)]
pub struct SavePlaceRequest {
    pub id: String,
    pub record: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Message, Clone)]
pub struct CopyReferenceRequest {
    pub panel: Entity,
    pub reference: String,
}

#[derive(Resource, Default)]
pub struct CopyReferencesAvailable;

#[derive(Message, Clone)]
pub struct AuthenticationRequest {
    pub id: String,
    pub node: String,
    pub record: String,
    pub person: String,
    pub expected_person: Option<String>,
    pub username: String,
    pub password: String,
}

#[derive(Component)]
pub struct Panel {
    record: String,
    person: String,
    context: Option<Context>,
    settings: Option<Settings>,
    transfer: String,
    minutes: String,
    latitude: String,
    longitude: String,
    place_latitude: String,
    place_longitude: String,
    reference: String,
    fields: Vec<(Field, Entity)>,
    status_text: Option<Entity>,
    view: Option<(View, Instant)>,
    notice: String,
    pending: Option<Pending>,
    next_poll: Instant,
    credits: Vec<(&'static str, &'static str)>,
    show_credits: bool,
    observer: bool,
    node: String,
    watching: bool,
    username: String,
    password: String,
    show_login: bool,
    authentication: Option<(String, Instant)>,
}

struct Pending {
    id: String,
    command: Command,
    then: Vec<Command>,
    sent: Instant,
    save_place: bool,
}

#[derive(Clone, Copy)]
enum Field {
    Minutes,
    Transfer,
    Latitude,
    Longitude,
    PlaceLatitude,
    PlaceLongitude,
    Person,
    Record,
    Node,
    Username,
    Password,
    Reference,
}

#[derive(Component)]
pub struct Editor {
    pub title: String,
    pub secret: bool,
}

#[derive(Component)]
struct PasswordMask(Entity);

#[derive(Component)]
pub struct Button {
    panel: Entity,
    intent: Intent,
}

#[derive(Clone)]
enum Intent {
    Person(String),
    Device(Choice),
    Recipient(String),
    Mode,
    Start,
    Approve,
    Stop,
    StopAll,
    Manual,
    Refresh,
    Credits,
    LoginForm,
    Authenticate,
    SavePlace,
    SetPlace,
    CopyReference,
    LoadReference,
}

pub struct LocationUiPlugin;

#[derive(Resource)]
struct UiTick(Instant);

impl Default for UiTick {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Plugin for LocationUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Request>()
            .init_resource::<UiTick>()
            .add_message::<SavePlaceRequest>()
            .add_message::<AuthenticationRequest>()
            .add_message::<CopyReferenceRequest>()
            .add_systems(Update, maintain)
            .add_systems(
                PostUpdate,
                masks
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::a11y::AccessibilitySystems::Update),
            );
    }
}

pub fn mount(
    world: &mut World,
    parent: Entity,
    record: &str,
    person: Option<&str>,
    transfer: Option<&str>,
) -> Entity {
    let entity = controls::column(world, parent);
    world.entity_mut(entity).insert(Panel {
        record: record.into(),
        person: person.unwrap_or_default().into(),
        context: None,
        settings: None,
        transfer: transfer.unwrap_or_default().into(),
        minutes: (DEFAULT_DURATION_SECONDS / 60).to_string(),
        latitude: String::new(),
        longitude: String::new(),
        place_latitude: String::new(),
        place_longitude: String::new(),
        reference: String::new(),
        fields: Vec::new(),
        status_text: None,
        view: None,
        notice: "Loading location controls…".into(),
        pending: None,
        next_poll: Instant::now(),
        credits: vec![(
            "Accessibility · AccessKit contributors · MIT",
            include_str!("../licenses/accesskit-MIT.txt"),
        )],
        show_credits: false,
        observer: false,
        node: String::new(),
        watching: false,
        username: String::new(),
        password: String::new(),
        show_login: false,
        authentication: None,
    });
    draw(world, entity);
    entity
}

pub fn mount_observer(
    world: &mut World,
    parent: Entity,
    record: &str,
    person: Option<&str>,
) -> Entity {
    let entity = mount(world, parent, record, person, None);
    if let Some(mut panel) = world.get_mut::<Panel>(entity) {
        panel.observer = true;
        panel.notice = "Enter the location reference shared with your Person".into();
    }
    draw(world, entity);
    entity
}

pub fn reference_copied(world: &mut World, entity: Entity, result: Result<(), String>) {
    let Some(mut panel) = world.get_mut::<Panel>(entity) else {
        return;
    };
    panel.notice = result.map_or_else(|error| error, |()| "Location reference copied. Share it through a private conversation; it grants no access by itself.".into());
    let status = panel.status_text;
    let notice = panel.notice.clone();
    if let Some(status) = status {
        controls::status(world, status, &notice);
    }
}

fn label(world: &mut World, parent: Entity, value: &str) -> Entity {
    let font = world.resource::<Typography>().text(16.0);
    world
        .spawn((
            ChildOf(parent),
            Text::new(value),
            font,
            crate::style::text(crate::tokens::Token::Ink),
        ))
        .id()
}

fn button(world: &mut World, parent: Entity, panel: Entity, title: &str, intent: Intent) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Button {
                panel,
                intent: intent.clone(),
            },
            Node {
                min_height: px(44),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::style::border(crate::tokens::Token::Accent),
        ))
        .id();
    label(world, entity, title);
    world.entity_mut(entity).observe(
        move |_: On<bevy::ui_widgets::Activate>, mut commands: Commands| {
            let intent = intent.clone();
            commands.queue(move |world: &mut World| apply(world, panel, intent));
        },
    );
}

fn input(world: &mut World, parent: Entity, title: &str, value: &str) -> Entity {
    label(world, parent, title);
    let bundle = controls::single_line_editor(value, world.resource::<Typography>(), 0, 128);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(title);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            bevy::a11y::AccessibilityNode::from(accessibility),
            Editor {
                title: title.into(),
                secret: false,
            },
            bundle,
        ))
        .id()
}

pub fn click(world: &mut World, entity: Entity) {
    if let Some(button) = world.get::<Button>(entity) {
        let (panel, intent) = (button.panel, button.intent.clone());
        apply(world, panel, intent);
    }
}

pub fn credits(world: &mut World, entity: Entity, credits: Vec<(&'static str, &'static str)>) {
    if let Some(mut panel) = world.get_mut::<Panel>(entity) {
        panel.credits.extend(credits);
    }
    draw(world, entity);
}

fn capture(world: &World, panel: &mut Panel) {
    for (field, entity) in &panel.fields {
        if let Ok(value) = controls::value(world, *entity) {
            match field {
                Field::Minutes => panel.minutes = value,
                Field::Transfer => panel.transfer = value,
                Field::Latitude => panel.latitude = value,
                Field::Longitude => panel.longitude = value,
                Field::PlaceLatitude => panel.place_latitude = value,
                Field::PlaceLongitude => panel.place_longitude = value,
                Field::Person => panel.person = value,
                Field::Record => panel.record = value,
                Field::Node => panel.node = value,
                Field::Username => panel.username = value,
                Field::Password => panel.password = value,
                Field::Reference => panel.reference = value,
            }
        }
    }
}

fn draw(world: &mut World, entity: Entity) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    controls::clear(world, entity);
    panel.fields.clear();
    label(world, entity, "Record location");
    label(
        world,
        entity,
        "Live position starts private. Named recipients and Organ visibility control sharing.",
    );
    panel.status_text = Some(label(world, entity, &panel.notice));
    if !panel.observer {
        if panel.transfer != panel.record {
            label(
                world,
                entity,
                "Saved place · retained independently of live sharing",
            );
            panel.fields.push((
                Field::PlaceLatitude,
                input(world, entity, "Saved latitude", &panel.place_latitude),
            ));
            panel.fields.push((
                Field::PlaceLongitude,
                input(world, entity, "Saved longitude", &panel.place_longitude),
            ));
            button(world, entity, entity, "Set saved place", Intent::SetPlace);
            label(
                world,
                entity,
                "Setting a place does not start acquisition or live sharing. Manage its audience with Saved place visibility.",
            );
        } else {
            label(
                world,
                entity,
                "Set agreed meeting places in Transfer terms. Live sharing never changes those terms.",
            );
        }
    }
    if panel.observer {
        let field = input(
            world,
            entity,
            "Private location reference",
            &panel.reference,
        );
        world
            .get_mut::<bevy::text::EditableText>(field)
            .unwrap()
            .max_characters = Some(1024);
        panel.fields.push((Field::Reference, field));
        button(
            world,
            entity,
            entity,
            "Use location reference",
            Intent::LoadReference,
        );
        label(
            world,
            entity,
            "A reference identifies a view. Its current visibility rules still decide access.",
        );
        panel.fields.push((
            Field::Person,
            input(
                world,
                entity,
                "Named recipient Person UID (optional for Organ access)",
                &panel.person,
            ),
        ));
        panel.fields.push((
            Field::Record,
            input(world, entity, "Shared location Record UID", &panel.record),
        ));
        panel.fields.push((
            Field::Node,
            input(
                world,
                entity,
                "Shared location viewing endpoint",
                &panel.node,
            ),
        ));
        label(
            world,
            entity,
            "This view grants no access to the Record or Transfer terms.",
        );
    } else if let Some(context) = &panel.context {
        if panel.person.is_empty() {
            label(world, entity, "Choose the Person controlling this location");
            for choice in &context.people {
                button(
                    world,
                    entity,
                    entity,
                    &choice.label,
                    Intent::Person(choice.uid.clone()),
                );
            }
        } else {
            let name = context
                .people
                .iter()
                .find(|choice| choice.uid == panel.person)
                .map_or(panel.person.as_str(), |choice| choice.label.as_str());
            label(world, entity, &format!("Controller: {name}"));
            if world.contains_resource::<CopyReferencesAvailable>() {
                button(
                    world,
                    entity,
                    entity,
                    "Copy private location reference",
                    Intent::CopyReference,
                );
            }
            label(
                world,
                entity,
                &format!(
                    "Location reference: {}\nViewing endpoint: {}",
                    panel.record, context.authority_node_id
                ),
            );
            if let Some(settings) = &panel.settings {
                label(world, entity, "Source device");
                let row = controls::row(world, entity);
                for device in &context.devices {
                    button(
                        world,
                        row,
                        entity,
                        &format!(
                            "{}{}",
                            if settings.source_cell_uid == device.uid {
                                "✓ "
                            } else {
                                ""
                            },
                            device.label
                        ),
                        Intent::Device(device.clone()),
                    );
                }
                button(
                    world,
                    entity,
                    entity,
                    match settings.source_kind {
                        SourceKind::Device => "Source: device location",
                        SourceKind::Manual => "Source: manually reported location",
                    },
                    Intent::Mode,
                );
                panel.fields.push((
                    Field::Minutes,
                    input(
                        world,
                        entity,
                        "Next session duration in minutes (1–1440)",
                        &panel.minutes,
                    ),
                ));
                panel.fields.push((
                    Field::Transfer,
                    input(
                        world,
                        entity,
                        "Next session: stop when this Transfer ends (optional UID)",
                        &panel.transfer,
                    ),
                ));
                label(
                    world,
                    entity,
                    "Named Person recipients · Organ rules apply separately",
                );
                label(
                    world,
                    entity,
                    "Stop before changing the active source or recipients.",
                );
                for person in &context.people {
                    if person.uid == panel.person {
                        continue;
                    }
                    button(
                        world,
                        entity,
                        entity,
                        &format!(
                            "{}{}",
                            if settings.recipients.contains(&person.uid) {
                                "✓ "
                            } else {
                                ""
                            },
                            person.label
                        ),
                        Intent::Recipient(person.uid.clone()),
                    );
                }
                if settings.source_kind == SourceKind::Manual {
                    panel.fields.push((
                        Field::Latitude,
                        input(world, entity, "Reported latitude", &panel.latitude),
                    ));
                    panel.fields.push((
                        Field::Longitude,
                        input(world, entity, "Reported longitude", &panel.longitude),
                    ));
                    button(
                        world,
                        entity,
                        entity,
                        "Update reported position",
                        Intent::Manual,
                    );
                }
                let row = controls::row(world, entity);
                button(world, row, entity, "Start location", Intent::Start);
                if settings.source_node_id == context.current_node_id {
                    button(world, row, entity, "Approve this device", Intent::Approve);
                }
                button(world, row, entity, "Stop", Intent::Stop);
                if panel.transfer != panel.record {
                    button(
                        world,
                        entity,
                        entity,
                        "Save current position as Record place",
                        Intent::SavePlace,
                    );
                }
                label(
                    world,
                    entity,
                    "Saving retains this position with its separate saved-place visibility policy.",
                );
            }
            button(
                world,
                entity,
                entity,
                "Stop all my location sharing",
                Intent::StopAll,
            );
        }
    }
    button(
        world,
        entity,
        entity,
        if panel.observer {
            "View shared location"
        } else {
            "Refresh location"
        },
        Intent::Refresh,
    );
    button(
        world,
        entity,
        entity,
        "Authenticate this device with the sharing Organ",
        Intent::LoginForm,
    );
    if panel.show_login {
        label(
            world,
            entity,
            "Use the Person's login at the location authority. Credentials stay in this login flow.",
        );
        panel.fields.push((
            Field::Username,
            input(world, entity, "Username", &panel.username),
        ));
        let password = input(world, entity, "Password", &panel.password);
        world
            .get_mut::<Editor>(password)
            .expect("password editor")
            .secret = true;
        world
            .entity_mut(password)
            .remove::<crate::style::TextToken>()
            .insert(TextColor(Color::NONE));
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(password) {
            node.set_role(accesskit::Role::PasswordInput);
            node.clear_value();
        }
        let font = world.resource::<Typography>().text(16.0);
        world.spawn((
            ChildOf(password),
            PasswordMask(password),
            Text::new(""),
            font,
            crate::style::text(crate::tokens::Token::Ink),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(4),
                top: px(4),
                ..default()
            },
        ));
        panel.fields.push((Field::Password, password));
        button(
            world,
            entity,
            entity,
            "Authenticate device",
            Intent::Authenticate,
        );
    }
    if !panel.credits.is_empty() {
        button(
            world,
            entity,
            entity,
            "Location credits and licenses",
            Intent::Credits,
        );
        if panel.show_credits {
            for (name, license) in &panel.credits {
                label(world, entity, name);
                label(world, entity, license);
            }
        }
    }
    world.entity_mut(entity).insert(panel);
}

fn queue(world: &mut World, panel: &mut Panel, command: Command, then: Vec<Command>) {
    let id = nucleus::new_uid("location-ui");
    panel.pending = Some(Pending {
        id: id.clone(),
        command: command.clone(),
        then,
        sent: Instant::now(),
        save_place: false,
    });
    panel.next_poll = Instant::now() + Duration::from_secs(3);
    world.write_message(Request { id, command });
}

fn apply(world: &mut World, entity: Entity, intent: Intent) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    capture(world, &mut panel);
    let result = (|| -> Result<bool, String> {
        if matches!(
            intent,
            Intent::Device(_) | Intent::Recipient(_) | Intent::Mode
        ) && (panel
            .view
            .as_ref()
            .is_some_and(|(view, _)| view.status != Status::Stopped)
            || panel.pending.as_ref().is_some_and(|pending| {
                matches!(
                    pending.command,
                    Command::Configure { .. } | Command::Start { .. } | Command::Approve { .. }
                )
            }))
        {
            return Err("Stop the current session before changing its source or recipients".into());
        }
        match intent {
            Intent::CopyReference => {
                let context = panel
                    .context
                    .as_ref()
                    .ok_or("Load location controls first")?;
                let reference = nucleus::location::Invitation {
                    record_uid: panel.record.clone(),
                    authority_node_id: context.authority_node_id.clone(),
                }
                .reference()
                .map_err(str::to_owned)?;
                world.write_message(CopyReferenceRequest {
                    panel: entity,
                    reference,
                });
                Ok(false)
            }
            Intent::LoadReference => {
                let invitation = nucleus::location::Invitation::parse(&panel.reference)
                    .map_err(str::to_owned)?;
                panel.record = invitation.record_uid;
                panel.node = invitation.authority_node_id;
                panel.view = None;
                panel.pending = None;
                panel.watching = false;
                panel.notice =
                    "Reference loaded. View shared location to check current access.".into();
                Ok(true)
            }
            Intent::SetPlace => {
                let latitude: f64 = panel
                    .place_latitude
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a valid saved latitude")?;
                let longitude: f64 = panel
                    .place_longitude
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a valid saved longitude")?;
                if !latitude.is_finite()
                    || !(-90.0..=90.0).contains(&latitude)
                    || !longitude.is_finite()
                    || !(-180.0..=180.0).contains(&longitude)
                {
                    return Err("Choose valid saved coordinates".into());
                }
                let id = nucleus::new_uid("location-ui");
                world.write_message(SavePlaceRequest {
                    id: id.clone(),
                    record: panel.record.clone(),
                    latitude,
                    longitude,
                });
                panel.pending = Some(Pending {
                    id,
                    command: Command::View {
                        person: panel.person.clone(),
                        record_uid: panel.record.clone(),
                    },
                    then: vec![],
                    sent: Instant::now(),
                    save_place: true,
                });
                panel.notice = "Saving the Record's place without starting sharing…".into();
                Ok(false)
            }
            Intent::SavePlace => {
                if panel
                    .settings
                    .as_ref()
                    .is_none_or(|settings| settings.controller_uid != panel.person)
                {
                    return Err("Only the location controller can save this position".into());
                }
                let (view, received) = panel.view.as_ref().ok_or("Wait for a fresh position")?;
                let fix = view.fix.as_ref().ok_or("Wait for a fresh position")?;
                if view
                    .expires_at_ms
                    .is_none_or(|expiry| expiry <= chrono::Utc::now().timestamp_millis())
                    || view
                        .age_ms
                        .unwrap_or_default()
                        .saturating_add(received.elapsed().as_millis() as u64)
                        >= nucleus::location::MAX_FIX_AGE_MS as u64
                {
                    return Err("Wait for a fresh position before saving it".into());
                }
                let id = nucleus::new_uid("location-ui");
                world.write_message(SavePlaceRequest {
                    id: id.clone(),
                    record: panel.record.clone(),
                    latitude: fix.latitude,
                    longitude: fix.longitude,
                });
                panel.pending = Some(Pending {
                    id,
                    command: Command::View {
                        person: panel.person.clone(),
                        record_uid: panel.record.clone(),
                    },
                    then: Vec::new(),
                    sent: Instant::now(),
                    save_place: true,
                });
                panel.notice = "Saving the Record's place…".into();
                Ok(false)
            }
            Intent::Person(person) => {
                panel.person = person;
                panel.context = None;
                panel.next_poll = Instant::now();
                panel.pending = None;
                Ok(true)
            }
            Intent::Device(device) => {
                let settings = panel
                    .settings
                    .as_mut()
                    .ok_or("Load location settings first")?;
                settings.source_cell_uid = device.uid;
                settings.source_node_id = device.node_id.ok_or("This device has no endpoint")?;
                Ok(true)
            }
            Intent::Recipient(person) => {
                let recipients = &mut panel
                    .settings
                    .as_mut()
                    .ok_or("Load location settings first")?
                    .recipients;
                if recipients.contains(&person) {
                    recipients.retain(|uid| uid != &person);
                } else {
                    recipients.push(person);
                }
                Ok(true)
            }
            Intent::Mode => {
                let settings = panel
                    .settings
                    .as_mut()
                    .ok_or("Load location settings first")?;
                settings.source_kind = if settings.source_kind == SourceKind::Device {
                    SourceKind::Manual
                } else {
                    SourceKind::Device
                };
                Ok(true)
            }
            Intent::Start => {
                if panel
                    .view
                    .as_ref()
                    .is_some_and(|(view, _)| view.status != Status::Stopped)
                {
                    return Err(
                        "Stop the current session before changing its source or recipients".into(),
                    );
                }
                let mut settings = panel.settings.clone().ok_or("Choose your Person first")?;
                let minutes = panel
                    .minutes
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "Enter a duration in whole minutes")?;
                settings.duration_seconds =
                    minutes.checked_mul(60).ok_or("Duration is too large")?;
                settings.transfer_uid =
                    (!panel.transfer.trim().is_empty()).then(|| panel.transfer.trim().into());
                settings.validate().map_err(str::to_string)?;
                let mut then = vec![Command::Start {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }];
                if panel
                    .context
                    .as_ref()
                    .is_some_and(|context| context.current_node_id == settings.source_node_id)
                {
                    then.push(Command::Approve {
                        person: panel.person.clone(),
                        record_uid: panel.record.clone(),
                    });
                }
                panel.settings = Some(settings.clone());
                queue(world, &mut panel, Command::Configure { settings }, then);
                Ok(false)
            }
            Intent::Approve => {
                let command = Command::Approve {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                };
                queue(world, &mut panel, command, Vec::new());
                Ok(false)
            }
            Intent::Stop | Intent::StopAll => {
                let command = if matches!(intent, Intent::StopAll) {
                    Command::StopAll {
                        person: panel.person.clone(),
                    }
                } else {
                    Command::Stop {
                        person: panel.person.clone(),
                        record_uid: panel.record.clone(),
                    }
                };
                panel.view = None;
                panel.notice = "Stopping location…".into();
                queue(world, &mut panel, command, Vec::new());
                Ok(false)
            }
            Intent::Manual => {
                let view = &panel
                    .view
                    .as_ref()
                    .ok_or("Start this manual source first")?
                    .0;
                let session_uid = view
                    .session_uid
                    .clone()
                    .ok_or("Start this manual source first")?;
                let sequence = chrono::Utc::now().timestamp_millis().max(1) as u64;
                let fix = Fix {
                    sequence,
                    latitude: panel
                        .latitude
                        .trim()
                        .parse()
                        .map_err(|_| "Enter a latitude")?,
                    longitude: panel
                        .longitude
                        .trim()
                        .parse()
                        .map_err(|_| "Enter a longitude")?,
                    accuracy_metres: None,
                    captured_at_ms: chrono::Utc::now().timestamp_millis(),
                };
                fix.validate(chrono::Utc::now().timestamp_millis())
                    .map_err(str::to_string)?;
                let command = Command::Publish {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                    session_uid,
                    fix,
                };
                queue(world, &mut panel, command, Vec::new());
                Ok(false)
            }
            Intent::Refresh => {
                if panel.observer {
                    if (!panel.person.is_empty() && !nucleus::valid_uid(&panel.person, "r"))
                        || !nucleus::valid_uid(&panel.record, "r")
                        || panel.node.len() != 64
                        || !panel.node.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        return Err(
                            "Enter your Person and the shared Record and endpoint identities"
                                .into(),
                        );
                    }
                    panel.watching = true;
                }
                panel.pending = None;
                panel.next_poll = Instant::now();
                Ok(false)
            }
            Intent::Credits => {
                panel.show_credits = !panel.show_credits;
                Ok(true)
            }
            Intent::LoginForm => {
                panel.show_login = !panel.show_login;
                Ok(true)
            }
            Intent::Authenticate => {
                if panel.authentication.is_some() {
                    return Err("Wait for the current device login".into());
                }
                if panel.username.trim().is_empty() || panel.password.is_empty() {
                    return Err("Enter the Person's username and password".into());
                }
                let node = if panel.observer {
                    panel.node.clone()
                } else {
                    panel
                        .context
                        .as_ref()
                        .map_or(String::new(), |context| context.authority_node_id.clone())
                };
                let id = nucleus::new_uid("location-auth");
                world.write_message(AuthenticationRequest {
                    id: id.clone(),
                    node,
                    record: panel.record.clone(),
                    person: panel.person.clone(),
                    expected_person: (!panel.observer && !panel.person.is_empty())
                        .then(|| panel.person.clone()),
                    username: panel.username.trim().into(),
                    password: std::mem::take(&mut panel.password),
                });
                for (field, input) in &panel.fields {
                    if matches!(field, Field::Password)
                        && let Some(mut editor) = world.get_mut::<bevy::text::EditableText>(*input)
                    {
                        editor.editor.set_text("");
                    }
                }
                panel.authentication = Some((id, Instant::now()));
                panel.notice = "Authenticating this device…".into();
                Ok(false)
            }
        }
    })();
    let redraw = match result {
        Ok(redraw) => redraw,
        Err(error) => {
            panel.notice = error;
            false
        }
    };
    if let Some(status) = panel.status_text {
        controls::status(world, status, &panel.notice);
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
}

pub fn receive(world: &mut World, id: &str, data: Result<serde_json::Value, String>) -> bool {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("location panel");
    capture(world, &mut panel);
    let pending = panel.pending.take().expect("location request");
    let mut redraw = false;
    match data {
        Err(error) => {
            panel.notice = error;
            panel.view = None;
        }
        Ok(data) => {
            if pending.save_place {
                panel.notice =
                    "Saved as the Record's place. Its saved-place visibility policy applies."
                        .into();
                panel.next_poll = Instant::now() + Duration::from_secs(5);
            } else if matches!(pending.command, Command::Context { .. }) {
                match serde_json::from_value::<Context>(data) {
                    Ok(context) => {
                        if panel.settings.is_none() || panel.context.is_none() {
                            panel.settings = context.settings.clone().or_else(|| {
                                (!panel.person.is_empty()).then(|| Settings {
                                    record_uid: panel.record.clone(),
                                    controller_uid: panel.person.clone(),
                                    source_cell_uid: context.current_cell_uid.clone(),
                                    source_node_id: context.current_node_id.clone(),
                                    source_kind: SourceKind::Device,
                                    duration_seconds: DEFAULT_DURATION_SECONDS,
                                    recipients: Vec::new(),
                                    transfer_uid: (!panel.transfer.is_empty())
                                        .then(|| panel.transfer.clone()),
                                })
                            });
                            if let Some(settings) = &panel.settings {
                                panel.minutes = (settings.duration_seconds / 60).to_string();
                                if let Some(transfer) = &settings.transfer_uid {
                                    panel.transfer = transfer.clone();
                                }
                            }
                            redraw = true;
                        }
                        panel.view = Some((context.view.clone(), Instant::now()));
                        panel.context = Some(context);
                        panel.notice.clear();
                    }
                    Err(_) => panel.notice = "Location controls returned an invalid reply".into(),
                }
            } else {
                let view = if data.get("view").is_some() {
                    data["view"].clone()
                } else {
                    data.clone()
                };
                if let Ok(mut view) = serde_json::from_value::<View>(view) {
                    view.age_ms = view
                        .age_ms
                        .map(|age| age.saturating_add(pending.sent.elapsed().as_millis() as u64));
                    panel.view = Some((view, Instant::now()));
                }
                panel.notice.clear();
                if !pending.then.is_empty() {
                    let mut then = pending.then;
                    let command = then.remove(0);
                    queue(world, &mut panel, command, then);
                } else {
                    panel.next_poll = Instant::now() + Duration::from_secs(3);
                }
            }
        }
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
    true
}

fn maintain(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Panel>>()
        .iter(world)
        .collect();
    if !entities.is_empty()
        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
    {
        let mut tick = world.resource_mut::<UiTick>();
        if tick.0.elapsed() >= Duration::from_secs(1) {
            tick.0 = Instant::now();
            wake.after(Duration::from_secs(1));
        }
    }
    for entity in entities {
        let mut panel = world
            .entity_mut(entity)
            .take::<Panel>()
            .expect("location panel");
        if panel
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent.elapsed() > Duration::from_secs(12))
        {
            panel.pending = None;
            panel.view = None;
            panel.notice = "Location device is unavailable".into();
            panel.next_poll = Instant::now() + Duration::from_secs(5);
        }
        if panel
            .authentication
            .as_ref()
            .is_some_and(|(_, sent)| sent.elapsed() > Duration::from_secs(50))
        {
            panel.authentication = None;
            panel.notice = "Device authentication timed out".into();
        }
        if panel.authentication.is_none()
            && panel.pending.is_none()
            && Instant::now() >= panel.next_poll
        {
            let command = if panel.observer {
                Command::Observe {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                    node_id: panel.node.clone(),
                }
            } else if panel.context.is_none() {
                Command::Context {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }
            } else {
                Command::View {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }
            };
            if (!panel.observer && (!panel.person.is_empty() || panel.context.is_none()))
                || (panel.observer && panel.watching)
            {
                queue(world, &mut panel, command, Vec::new());
            }
        }
        if let Some(status) = panel.status_text {
            let text = if !panel.notice.is_empty() {
                panel.notice.clone()
            } else {
                position_text(panel.view.as_ref())
            };
            controls::status(world, status, text);
        }
        world.entity_mut(entity).insert(panel);
    }
}

pub fn authentication_reply(world: &mut World, id: &str, result: Result<String, String>) -> bool {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .authentication
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("location panel");
    capture(world, &mut panel);
    panel.authentication = None;
    match result {
        Ok(person) => {
            if panel.person.is_empty() {
                panel.person = person;
            }
            panel.notice =
                "Device authenticated. You can start or view the permitted location.".into();
            panel.context = None;
            panel.pending = None;
            panel.next_poll = Instant::now();
            panel.show_login = false;
            panel.watching = panel.observer;
        }
        Err(error) => panel.notice = error,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
    true
}

fn masks(
    inputs: Query<&bevy::text::EditableText>,
    mut masks: Query<(&PasswordMask, &mut Text)>,
    mut editors: Query<(Entity, &Editor, &mut bevy::a11y::AccessibilityNode)>,
) {
    for (entity, editor, mut node) in &mut editors {
        if editor.secret {
            node.set_role(accesskit::Role::PasswordInput);
            node.clear_value();
        } else if let Ok(input) = inputs.get(entity) {
            node.set_value(input.value().to_string());
        }
    }
    for (mask, mut text) in &mut masks {
        if let Ok(input) = inputs.get(mask.0) {
            text.set_if_neq(Text::new("•".repeat(input.value().chars().count())));
        }
    }
}

fn position_text(view: Option<&(View, Instant)>) -> String {
    let Some((view, received)) = view else {
        return "Location stopped".into();
    };
    if view
        .expires_at_ms
        .is_some_and(|expiry| expiry <= chrono::Utc::now().timestamp_millis())
    {
        return "Location expired".into();
    }
    let age = view
        .age_ms
        .unwrap_or_default()
        .saturating_add(received.elapsed().as_millis() as u64);
    let ending = view.expires_at_ms.map_or(String::new(), |expiry| {
        format!(
            " · {} min left",
            expiry
                .saturating_sub(chrono::Utc::now().timestamp_millis())
                .saturating_add(59_999)
                / 60_000
        )
    });
    if age >= nucleus::location::MAX_FIX_AGE_MS as u64 && view.fix.is_some() {
        return "Location unavailable · last fix expired".into();
    }
    if let Some(fix) = &view.fix {
        let accuracy = fix
            .accuracy_metres
            .map_or(String::new(), |accuracy| format!(" · ±{accuracy:.0} m"));
        return format!(
            "{:.6}, {:.6}{accuracy} · {} s old{}{ending}",
            fix.latitude,
            fix.longitude,
            age / 1000,
            if age > 15_000 { " · stale" } else { "" }
        );
    }
    match view.status {
        Status::Stopped => "Location stopped",
        Status::AwaitingApproval => "Waiting for approval on the selected source device",
        Status::Unavailable => "Device location unavailable · check location permission",
        _ => "Waiting for a fresh location fix",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_a_location_reference_never_starts_viewing_or_tracking() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let panel = mount_observer(&mut world, parent, "", None);
        let invitation = nucleus::location::Invitation {
            record_uid: nucleus::new_uid("r"),
            authority_node_id: "a".repeat(64),
        };
        world.get_mut::<Panel>(panel).unwrap().reference = invitation.reference().unwrap();
        world.get_mut::<Panel>(panel).unwrap().fields.clear();
        apply(&mut world, panel, Intent::LoadReference);
        let state = world.get::<Panel>(panel).unwrap();
        assert_eq!(state.record, invitation.record_uid);
        assert_eq!(state.node, invitation.authority_node_id);
        assert!(!state.watching);
        assert!(state.settings.is_none());
        assert!(world.resource::<Messages<Request>>().is_empty());
    }

    #[test]
    fn setting_a_saved_place_never_requests_live_acquisition() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<SavePlaceRequest>>();
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, None, None);
        assert!(world.get::<Panel>(panel).unwrap().context.is_none());
        assert!(
            world
                .get::<Panel>(panel)
                .unwrap()
                .fields
                .iter()
                .any(|(field, _)| matches!(field, Field::PlaceLatitude))
        );
        world.resource_mut::<Messages<Request>>().clear();
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.pending = None;
            state.place_latitude = "-23.5".into();
            state.place_longitude = "-46.6".into();
            state.fields.clear();
        }
        apply(&mut world, panel, Intent::SetPlace);
        let requests: Vec<_> = world
            .resource_mut::<Messages<SavePlaceRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].record, record);
        assert_eq!(requests[0].latitude, -23.5);
        assert!(world.resource::<Messages<Request>>().is_empty());
        assert!(world.get::<Panel>(panel).unwrap().settings.is_none());
    }

    fn view() -> View {
        View {
            record_uid: nucleus::new_uid("r"),
            status: Status::Live,
            session_uid: Some(nucleus::new_uid("location")),
            expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 60_000),
            source_kind: Some(SourceKind::Device),
            fix: Some(Fix {
                sequence: 1,
                latitude: -23.5505,
                longitude: -46.6333,
                accuracy_metres: Some(8.0),
                captured_at_ms: chrono::Utc::now().timestamp_millis(),
            }),
            age_ms: Some(0),
        }
    }

    #[test]
    fn live_recipient_edits_keep_showing_the_effective_audience_until_stopped() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        let parent = world.spawn_empty().id();
        let person = nucleus::new_uid("r");
        let recipient = nucleus::new_uid("r");
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, Some(&person), None);
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.settings = Some(Settings {
                record_uid: record,
                controller_uid: person,
                source_cell_uid: nucleus::new_uid("r"),
                source_node_id: "a".repeat(64),
                source_kind: SourceKind::Device,
                duration_seconds: DEFAULT_DURATION_SECONDS,
                recipients: vec![recipient.clone()],
                transfer_uid: None,
            });
            state.view = Some((view(), Instant::now()));
        }
        apply(&mut world, panel, Intent::Recipient(recipient.clone()));
        assert_eq!(
            world
                .get::<Panel>(panel)
                .unwrap()
                .settings
                .as_ref()
                .unwrap()
                .recipients,
            vec![recipient]
        );
        assert!(world.get::<Panel>(panel).unwrap().notice.contains("Stop"));
    }

    #[test]
    fn completed_view_polls_wait_before_requesting_another_fix() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let panel = mount(
            &mut world,
            parent,
            &nucleus::new_uid("r"),
            Some(&nucleus::new_uid("r")),
            None,
        );
        {
            let mut state = world.entity_mut(panel).take::<Panel>().unwrap();
            let command = Command::View {
                person: state.person.clone(),
                record_uid: state.record.clone(),
            };
            queue(&mut world, &mut state, command, Vec::new());
            world.entity_mut(panel).insert(state);
        }
        let id = world
            .get::<Panel>(panel)
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .id
            .clone();
        assert!(receive(
            &mut world,
            &id,
            Ok(serde_json::to_value(view()).unwrap())
        ));
        assert!(
            world.get::<Panel>(panel).unwrap().next_poll > Instant::now() + Duration::from_secs(2)
        );
    }

    #[test]
    fn saving_requires_a_fresh_fix_and_an_explicit_request() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<SavePlaceRequest>>();
        let parent = world.spawn_empty().id();
        let person = nucleus::new_uid("r");
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, Some(&person), None);
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.settings = Some(Settings {
                record_uid: record.clone(),
                controller_uid: person,
                source_cell_uid: nucleus::new_uid("r"),
                source_node_id: "a".repeat(64),
                source_kind: SourceKind::Device,
                duration_seconds: DEFAULT_DURATION_SECONDS,
                recipients: Vec::new(),
                transfer_uid: None,
            });
            state.view = Some((view(), Instant::now() - Duration::from_secs(61)));
        }
        apply(&mut world, panel, Intent::SavePlace);
        assert!(world.resource::<Messages<SavePlaceRequest>>().is_empty());
        world.get_mut::<Panel>(panel).unwrap().view = Some((view(), Instant::now()));
        apply(&mut world, panel, Intent::SavePlace);
        let requests: Vec<_> = world
            .resource_mut::<Messages<SavePlaceRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].record, record);
        assert_eq!(requests[0].latitude, -23.5505);
        assert!(receive(
            &mut world,
            &requests[0].id,
            Ok(serde_json::Value::Null)
        ));
        assert!(world.get::<Panel>(panel).unwrap().notice.contains("Saved"));
    }

    #[test]
    fn device_authentication_keeps_the_confirmed_observer_identity() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        let parent = world.spawn_empty().id();
        let panel = mount_observer(&mut world, parent, "", None);
        world.get_mut::<Panel>(panel).unwrap().authentication =
            Some(("login".into(), Instant::now()));
        let person = nucleus::new_uid("r");
        assert!(authentication_reply(
            &mut world,
            "login",
            Ok(person.clone())
        ));
        assert_eq!(world.get::<Panel>(panel).unwrap().person, person);
        assert!(world.get::<Panel>(panel).unwrap().watching);
    }

    #[test]
    fn expired_positions_disappear_using_local_elapsed_time() {
        let old = (view(), Instant::now() - Duration::from_secs(61));
        let text = position_text(Some(&old));
        assert!(!text.contains("23.5505"));
        assert!(text.contains("expired"));
    }

    #[test]
    fn stopping_hides_coordinates_and_ignores_earlier_view_replies() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let panel = mount(
            &mut world,
            parent,
            &nucleus::new_uid("r"),
            Some(&nucleus::new_uid("r")),
            None,
        );
        let current = view();
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.view = Some((current.clone(), Instant::now()));
            state.pending = Some(Pending {
                id: "old-view".into(),
                command: Command::View {
                    person: state.person.clone(),
                    record_uid: state.record.clone(),
                },
                then: Vec::new(),
                sent: Instant::now(),
                save_place: false,
            });
        }
        apply(&mut world, panel, Intent::Stop);
        assert!(world.get::<Panel>(panel).unwrap().view.is_none());
        assert!(!receive(
            &mut world,
            "old-view",
            Ok(serde_json::to_value(current).unwrap())
        ));
        assert!(world.get::<Panel>(panel).unwrap().view.is_none());
    }
}
