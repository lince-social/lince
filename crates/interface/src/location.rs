//! Shared controls for live location, saved places, and private invitations.

mod actions;
mod clock;
#[cfg(test)]
mod invitation_tests;
mod render;
mod requests;
mod saved;
#[cfg(test)]
mod tests;

use std::time::Instant;

use bevy::prelude::*;
use nucleus::location::{Choice, Command, Context, DEFAULT_DURATION_SECONDS, Settings, View};

use crate::controls;

use actions::apply;
#[cfg(test)]
use actions::observer_command;
pub use clock::ElapsedClock;
use clock::Stamp;
use render::{draw, masks};
use requests::maintain;
pub use requests::{authentication_reply, receive, receive_ref};

/// A live-location command and the identity used to match its reply.
#[derive(Message, Clone)]
pub struct Request {
    pub owner: Entity,
    pub id: String,
    pub command: Command,
}

/// Saves a position through the Record's separate saved-place policy.
#[derive(Message, Clone)]
pub struct SavePlaceRequest {
    pub owner: Entity,
    pub id: String,
    pub record: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Looks up or edits a saved location independently of live sharing.
#[derive(Message, Clone)]
pub struct SavedLocationRequest {
    pub owner: Entity,
    pub id: String,
    pub command: nucleus::vocabulary::SavedLocationCommand,
}

/// Asks the host to copy a location reference for this panel.
#[derive(Message, Clone)]
pub struct CopyReferenceRequest {
    pub panel: Entity,
    pub reference: String,
}

/// Advertises that the host can handle copy-reference requests.
#[derive(Resource, Default)]
pub struct CopyReferencesAvailable;

/// Carries credentials for one login at the location authority.
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

struct DraftScope {
    person: String,
    record: String,
    node: String,
    settings: Option<Settings>,
    minutes: String,
    transfer: String,
    invitation_fields: [String; 4],
}

impl DraftScope {
    fn matches(&self, panel: &Panel) -> bool {
        self.person == panel.person
            && self.record == panel.record
            && self.node == panel.node
            && self.settings == panel.settings
            && self.minutes == panel.minutes
            && self.transfer == panel.transfer
            && self.invitation_fields.each_ref()
                == [
                    &panel.invite_recipient,
                    &panel.invite_minutes,
                    &panel.invite_radius,
                    &panel.invite_conversation,
                ]
    }

    fn of(panel: &Panel) -> Self {
        Self {
            person: panel.person.clone(),
            record: panel.record.clone(),
            node: panel.node.clone(),
            settings: panel.settings.clone(),
            minutes: panel.minutes.clone(),
            transfer: panel.transfer.clone(),
            invitation_fields: [
                panel.invite_recipient.clone(),
                panel.invite_minutes.clone(),
                panel.invite_radius.clone(),
                panel.invite_conversation.clone(),
            ],
        }
    }
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

/// Loads a reference into an observer panel. Viewing requires a separate action.
///
/// # Errors
/// Returns a message for an invalid reference or an unavailable observer panel.
pub fn load_reference(world: &mut World, entity: Entity, reference: &str) -> Result<(), String> {
    nucleus::location::Invitation::parse(reference).map_err(str::to_owned)?;
    let mut panel = world
        .get_mut::<Panel>(entity)
        .ok_or("Location controls are unavailable")?;
    if !panel.observer {
        return Err("Open shared location controls for this reference".into());
    }
    panel.reference = reference.into();
    panel.fields.clear();
    apply(world, entity, Intent::LoadReference);
    Ok(())
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
    let panels = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .filter(|(_, panel)| panel.person != person)
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in panels {
        let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
            continue;
        };
        clear_private_actor(&mut panel);
        if panel.observer {
            panel.record.clear();
            panel.node.clear();
            panel.reference.clear();
            panel.invitation = None;
        }
        panel.person = person.into();
        panel.next_poll = Instant::now();
        world.entity_mut(entity).insert(panel);
        draw(world, entity);
    }
}

fn capture(world: &World, panel: &mut Panel) {
    for &(field, entity) in &panel.fields {
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
                    if panel.person != value {
                        clear_private_actor(panel);
                        panel.person = value;
                        break;
                    }
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
