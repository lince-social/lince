use crate::{controls, theme::Typography};
use bevy::prelude::*;
use nucleus::location::{
    Choice, Command, Context, DEFAULT_DURATION_SECONDS, Fix, Settings, SourceKind, Status, View,
};
use std::time::{Duration, Instant};

mod clock;
#[cfg(test)]
#[path = "location/invitation_tests.rs"]
mod invitation_tests;
pub use clock::ElapsedClock;
use clock::Stamp;

#[derive(Message, Clone)]
pub struct Request {
    pub owner: Entity,
    pub id: String,
    pub command: Command,
}

#[derive(Message, Clone)]
pub struct SavePlaceRequest {
    pub owner: Entity,
    pub id: String,
    pub record: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Message, Clone)]
pub struct SavedLocationRequest {
    pub owner: Entity,
    pub id: String,
    pub command: nucleus::vocabulary::SavedLocationCommand,
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
    pub owner: Entity,
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
    owner: Entity,
    record: String,
    person: String,
    context: Option<Context>,
    settings: Option<Settings>,
    transfer: String,
    minutes: String,
    latitude: String,
    longitude: String,
    manual_sequence: u64,
    handover: Option<HandoverReview>,
    invitation: Option<nucleus::location::Invitation>,
    invitation_review: Option<InvitationReview>,
    invite_recipient: String,
    invite_minutes: String,
    invite_radius: String,
    invite_conversation: String,
    place_latitude: String,
    place_longitude: String,
    address: String,
    saved_location: Option<nucleus::vocabulary::SavedLocation>,
    addresses: Vec<nucleus::vocabulary::LocationCandidate>,
    saved_pending: Option<(String, nucleus::vocabulary::SavedLocationCommand, Instant)>,
    saved_poll: Instant,
    reference: String,
    fields: Vec<(Field, Entity)>,
    status_text: Option<Entity>,
    view: Option<(View, Stamp)>,
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
    sent: Stamp,
    save_place: bool,
    scope: Option<DraftScope>,
}

#[derive(Clone, PartialEq)]
struct DraftScope {
    person: String,
    record: String,
    node: String,
    settings: Option<Settings>,
    minutes: String,
    transfer: String,
    invitation_fields: [String;4],
}

impl DraftScope {
    fn of(panel: &Panel) -> Self {Self {person:panel.person.clone(),record:panel.record.clone(),node:panel.node.clone(),settings:panel.settings.clone(),minutes:panel.minutes.clone(),transfer:panel.transfer.clone(),invitation_fields:[panel.invite_recipient.clone(),panel.invite_minutes.clone(),panel.invite_radius.clone(),panel.invite_conversation.clone()]}}
}

struct HandoverReview {
    session: String,
    settings: Settings,
    scope: DraftScope,
}

struct InvitationReview {
    invitation: nucleus::location::Invitation,
    remaining_millis: u64,
    received: Stamp,
    scope: DraftScope,
}

#[derive(Clone, Copy)]
enum Field {
    InviteRecipient,
    InviteMinutes,
    InviteRadius,
    InviteConversation,
    Minutes,
    Transfer,
    Latitude,
    Longitude,
    PlaceLatitude,
    PlaceLongitude,
    Address,
    Person,
    Record,
    Node,
    Username,
    Password,
    Reference,
}

pub use crate::controls::Editor;

#[derive(Component)]
struct PasswordMask(Entity);

#[derive(Component)]
pub struct Button {
    panel: Entity,
    intent: Intent,
}

#[derive(Clone)]
enum Intent {
    ReviewInvitation,
    SendInvitation,
    CancelInvitation,
    Person(String),
    Device(Choice),
    Recipient(String),
    Mode,
    Start,
    ReviewHandover,
    ConfirmHandover,
    CancelHandover,
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
    SearchAddress,
    BindAddress(String),
    SelectSaved(Option<String>),
    RefreshSaved,
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
            .add_message::<SavedLocationRequest>()
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
    world.init_resource::<Messages<SavedLocationRequest>>();
    let entity = controls::column(world, parent);
    world.entity_mut(entity).insert(Panel {
        owner: entity,
        record: record.into(),
        person: person.unwrap_or_default().into(),
        context: None,
        settings: None,
        transfer: transfer.unwrap_or_default().into(),
        minutes: (DEFAULT_DURATION_SECONDS / 60).to_string(),
        latitude: String::new(),
        longitude: String::new(),
        manual_sequence: 0,
        handover: None,
        invitation: None,
        invitation_review: None,
        invite_recipient: String::new(),
        invite_minutes: "60".into(),
        invite_radius: "1000".into(),
        invite_conversation: String::new(),
        place_latitude: String::new(),
        place_longitude: String::new(),
        address: String::new(),
        saved_location: None,
        addresses: Vec::new(),
        saved_pending: None,
        saved_poll: Instant::now(),
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

pub fn load_reference(world: &mut World, entity: Entity, reference: &str) -> Result<(),String> {
    nucleus::location::Invitation::parse(reference).map_err(str::to_owned)?;
    let mut panel = world.get_mut::<Panel>(entity).ok_or("Location controls are unavailable")?;
    if !panel.observer {return Err("Open shared location controls for this reference".into());}
    panel.reference = reference.into();
    panel.fields.clear();
    drop(panel);
    apply(world,entity,Intent::LoadReference);
    Ok(())
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

fn clear_private_actor(panel: &mut Panel) {
    panel.handover = None;
    panel.invitation_review = None;
    panel.invite_recipient.clear();
    panel.invite_conversation.clear();
    panel.view = None;
    panel.settings = None;
    panel.context = None;
    panel.pending = None;
    panel.saved_location = None;
    panel.saved_pending = None;
    panel.addresses.clear();
    panel.latitude.clear();
    panel.longitude.clear();
    panel.place_latitude.clear();
    panel.place_longitude.clear();
    panel.address.clear();
    panel.password.clear();
    panel.username.clear();
    panel.watching = false;
    panel.authentication = None;
}

pub fn actor_changed(world: &mut World, person: &str) {
    let panels = world.query::<(Entity,&Panel)>().iter(world).filter(|(_,panel)|panel.person != person).map(|(entity,_)|entity).collect::<Vec<_>>();
    for entity in panels {
        let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {continue;};
        clear_private_actor(&mut panel);
        if panel.observer {panel.record.clear();panel.node.clear();panel.reference.clear();panel.invitation = None;}
        panel.person = person.into();
        panel.next_poll = Instant::now();
        world.entity_mut(entity).insert(panel);
        draw(world,entity);
    }
}

fn capture(world: &World, panel: &mut Panel) {
    for (field, entity) in panel.fields.clone() {
        if let Ok(value) = controls::value(world, entity) {
            match field {
                Field::InviteRecipient => panel.invite_recipient = value,
                Field::InviteMinutes => panel.invite_minutes = value,
                Field::InviteRadius => panel.invite_radius = value,
                Field::InviteConversation => panel.invite_conversation = value,
                Field::Minutes => panel.minutes = value,
                Field::Transfer => panel.transfer = value,
                Field::Latitude => panel.latitude = value,
                Field::Longitude => panel.longitude = value,
                Field::PlaceLatitude => panel.place_latitude = value,
                Field::PlaceLongitude => panel.place_longitude = value,
                Field::Address => panel.address = value,
                Field::Person => {
                    if panel.person != value {clear_private_actor(panel);panel.person = value;break;}
                }
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
            panel.fields.push((
                Field::Address,
                input(
                    world,
                    entity,
                    "Address Record (@slug or Record ID)",
                    &panel.address,
                ),
            ));
            button(
                world,
                entity,
                entity,
                "Find saved addresses",
                Intent::SearchAddress,
            );
            button(
                world,
                entity,
                entity,
                "Link address Record",
                Intent::BindAddress(String::new()),
            );
            for address in &panel.addresses {
                button(
                    world,
                    entity,
                    entity,
                    &format!(
                        "Link {} · {:.5}, {:.5}",
                        address.head, address.latitude, address.longitude
                    ),
                    Intent::BindAddress(address.record_uid.clone()),
                );
            }
            if let Some(location) = &panel.saved_location {
                let status = if let Some(selected) = &location.selected {
                    if let Some(radius) = selected.radius_metres {
                        format!("Approximate saved area · radius {radius} m")
                    } else {
                        format!(
                            "Using {} · {:.5}, {:.5}",
                            if selected.assertion_uid.is_none() {
                                "this Record's coordinates"
                            } else {
                                &selected.head
                            },
                            selected.latitude,
                            selected.longitude
                        )
                    }
                } else if location.unavailable {
                    "The selected saved location is unavailable. Choose another explicitly.".into()
                } else if !location.candidates.is_empty() {
                    "Choose which saved location to use.".into()
                } else {
                    "No saved location. Set coordinates here or link a named address Record.".into()
                };
                label(world, entity, &status);
                for candidate in &location.candidates {
                    let title = if candidate.assertion_uid.is_none() {
                        "Use this Record's coordinates".into()
                    } else {
                        format!("Use {}", candidate.head)
                    };
                    button(
                        world,
                        entity,
                        entity,
                        &title,
                        Intent::SelectSaved(candidate.assertion_uid.clone()),
                    );
                }
            }
            button(
                world,
                entity,
                entity,
                "Refresh saved location",
                Intent::RefreshSaved,
            );
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
        if let Some(grant) = panel.invitation.as_ref().and_then(|invitation|invitation.grant.as_ref()) {
            let expiry=chrono::DateTime::from_timestamp_millis(grant.expires_at_ms).map(|time|time.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_else(||"unavailable".into());
            label(world,entity,&format!("Intended Person: {}. Minimum shared radius: {}. Expires at {expiry} on the source clock. Authenticate as this recipient; current permissions can further restrict or deny access.",grant.recipient,grant.radius_metres.map_or("permitted exact position".into(),|radius|format!("{radius} metres"))));
        }
        panel.fields.push((
            Field::Person,
            input(
                world,
                entity,
                if panel.invitation.as_ref().is_some_and(|invitation|invitation.grant.is_some()) {"Named recipient Person UID (required)"} else {"Named recipient Person UID (optional for Organ access)"},
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
                label(world, entity, "Selected source device");
                if let Some(active) = &context.settings {
                    if panel.view.as_ref().is_some_and(|(view,_)|view.status != Status::Stopped) {
                        let name = context.devices.iter().find(|device|device.uid == active.source_cell_uid).map_or(active.source_cell_uid.as_str(),|device|device.label.as_str());
                        label(world,entity,&format!("Current session source: {name}"));
                    }
                }
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
                        SourceKind::Device => "Selected source: device location",
                        SourceKind::Manual => "Selected source: manually reported location",
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
                    if panel.view.as_ref().is_some_and(|(view,_)|view.source_kind == Some(SourceKind::Manual) && view.status != Status::AwaitingApproval && view.session_uid.is_some()) && context.settings.as_ref().is_some_and(|active|active.source_node_id == settings.source_node_id) {
                        button(world,entity,entity,"Update reported position",Intent::Manual);
                    }
                }
                let row = controls::row(world, entity);
                button(world, row, entity, "Start location", Intent::Start);
                if panel.view.as_ref().is_some_and(|(view,_)|view.session_uid.is_some() && view.status != Status::Stopped) {
                    button(world,row,entity,"Review source handover",Intent::ReviewHandover);
                }
                if settings.source_node_id == context.current_node_id && context.settings.as_ref().is_some_and(|active|active.source_cell_uid == settings.source_cell_uid && active.source_kind == settings.source_kind) && panel.view.as_ref().is_some_and(|(view,_)|view.status == Status::AwaitingApproval) {
                    button(world, row, entity, "Approve this device", Intent::Approve);
                }
                button(world, row, entity, "Stop", Intent::Stop);
                if let Some(review) = &panel.handover {
                    let source = context.devices.iter().find(|device|device.uid == review.settings.source_cell_uid).map_or(review.settings.source_cell_uid.as_str(),|device|device.label.as_str());
                    label(world,entity,&format!("Replace the current source with {source} ({}) for {} minutes. Mode: {:?}. Recipients: {}. Transfer end condition: {}. The previous session ends; the replacement needs approval on its device. No position or approval is copied.",review.settings.source_node_id,review.settings.duration_seconds/60,review.settings.source_kind,review.settings.recipients.join(", "),review.settings.transfer_uid.as_deref().unwrap_or("none")));
                    button(world,entity,entity,"Confirm reviewed handover",Intent::ConfirmHandover);
                    button(world,entity,entity,"Cancel handover review",Intent::CancelHandover);
                }
                if panel.view.as_ref().is_some_and(|(view,_)|view.session_uid.is_some()) {
                    label(world,entity,"Private conversation invitation");
                    panel.fields.push((Field::InviteRecipient,input(world,entity,"Recipient Person record",&panel.invite_recipient)));
                    panel.fields.push((Field::InviteConversation,input(world,entity,"Private conversation record",&panel.invite_conversation)));
                    panel.fields.push((Field::InviteMinutes,input(world,entity,"Invitation lifetime in minutes",&panel.invite_minutes)));
                    panel.fields.push((Field::InviteRadius,input(world,entity,"Minimum shared radius in metres (blank for permitted exact position)",&panel.invite_radius)));
                    label(world,entity,"The reference contains no coordinates. It does not grant Record access; the named recipient must authenticate and still pass current visibility rules.");
                    button(world,entity,entity,"Review private invitation",Intent::ReviewInvitation);
                }
                if let Some(review) = &panel.invitation_review {
                    let grant = review.invitation.grant.as_ref().expect("Scoped invitation review");
                    let left = review.remaining_millis.saturating_sub(review.received.elapsed().as_millis() as u64);
                    label(world,entity,&format!("Send Record {} at {} to Person {} in private conversation {}. Radius: {}. At most {} seconds remain. The ordinary conversation and delivery permissions apply.",review.invitation.record_uid,review.invitation.authority_node_id,grant.recipient,panel.invite_conversation,grant.radius_metres.map_or("permitted exact position".into(),|radius|format!("{radius} metres")),left/1000));
                    button(world,entity,entity,"Send reviewed private invitation",Intent::SendInvitation);
                    button(world,entity,entity,"Cancel invitation review",Intent::CancelInvitation);
                }
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

fn configured_settings(panel: &Panel) -> Result<Settings,String> {
    let mut settings = panel.settings.clone().ok_or("Choose your Person first")?;
    if settings.controller_uid != panel.person {return Err("Load settings for the selected Person first".into());}
    let minutes = panel.minutes.trim().parse::<u32>().map_err(|_|"Enter a duration in whole minutes")?;
    settings.duration_seconds = minutes.checked_mul(60).ok_or("Duration is too large")?;
    settings.transfer_uid = (!panel.transfer.trim().is_empty()).then(||panel.transfer.trim().into());
    settings.validate().map_err(str::to_string)?;
    Ok(settings)
}

fn observer_command(panel: &Panel) -> Result<Command,String> {
    if let Some(invitation) = panel.invitation.as_ref().filter(|invitation|invitation.grant.is_some()) {
        if invitation.record_uid != panel.record || invitation.authority_node_id != panel.node {return Err("Reload the scoped invitation before viewing a changed reference".into());}
        if panel.person.is_empty() {return Err("Authenticate the named recipient to open this invitation".into());}
        Ok(Command::ObserveInvitation {person:panel.person.clone(),invitation:invitation.clone()})
    } else {Ok(Command::Observe {person:panel.person.clone(),record_uid:panel.record.clone(),node_id:panel.node.clone()})}
}

fn queue(world: &mut World, panel: &mut Panel, command: Command, then: Vec<Command>) {
    let id = nucleus::new_uid("location-ui");
    panel.pending = Some(Pending {
        id: id.clone(),
        command: command.clone(),
        then,
        sent: Stamp::for_world(world),
        save_place: false,
        scope: Some(DraftScope::of(panel)),
    });
    panel.next_poll = Instant::now() + Duration::from_secs(3);
    world.write_message(Request { owner: panel.owner, id, command });
}

fn queue_saved(
    world: &mut World,
    panel: &mut Panel,
    command: nucleus::vocabulary::SavedLocationCommand,
) {
    let id = nucleus::new_uid("saved-location-ui");
    panel.saved_pending = Some((id.clone(), command.clone(), Instant::now()));
    panel.saved_poll = Instant::now() + Duration::from_secs(10);
    world.write_message(SavedLocationRequest { owner: panel.owner, id, command });
}

fn receive_saved(world: &mut World, id: &str, data: &Result<serde_json::Value, String>) -> bool {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .saved_pending
                .as_ref()
                .is_some_and(|(pending, _, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("saved location panel");
    capture(world, &mut panel);
    let (_, command, _) = panel.saved_pending.take().expect("saved location request");
    let mut redraw = false;
    match data {
        Err(error) => panel.notice = error.clone(),
        Ok(data) => {
            if matches!(
                command,
                nucleus::vocabulary::SavedLocationCommand::Search { .. }
            ) {
                match serde_json::from_value::<Vec<nucleus::vocabulary::LocationCandidate>>(
                    data["addresses"].clone(),
                ) {
                    Ok(addresses) => {
                        panel.addresses = addresses;
                        panel.notice = "Choose an address Record with saved coordinates".into();
                        redraw = true;
                    }
                    Err(_) => panel.notice = "The saved address response was invalid".into(),
                }
            } else {
                match serde_json::from_value::<nucleus::vocabulary::SavedLocation>(data.clone()) {
                    Ok(location) if location.record_uid == panel.record => {
                        redraw = panel.saved_location.as_ref() != Some(&location);
                        panel.saved_location = Some(location);
                        if !matches!(
                            command,
                            nucleus::vocabulary::SavedLocationCommand::Inspect { .. }
                        ) {
                            panel.addresses.clear();
                            panel.notice = "Saved location updated".into();
                            redraw = true;
                        }
                    }
                    _ => panel.notice = "The saved location response was invalid".into(),
                }
            }
        }
    }
    panel.saved_poll = Instant::now() + Duration::from_secs(10);
    if let Some(status) = panel.status_text {
        controls::status(world, status, &panel.notice);
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
    true
}

fn apply(world: &mut World, entity: Entity, intent: Intent) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    capture(world, &mut panel);
    let result = (|| -> Result<bool, String> {
        if matches!(intent,Intent::Recipient(_)) && (panel
            .view
            .as_ref()
            .is_some_and(|(view, _)| view.status != Status::Stopped)
            || panel.pending.as_ref().is_some_and(|pending| {
                matches!(
                    pending.command,
                    Command::Configure { .. } | Command::Handover {..} | Command::Start { .. } | Command::Approve { .. }
                )
            }))
        {
            return Err("Stop the current session before changing its source or recipients".into());
        }
        if matches!(intent,Intent::Device(_)|Intent::Mode|Intent::Start|Intent::Approve|Intent::Manual|Intent::ReviewHandover|Intent::ConfirmHandover|Intent::ReviewInvitation|Intent::SendInvitation|Intent::Refresh) && panel.pending.is_some() {return Err("Wait for the current source request".into());}
        match intent {
            Intent::ReviewInvitation => {
                let recipient = panel.invite_recipient.trim().to_owned();
                let conversation = panel.invite_conversation.trim();
                if !nucleus::valid_uid(&recipient,"r") || !nucleus::valid_uid(conversation,"r") {return Err("Choose the recipient Person and an ordinary private conversation Record".into());}
                let minutes = panel.invite_minutes.trim().parse::<u32>().map_err(|_|"Enter an invitation lifetime in whole minutes")?;
                let duration_seconds = minutes.checked_mul(60).ok_or("Invitation duration is too large")?;
                if duration_seconds == 0 || duration_seconds > nucleus::location::MAX_DURATION_SECONDS {return Err("Choose an invitation lifetime of 1–1440 minutes".into());}
                let radius_metres = if panel.invite_radius.trim().is_empty() {None} else {Some(panel.invite_radius.trim().parse::<u32>().map_err(|_|"Enter a radius in whole metres")?)};
                if radius_metres.is_some_and(|radius|!nucleus::place::valid_area_radius(radius)) {return Err("Choose a supported geographic area radius".into());}
                panel.invitation_review = None;
                let command = Command::PrepareInvitation {person:panel.person.clone(),record_uid:panel.record.clone(),recipient,duration_seconds,radius_metres};
                queue(world,&mut panel,command,vec![]);
                Ok(false)
            }
            Intent::SendInvitation => {
                let review = panel.invitation_review.take().ok_or("Review the private invitation first")?;
                if review.scope != DraftScope::of(&panel) || review.remaining_millis <= review.received.elapsed().as_millis() as u64 {return Err("The invitation draft changed or expired; review it again".into());}
                let command = Command::SendInvitation {person:panel.person.clone(),conversation:panel.invite_conversation.trim().into(),invitation:review.invitation};
                queue(world,&mut panel,command,vec![]);
                Ok(true)
            }
            Intent::CancelInvitation => {panel.invitation_review = None;Ok(true)}
            Intent::SearchAddress
            | Intent::BindAddress(_)
            | Intent::SelectSaved(_)
            | Intent::RefreshSaved => {
                use nucleus::vocabulary::SavedLocationCommand as Saved;
                if panel.saved_pending.is_some() {
                    return Err("Wait for the saved location change to finish".into());
                }
                let command = match intent {
                    Intent::SearchAddress => Saved::Search {
                        text: panel.address.clone(),
                    },
                    Intent::BindAddress(address) => Saved::Bind {
                        record: panel.record.clone(),
                        address: if address.is_empty() {
                            panel.address.trim().into()
                        } else {
                            address
                        },
                    },
                    Intent::SelectSaved(assertion) => Saved::Select {
                        record: panel.record.clone(),
                        assertion,
                        expected_revision: panel
                            .saved_location
                            .as_ref()
                            .map_or(0, |location| location.revision),
                    },
                    _ => Saved::Inspect {
                        record: panel.record.clone(),
                    },
                };
                queue_saved(world, &mut panel, command);
                panel.notice = "Loading saved location…".into();
                Ok(false)
            }
            Intent::CopyReference => {
                let context = panel
                    .context
                    .as_ref()
                    .ok_or("Load location controls first")?;
                let reference = nucleus::location::Invitation {
                    record_uid: panel.record.clone(),
                    authority_node_id: context.authority_node_id.clone(),
                    grant: None,
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
                panel.record = invitation.record_uid.clone();
                panel.node = invitation.authority_node_id.clone();
                panel.invitation = Some(invitation);
                panel.view = None;
                panel.pending = None;
                panel.authentication = None;
                panel.password.clear();
                panel.username.clear();
                panel.show_login = false;
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
                    owner: entity,
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
                    sent: Stamp::for_world(world),
                    save_place: true,
                    scope: None,
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
                    .remaining_millis
                    .is_none_or(|remaining| remaining <= received.elapsed().as_millis() as u64)
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
                    owner: entity,
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
                    sent: Stamp::for_world(world),
                    save_place: true,
                    scope: None,
                });
                panel.notice = "Saving the Record's place…".into();
                Ok(false)
            }
            Intent::Person(person) => {
                clear_private_actor(&mut panel);
                panel.person = person;
                panel.context = None;
                panel.next_poll = Instant::now();
                panel.pending = None;
                Ok(true)
            }
            Intent::Device(device) => {
                panel.handover = None;
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
                panel.handover = None;
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
                let settings = configured_settings(&panel)?;
                let then = vec![Command::Start {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }];
                panel.settings = Some(settings.clone());
                queue(world, &mut panel, Command::Configure { settings }, then);
                Ok(false)
            }
            Intent::ReviewHandover => {
                if panel.pending.is_some() {return Err("Wait for the current request".into());}
                let session = panel.view.as_ref().and_then(|(view,_)|view.session_uid.clone()).ok_or("Start a location session before handing over")?;
                let settings = configured_settings(&panel)?;
                panel.handover = Some(HandoverReview {session,settings,scope:DraftScope::of(&panel)});
                Ok(true)
            }
            Intent::ConfirmHandover => {
                let review = panel.handover.take().ok_or("Review the replacement source first")?;
                if review.scope != DraftScope::of(&panel) || panel.view.as_ref().and_then(|(view,_)|view.session_uid.as_deref()) != Some(review.session.as_str()) {return Err("The source or private settings changed; review handover again".into());}
                panel.settings = Some(review.settings.clone());
                panel.view = None;
                let then = vec![Command::Context {person:panel.person.clone(),record_uid:panel.record.clone()}];
                queue(world,&mut panel,Command::Handover {settings:review.settings,expected_session_uid:review.session},then);
                panel.notice = "Changing source; approval is required on the replacement device".into();
                Ok(true)
            }
            Intent::CancelHandover => {panel.handover = None;Ok(true)}
            Intent::Approve => {
                let settings = panel.settings.as_ref().ok_or("Load the source settings first")?;
                if !panel.context.as_ref().and_then(|context|context.settings.as_ref()).is_some_and(|active|active.source_cell_uid == settings.source_cell_uid && active.source_node_id == settings.source_node_id && active.source_kind == settings.source_kind) {return Err("Review handover before approving a changed source".into());}
                let command = Command::Approve {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                    expected_session_uid: panel.view.as_ref().and_then(|(view,_)|view.session_uid.clone()).ok_or("Load the pending source session before approving it")?,
                };
                queue(world, &mut panel, command, Vec::new());
                Ok(false)
            }
            Intent::Stop | Intent::StopAll => {
                panel.handover = None;
                panel.invitation_review = None;
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
                let settings = panel.settings.as_ref().ok_or("Load the source settings first")?;
                if !panel.context.as_ref().and_then(|context|context.settings.as_ref()).is_some_and(|active|active.source_cell_uid == settings.source_cell_uid && active.source_kind == SourceKind::Manual && settings.source_kind == SourceKind::Manual) {return Err("Start and approve this manual source first".into());}
                let view = &panel
                    .view
                    .as_ref()
                    .ok_or("Start this manual source first")?
                    .0;
                let session_uid = view
                    .session_uid
                    .clone()
                    .ok_or("Start this manual source first")?;
                let sequence = view.next_sequence.unwrap_or(1).max(panel.manual_sequence.saturating_add(1));
                panel.manual_sequence = sequence;
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
                    capture_age_ms: 0,
                };
                fix.validate()
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
                    owner: entity,
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
    let stamp = Stamp::for_world(world);
    if receive_saved(world, id, &data) {
        return true;
    }
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
    let Some(pending) = panel.pending.take() else {world.entity_mut(entity).insert(panel);draw(world,entity);return true;};
    if pending.scope.as_ref().is_some_and(|scope|scope != &DraftScope::of(&panel)) {
        panel.handover = None;
        panel.invitation_review = None;
        panel.view = None;
        panel.notice = "The private source draft changed. Refresh location controls before continuing".into();
        world.entity_mut(entity).insert(panel);
        draw(world,entity);
        return true;
    }
    let mut redraw = false;
    match data {
        Err(error) => {
            panel.notice = error;
            panel.view = None;
        }
        Ok(data) => {
            if let Command::Configure {settings} | Command::Handover {settings,..} = &pending.command {
                if let Some(context) = &mut panel.context {context.settings = Some(settings.clone());}
            }
            if let Command::PrepareInvitation {person,record_uid,recipient,duration_seconds,radius_metres} = &pending.command {
                let invitation = serde_json::from_value::<nucleus::location::Invitation>(data["invitation"].clone());
                match invitation {
                    Ok(invitation) if invitation.validate().is_ok()
                        && &invitation.record_uid == record_uid
                        && panel.context.as_ref().is_some_and(|context|context.authority_node_id == invitation.authority_node_id)
                        && invitation.grant.as_ref().is_some_and(|grant|&grant.issuer == person && &grant.recipient == recipient && &grant.radius_metres == radius_metres)
                        && data["remaining_millis"].as_u64().is_some_and(|remaining|remaining > 0 && remaining <= u64::from(*duration_seconds)*1000) => {
                        let remaining_millis = data["remaining_millis"].as_u64().unwrap_or(0).saturating_sub(pending.sent.elapsed().as_millis() as u64);
                        panel.invitation_review = Some(InvitationReview {invitation,remaining_millis,received:stamp,scope:DraftScope::of(&panel)});
                        panel.notice = "Review the recipient, conversation, precision and remaining lifetime before sending".into();
                        redraw = true;
                    }
                    _ => panel.notice = "The invitation response was invalid".into(),
                }
            } else if matches!(pending.command,Command::SendInvitation {..}) {
                panel.notice = "Private invitation saved for ordinary conversation delivery. Check the conversation for delivery status".into();
                redraw = true;
            } else if pending.save_place {
                panel.saved_poll = Instant::now();
                panel.notice =
                    "Saved as the Record's place. Its saved-place visibility policy applies."
                        .into();
                panel.next_poll = Instant::now() + Duration::from_secs(5);
            } else if matches!(pending.command, Command::Context { .. }) {
                match serde_json::from_value::<Context>(data) {
                    Ok(mut context) => {
                        context.view.elapse(pending.sent.elapsed().as_millis() as u64);
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
                        panel.view = Some((context.view.clone(), stamp));
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
                    view.elapse(pending.sent.elapsed().as_millis() as u64);
                    redraw |= panel.view.as_ref().is_none_or(|(previous,_)|previous.status!=view.status || previous.session_uid!=view.session_uid || previous.source_kind!=view.source_kind || previous.fix.is_some()!=view.fix.is_some());
                    panel.view = Some((view, stamp));
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
            .saved_pending
            .as_ref()
            .is_some_and(|(_, _, sent)| sent.elapsed() > Duration::from_secs(12))
        {
            panel.saved_pending = None;
            panel.notice = "Saved location request timed out".into();
            panel.saved_poll = Instant::now() + Duration::from_secs(10);
        }
        if !panel.observer
            && panel.transfer != panel.record
            && panel.saved_pending.is_none()
            && Instant::now() >= panel.saved_poll
        {
            let record = panel.record.clone();
            queue_saved(
                world,
                &mut panel,
                nucleus::vocabulary::SavedLocationCommand::Inspect { record },
            );
        }
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
                match observer_command(&panel) {Ok(command)=>command,Err(error)=>{panel.notice=error;panel.watching=false;world.entity_mut(entity).insert(panel);draw(world,entity);continue;}}
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
                clear_private_actor(&mut panel);
                panel.person = person;
            }
            panel.handover = None;
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

fn position_text(view: Option<&(View, Stamp)>) -> String {
    let Some((view, received)) = view else {
        return "Location stopped".into();
    };
    if view
        .remaining_millis
        .is_some_and(|remaining| remaining <= received.elapsed().as_millis() as u64)
    {
        return "Location expired".into();
    }
    let age = view
        .age_ms
        .unwrap_or_default()
        .saturating_add(received.elapsed().as_millis() as u64);
    let ending = view.remaining_millis.map_or(String::new(), |remaining| {
        format!(
            " · {} min left",
            remaining
                .saturating_sub(received.elapsed().as_millis() as u64)
                .saturating_add(59_999)
                / 60_000
        )
    });
    if age >= nucleus::location::MAX_FIX_AGE_MS as u64 && view.fix.is_some() {
        return "Location unavailable · last fix expired".into();
    }
    if let Some(fix) = &view.fix {
        if let Some(radius) = view.radius_metres {
            return format!(
                "Approximate area · radius {radius} m · {} s old{ending}",
                age / 1000
            );
        }
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
    fn linking_and_selecting_saved_addresses_send_identities_without_live_acquisition() {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        world.init_resource::<Messages<SavedLocationRequest>>();
        let parent = world.spawn_empty().id();
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world, parent, &record, None, None);
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.fields.clear();
            state.address = "@street4".into();
        }
        apply(&mut world, panel, Intent::BindAddress(String::new()));
        let mut cursor = world
            .resource::<Messages<SavedLocationRequest>>()
            .get_cursor();
        let requests: Vec<_> = cursor
            .read(world.resource::<Messages<SavedLocationRequest>>())
            .cloned()
            .collect();
        assert_eq!(requests.len(), 1);
        assert!(
            matches!(&requests[0].command, nucleus::vocabulary::SavedLocationCommand::Bind { record: uid, address } if uid == &record && address == "@street4")
        );
        let assertion = nucleus::new_uid("a");
        let candidate = nucleus::vocabulary::LocationCandidate {
            assertion_uid: Some(assertion.clone()),
            record_uid: nucleus::new_uid("r"),
            head: "Street4".into(),
            slug: Some("street4".into()),
            latitude: 1.0,
            longitude: 2.0,
            radius_metres: None,
        };
        let location = nucleus::vocabulary::SavedLocation {
            record_uid: record.clone(),
            candidates: vec![candidate.clone()],
            selected: Some(candidate),
            explicit: true,
            unavailable: false,
            revision: 4,
        };
        assert!(receive(
            &mut world,
            &requests[0].id,
            Ok(serde_json::to_value(location).unwrap())
        ));
        apply(
            &mut world,
            panel,
            Intent::SelectSaved(Some(assertion.clone())),
        );
        let next: Vec<_> = cursor
            .read(world.resource::<Messages<SavedLocationRequest>>())
            .cloned()
            .collect();
        assert!(
            matches!(&next[0].command, nucleus::vocabulary::SavedLocationCommand::Select { assertion: Some(uid), expected_revision: 4, .. } if uid == &assertion)
        );
        assert!(world.resource::<Messages<Request>>().is_empty());
        assert!(world.get::<Panel>(panel).unwrap().settings.is_none());
    }

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
            grant: None,
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
                capture_age_ms: 0,
            }),
            age_ms: Some(0),
            remaining_millis: Some(60_000),
            next_sequence: Some(2),
            radius_metres: None,
        }
    }

    #[test]
    fn approximate_live_location_reports_area_size_without_point_or_device_accuracy() {
        let mut area = view();
        area.radius_metres = Some(2500);
        let text = position_text(Some(&(area, Stamp::now())));
        assert!(text.contains("radius 2500 m"));
        assert!(!text.contains("-23.5505"));
        assert!(!text.contains("-46.6333"));
        assert!(!text.contains('±'));
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
            state.view = Some((view(), Stamp::now()));
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
            state.view = Some((view(), Stamp::now() - Duration::from_secs(61)));
        }
        apply(&mut world, panel, Intent::SavePlace);
        assert!(world.resource::<Messages<SavePlaceRequest>>().is_empty());
        world.get_mut::<Panel>(panel).unwrap().view = Some((view(), Stamp::now()));
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
        let old = (view(), Stamp::now() - Duration::from_secs(61));
        let text = position_text(Some(&old));
        assert!(!text.contains("23.5505"));
        assert!(text.contains("expired"));
    }

    #[test]
    fn display_uses_shared_elapsed_clock_instead_of_authority_or_capture_wall_time() {
        use std::sync::atomic::{AtomicU64,Ordering};
        static TIME: AtomicU64 = AtomicU64::new(1_000);
        let mut world = World::new();
        world.insert_resource(ElapsedClock(||TIME.load(Ordering::SeqCst)));
        let stamp = Stamp::for_world(&world);
        let mut current = view();
        current.expires_at_ms = Some(chrono::Utc::now().timestamp_millis() - 86_400_000);
        current.fix.as_mut().unwrap().captured_at_ms += 86_400_000;
        current.remaining_millis = Some(3_600_000);
        assert!(position_text(Some(&(current.clone(),stamp))).contains("23.5505"));
        TIME.store(91_000,Ordering::SeqCst);
        assert!(!position_text(Some(&(current,stamp))).contains("23.5505"));
    }

    pub(super) fn handover_panel() -> (World,Entity,View,Choice) {
        let mut world = World::new();
        world.insert_resource(Typography(Handle::default()));
        world.init_resource::<Messages<Request>>();
        let parent = world.spawn_empty().id();
        let person = nucleus::new_uid("r");
        let record = nucleus::new_uid("r");
        let panel = mount(&mut world,parent,&record,Some(&person),None);
        let current = Choice {uid:nucleus::new_uid("r"),label:"Desktop".into(),node_id:Some("a".repeat(64))};
        let replacement = Choice {uid:nucleus::new_uid("r"),label:"Phone".into(),node_id:Some("b".repeat(64))};
        let settings = Settings {record_uid:record.clone(),controller_uid:person,source_cell_uid:current.uid.clone(),source_node_id:"a".repeat(64),source_kind:SourceKind::Device,duration_seconds:DEFAULT_DURATION_SECONDS,recipients:vec![],transfer_uid:None};
        let mut active = view();
        active.record_uid = record;
        let mut state = world.get_mut::<Panel>(panel).unwrap();
        state.context = Some(Context {authority_node_id:"a".repeat(64),settings:Some(settings.clone()),people:vec![],devices:vec![current.clone(),replacement.clone()],current_cell_uid:current.uid,current_node_id:"a".repeat(64),view:active.clone()});
        state.settings = Some(settings);
        state.view = Some((active.clone(),Stamp::now()));
        state.fields.clear();
        drop(state);
        (world,panel,active,replacement)
    }

    #[test]
    fn handover_requires_an_unchanged_review_and_never_queues_source_approval() {
        let (mut world,panel,active,replacement) = handover_panel();
        apply(&mut world,panel,Intent::Device(replacement.clone()));
        apply(&mut world,panel,Intent::ReviewHandover);
        assert!(world.resource::<Messages<Request>>().is_empty());
        {
            let mut state = world.get_mut::<Panel>(panel).unwrap();
            state.minutes = "120".into();
            state.fields.clear();
        }
        apply(&mut world,panel,Intent::ConfirmHandover);
        assert!(world.resource::<Messages<Request>>().is_empty());
        assert!(world.get::<Panel>(panel).unwrap().handover.is_none());
        apply(&mut world,panel,Intent::ReviewHandover);
        apply(&mut world,panel,Intent::ConfirmHandover);
        let requests = world.resource_mut::<Messages<Request>>().drain().collect::<Vec<_>>();
        assert_eq!(requests.len(),1);
        assert!(matches!(&requests[0].command,Command::Handover {settings,expected_session_uid} if settings.source_cell_uid == replacement.uid && settings.duration_seconds == 7200 && Some(expected_session_uid) == active.session_uid.as_ref()));
        let state = world.get::<Panel>(panel).unwrap();
        assert!(state.view.is_none());
        assert_eq!(state.pending.as_ref().unwrap().then.len(),1);
        assert!(matches!(state.pending.as_ref().unwrap().then[0],Command::Context {..}));
    }

    #[test]
    fn handover_replies_cannot_restore_a_changed_private_draft_or_viewer() {
        for viewer_changed in [false,true] {
            let (mut world,panel,mut reply,replacement) = handover_panel();
            apply(&mut world,panel,Intent::Device(replacement));
            apply(&mut world,panel,Intent::ReviewHandover);
            apply(&mut world,panel,Intent::ConfirmHandover);
            let request = world.resource_mut::<Messages<Request>>().drain().next().unwrap();
            if viewer_changed {actor_changed(&mut world,&nucleus::new_uid("r"));} else {
                let mut state = world.get_mut::<Panel>(panel).unwrap();
                state.minutes = "121".into();
                state.fields.clear();
            }
            reply.session_uid = Some(nucleus::new_uid("location"));
            reply.status = Status::AwaitingApproval;
            reply.fix = None;
            let handled = receive(&mut world,&request.id,Ok(serde_json::to_value(reply).unwrap()));
            assert_eq!(handled,!viewer_changed);
            let state = world.get::<Panel>(panel).unwrap();
            assert!(state.view.is_none());
            assert!(state.pending.is_none());
            assert!(state.handover.is_none());
            assert!(world.resource::<Messages<Request>>().is_empty());
            if viewer_changed {assert!(state.settings.is_none());assert!(state.context.is_none());}
        }
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
            state.view = Some((current.clone(), Stamp::now()));
            state.pending = Some(Pending {
                id: "old-view".into(),
                command: Command::View {
                    person: state.person.clone(),
                    record_uid: state.record.clone(),
                },
                then: Vec::new(),
                sent: Stamp::now(),
                save_place: false,
                scope: None,
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
