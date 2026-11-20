pub(crate) mod checks;
mod editor;
pub(crate) mod policy;
mod publishing;
mod replica;
mod review;

use replica::install;

use crate::native::{actions::Action, protein_area::Source, sand_panel as panel};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use engine::workspace_sync::{Change, Client, Command as Backend, Element, Layout, Request};
use nucleus::{
    canvas::{Component as LayoutComponent, Geometry},
    component::ComponentState,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Component)]
struct WorkspaceSync {
    root: Entity,
    sand: Entity,
    source: Source,
    host: Entity,
    name: Entity,
    policy: Entity,
    change: Entity,
    username: Entity,
    password: Entity,
    password_mask: Entity,
    status: Entity,
    workspace_status: Entity,
    catalogue: Entity,
    history: Entity,
    selected: Option<String>,
    revision: i64,
    subscribed: bool,
    editable: bool,
    reviewable: bool,
    permitted: bool,
    layout: Arc<Layout>,
    replica_workspace: Option<u64>,
    replicas: HashMap<String, Entity>,
    pending: Option<(String, Kind)>,
    draft_base: Option<i64>,
    draft_workspace: Option<String>,
    draft_host: Option<Source>,
    draft_text: String,
    policy_snapshot: Option<Value>,
    delete_confirmation: bool,
    grant_index: Entity,
    quantity: Entity,
    assertion_add: Entity,
    assertion_remove: Entity,
    assignment_add: Entity,
    assignment_remove: Entity,
    audience_actors: Entity,
    audience_roles: Entity,
    required_capabilities: Entity,
}

#[derive(Clone, Copy)]
enum Kind {
    List,
    Create,
    Inspect,
    Change,
    DraftChange,
    History,
    Delete,
    Draft,
    Drafts,
    Preview,
    Import,
    Capabilities,
}

#[derive(Clone)]
enum Control {
    Connect,
    Login,
    Logout,
    Select(String),
    Permitted,
    Create,
    History,
    OlderHistory(String),
    Revise(Value),
    Submit,
    Policy,
    Delete,
    AddText,
    AddRecord,
    AddArea,
    Review(String, bool),
    ReadSelector,
    WriteSelector,
    Preview,
    PreviewPolicy,
    PreviewProposal(String),
    SaveDraft,
    Drafts,
    LoadDraft(Value),
    DiscardDraft(String),
    CreateRecord,
    DeleteRecord,
    ConfigureArea,
    ChangeRecord,
    Allow(protein::authority::Operation),
    Audience(&'static str, bool),
    RequiredCapabilities,
    AllowTags(bool),
}

#[derive(Resource, Default)]
struct ReplicaFeeds(HashMap<Entity, (Source, Vec<Entity>)>);

#[derive(Component)]
pub(crate) struct HostedReplica;

pub struct WorkspaceSyncPlugin;

#[cfg(test)]
#[path = "workspace_sync/tests.rs"]
mod tests;
impl Plugin for WorkspaceSyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReplicaFeeds>()
            .add_systems(
                Update,
                (
                    receive_local.after(crate::native::cell_bridge::ReceiveCell),
                    maintain,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                protect_passwords.before(bevy::text::EditableTextSystems),
            );
    }
}

fn status(world: &mut World, owner: Entity, text: &str) {
    if let Some(entity) = world.get::<WorkspaceSync>(owner).map(|view| view.status) {
        panel::status(world, entity, text);
    }
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::native::edit_mode::label(world, parent, text, 14.0)
}
fn subscription(owner: Entity) -> String {
    format!("workspace-sync-{}", owner.to_bits())
}
fn records_subscription(owner: Entity) -> String {
    format!("workspace-records-{}", owner.to_bits())
}

pub(crate) fn organs(world: &mut World) -> HashSet<String> {
    world
        .query::<&WorkspaceSync>()
        .iter(world)
        .filter_map(|view| match &view.source {
            Source::Organ(uid) => Some(uid.clone()),
            Source::Local => None,
        })
        .collect()
}

fn send(world: &mut World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner) {
        return Err("Workspace Sync is unavailable in the Laboratory".into());
    }
    let source = world
        .get::<WorkspaceSync>(owner)
        .ok_or("Workspace panel unavailable")?
        .source
        .clone();
    crate::native::protein_area::ensure_auxiliary(world, &source);
    if let (Source::Organ(organ), ClientMessage::Unsubscribe { id }) = (&source, &message) {
        crate::native::protein_area::auxiliary_unsubscribe(world, organ, id.clone());
        return Ok(());
    }
    let sender = crate::native::protein_area::auxiliary_sender(world, &source)
        .ok_or("Connect or sign in to this Organ first")?;
    sender
        .try_send(message.clone())
        .map_err(|error| error.to_string())?;
    crate::native::protein_area::auxiliary_subscribed(world, &source, &message);
    if matches!(
        message,
        ClientMessage::WorkspaceSubscribe { .. } | ClientMessage::Subscribe { .. }
    ) {
        world.init_resource::<ReplicaFeeds>();
        world
            .resource_mut::<ReplicaFeeds>()
            .0
            .entry(owner)
            .or_insert_with(|| (source.clone(), vec![]))
            .0 = source.clone();
    }
    Ok(())
}

fn act(world: &mut World, owner: Entity, command: Backend, kind: Kind) {
    if world
        .get::<WorkspaceSync>(owner)
        .is_none_or(|view| view.pending.is_some())
    {
        return;
    }
    let id = nucleus::new_uid("request");
    let message = ClientMessage::Act {
        id: id.clone(),
        action: engine::actions::Action::Workspace {
            request: Request {
                client: Client::default(),
                command,
            },
        },
    };
    match send(world, owner, message) {
        Ok(()) => {
            let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                return;
            };
            view.pending = Some((id, kind));
            status(world, owner, "Waiting for the host…");
        }
        Err(error) => status(
            world,
            owner,
            &format!("{error}. Draft text stays here until you submit it."),
        ),
    }
}

fn draft_editor(text: &str) -> EditableText {
    let mut editor = crate::native::sand::editable(text);
    editor.visible_lines = Some(5.0);
    editor.max_characters = Some(65536);
    editor
}

fn local_draft_request(world: &mut World, owner: Entity, command: Backend, kind: Kind) {
    let id = nucleus::new_uid("request");
    let result = panel::send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command,
                },
            },
        },
    );
    match result {
        Ok(()) => {
            if let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) {
                view.pending = Some((id, kind));
            }
        }
        Err(error) => status(world, owner, &error),
    }
}

fn selected(world: &World, owner: Entity) -> Option<(String, i64)> {
    world
        .get::<WorkspaceSync>(owner)
        .and_then(|view| Some((view.selected.clone()?, view.revision)))
}

fn first_record(component: &LayoutComponent) -> Option<&str> {
    component
        .visit_records(&mut |record| std::ops::ControlFlow::Break(record))
        .break_value()
}

fn selected_element(world: &World, owner: Entity) -> Option<&Element> {
    let view = world.get::<WorkspaceSync>(owner)?;
    let selected = &world
        .get::<crate::native::canvas_selection::SandSelection>(view.root)?
        .0;
    let entity = selected.first()?;
    if selected.len() != 1 {
        return None;
    }
    let id = view
        .replicas
        .iter()
        .find_map(|(id, replica)| (replica == entity).then_some(id))?;
    view.layout
        .elements
        .iter()
        .find(|element| &element.id == id)
}

fn unsubscribe(world: &mut World, owner: Entity) {
    let _ = send(
        world,
        owner,
        ClientMessage::Unsubscribe {
            id: subscription(owner),
        },
    );
    let _ = send(
        world,
        owner,
        ClientMessage::Unsubscribe {
            id: records_subscription(owner),
        },
    );
    if let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) {
        view.subscribed = false;
    }
}

fn restore_positions(world: &mut World, owner: Entity) {
    replica::restore(world, owner, replica::Restore::Current);
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            return;
        };
        if view.pending.is_some() {
            status(
                world,
                owner,
                "Wait for the host to acknowledge the current request.",
            );
            return;
        }
        match self {
            Self::Revise(change) => {
                let field = view.change;
                let raw = match serde_json::to_string_pretty(change) {
                    Ok(raw) => raw,
                    Err(error) => {
                        status(world, owner, &format!("Could not load proposal: {error}"));
                        return;
                    }
                };
                let Some(mut editor) = world.get_mut::<EditableText>(field) else {
                    status(world, owner, "The draft editor is unavailable.");
                    return;
                };
                *editor = draft_editor(&raw);
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.draft_base = Some(view.revision);
                view.draft_workspace = view.selected.clone();
                view.draft_host = Some(view.source.clone());
                view.draft_text = raw;
                status(
                    world,
                    owner,
                    "Loaded as a new draft against the current revision. Revise, preview, then submit it. The original proposal remains in history until reviewed.",
                );
            }
            Self::OlderHistory(before) => {
                if let Some((workspace, _)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::History {
                            workspace,
                            before: Some(before.clone()),
                        },
                        Kind::History,
                    );
                }
            }
            Self::Audience(..) | Self::RequiredCapabilities | Self::AllowTags(_) => {
                let field = view.policy;
                let mut policy = match panel::value(world, field).and_then(|raw| {
                    serde_json::from_str::<engine::workspace_sync::Policy>(&raw)
                        .map_err(|error| error.to_string())
                }) {
                    Ok(policy) => policy,
                    Err(error) => {
                        status(world, owner, &error);
                        return;
                    }
                };
                match self {
                    Self::Audience(audience, everyone) => {
                        let actors: std::collections::BTreeSet<_> =
                            panel::value(world, view.audience_actors)
                                .unwrap_or_default()
                                .split(',')
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned)
                                .collect();
                        let roles: Result<std::collections::BTreeSet<i64>, _> =
                            panel::value(world, view.audience_roles)
                                .unwrap_or_default()
                                .split(',')
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(str::parse)
                                .collect();
                        let Ok(roles) = roles else {
                            status(world, owner, "Use numeric Role IDs separated by commas.");
                            return;
                        };
                        if actors.iter().any(|actor| !nucleus::valid_uid(actor, "r"))
                            || roles.iter().any(|role| *role <= 0)
                        {
                            status(world, owner, "Use Person UIDs and positive Role IDs.");
                            return;
                        }
                        let chosen = engine::workspace_sync::Audience {
                            everyone: *everyone,
                            actors: if *everyone {
                                Default::default()
                            } else {
                                actors
                            },
                            roles: if *everyone { Default::default() } else { roles },
                        };
                        match *audience {
                            "viewers" => policy.viewers = chosen,
                            "editors" => policy.editors = chosen,
                            _ => policy.managers = chosen,
                        }
                    }
                    Self::RequiredCapabilities => {
                        policy.required_capabilities =
                            panel::value(world, view.required_capabilities)
                                .unwrap_or_default()
                                .split(',')
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned)
                                .collect();
                    }
                    Self::AllowTags(remove) => {
                        let index = panel::value(world, view.grant_index)
                            .ok()
                            .and_then(|value| value.parse::<usize>().ok())
                            .and_then(|index| index.checked_sub(1));
                        let Some(grant) =
                            index.and_then(|index| policy.ceiling.grants.get_mut(index))
                        else {
                            status(world, owner, "Choose an existing write grant number.");
                            return;
                        };
                        let input = if *remove {
                            view.assertion_remove
                        } else {
                            view.assertion_add
                        };
                        let concepts: Vec<_> = panel::value(world, input)
                            .unwrap_or_default()
                            .split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                            .collect();
                        if concepts.is_empty()
                            || concepts
                                .iter()
                                .any(|concept| !nucleus::valid_uid(concept, "c"))
                        {
                            status(world, owner, "Enter Concept UIDs in the assertion fields.");
                            return;
                        }
                        let rules = if *remove {
                            &mut grant.assertions_remove
                        } else {
                            &mut grant.assertions_add
                        };
                        if rules.len() + concepts.len() > 128 {
                            status(world, owner, "Keep each grant within 128 assertion rules.");
                            return;
                        }
                        for predicate_uid in concepts {
                            rules.push(protein::authority::AssertionGrant {
                                predicate_uid,
                                target: protein::authority::AssertionTarget::Unary,
                                role: protein::authority::AssertionRole::Ordinary,
                                properties: Default::default(),
                            });
                        }
                    }
                    _ => unreachable!(),
                }
                let raw = match serde_json::to_string_pretty(&policy) {
                    Ok(raw) => raw,
                    Err(error) => {
                        status(
                            world,
                            owner,
                            &format!("Could not write policy draft: {error}"),
                        );
                        return;
                    }
                };
                let Some(mut editor) = world.get_mut::<EditableText>(field) else {
                    status(world, owner, "The policy editor is unavailable.");
                    return;
                };
                *editor = draft_editor(&raw);
                status(
                    world,
                    owner,
                    "Workspace policy draft updated. Preview and propose it to apply these limits.",
                );
            }
            Self::SaveDraft => {
                let Some((workspace, revision)) = selected(world, owner) else {
                    status(world, owner, "Open a workspace before saving its draft.");
                    return;
                };
                if view.draft_workspace.is_some()
                    && (view.draft_workspace.as_deref() != Some(&workspace)
                        || view.draft_host.as_ref() != Some(&view.source))
                {
                    status(
                        world,
                        owner,
                        "Connect to the draft's original host and workspace before saving it.",
                    );
                    return;
                }
                let base_revision = view.draft_base.unwrap_or(revision);
                let host = match &view.source {
                    Source::Organ(host) => Some(host.clone()),
                    Source::Local => None,
                };
                let change = panel::value(world, view.change).and_then(|raw| {
                    serde_json::from_str::<Change>(&raw).map_err(|error| error.to_string())
                });
                match change {
                    Ok(change) => local_draft_request(
                        world,
                        owner,
                        Backend::SaveDraft {
                            uid: nucleus::new_uid("draft"),
                            host,
                            workspace,
                            base_revision,
                            change,
                        },
                        Kind::Draft,
                    ),
                    Err(error) => status(world, owner, &error),
                }
            }
            Self::Drafts => local_draft_request(world, owner, Backend::Drafts, Kind::Drafts),
            Self::DiscardDraft(uid) => local_draft_request(
                world,
                owner,
                Backend::DiscardDraft { uid: uid.clone() },
                Kind::Draft,
            ),
            Self::LoadDraft(draft) => {
                let host = view.host;
                let field = view.change;
                let text = match serde_json::to_string_pretty(&draft["change"]) {
                    Ok(text) => text,
                    Err(error) => {
                        status(world, owner, &format!("Could not load draft: {error}"));
                        return;
                    }
                };
                let Ok([mut host, mut field]) = world.get_entity_mut([host, field]) else {
                    status(world, owner, "The draft editor is unavailable.");
                    return;
                };
                let (Some(mut host), Some(mut editor)) = (
                    host.get_mut::<EditableText>(),
                    field.get_mut::<EditableText>(),
                ) else {
                    status(world, owner, "The draft editor is unavailable.");
                    return;
                };
                *host = crate::native::sand::editable(draft["host"].as_str().unwrap_or(""));
                *editor = draft_editor(&text);
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.draft_base = draft["base_revision"].as_i64();
                view.draft_workspace = draft["workspace"].as_str().map(str::to_owned);
                view.draft_host = Some(
                    draft["host"]
                        .as_str()
                        .map_or(Source::Local, |host| Source::Organ(host.into())),
                );
                view.draft_text = text;
                status(
                    world,
                    owner,
                    &format!(
                        "Draft restored at revision {}. Connect to its host and select {} before submitting.",
                        draft["base_revision"],
                        draft["workspace"].as_str().unwrap_or("?")
                    ),
                );
            }
            Self::Connect => {
                let field = view.host;
                unsubscribe(world, owner);
                let host = panel::value(world, field)
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if !host.is_empty() && !nucleus::valid_uid(&host, "r") {
                    status(
                        world,
                        owner,
                        "Choose a contact's Organ UID or leave the host empty for this Organ.",
                    );
                    return;
                }
                clear_replica(world, owner);
                let source = if host.is_empty() {
                    Source::Local
                } else {
                    Source::Organ(host)
                };
                if let Source::Organ(organ) = &source {
                    crate::native::protein_area::reconnect_auxiliary(world, organ);
                }
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.source = source;
                view.selected = None;
                view.policy_snapshot = None;
                view.delete_confirmation = false;
                view.subscribed = false;
                view.editable = false;
                view.reviewable = false;
                act(world, owner, Backend::List, Kind::List);
            }
            Self::Login => {
                let Source::Organ(organ) = &view.source else {
                    return;
                };
                let organ = organ.clone();
                let username = panel::value(world, view.username).unwrap_or_default();
                let password = panel::value(world, view.password).unwrap_or_default();
                let field = view.password;
                let result =
                    crate::native::protein_area::auxiliary_login(world, &organ, username, password);
                if let Some(mut editor) = world.get_mut::<EditableText>(field) {
                    *editor = crate::native::sand::editable("");
                }
                status(
                    world,
                    owner,
                    result.as_ref().err().map_or("Signing in…", String::as_str),
                );
            }
            Self::Logout => {
                if let Source::Organ(organ) = &view.source {
                    let organ = organ.clone();
                    crate::native::protein_area::logout_organ(world, &organ);
                }
                clear_replica(world, owner);
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.subscribed = false;
                view.editable = false;
                status(world, owner, "Signed out. Reconnect to sign in again.");
            }
            Self::Select(uid) => {
                unsubscribe(world, owner);
                clear_replica(world, owner);
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.selected = Some(uid.clone());
                view.policy_snapshot = None;
                view.delete_confirmation = false;
                view.permitted = false;
                view.subscribed = false;
                view.revision = 0;
                act(
                    world,
                    owner,
                    Backend::Inspect {
                        workspace: uid.clone(),
                        permitted_view: false,
                    },
                    Kind::Inspect,
                );
            }
            Self::Permitted => {
                unsubscribe(world, owner);
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.permitted = true;
                if let Some((workspace, _)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::Inspect {
                            workspace,
                            permitted_view: true,
                        },
                        Kind::Inspect,
                    );
                }
            }
            Self::Create | Self::Policy => {
                let name = panel::value(world, view.name).unwrap_or_default();
                let policy = panel::value(world, view.policy).and_then(|text| {
                    serde_json::from_str::<Value>(&text).map_err(|error| error.to_string())
                });
                match policy {
                    Ok(policy) if matches!(self, Self::Create) => {
                        act(world, owner, Backend::Create { name, policy }, Kind::Create)
                    }
                    Ok(policy) => propose(world, owner, Change::Policy { policy }),
                    Err(error) => status(world, owner, &error),
                }
            }
            Self::PreviewProposal(proposal) => {
                if let Some((workspace, _)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::PreviewProposal {
                            workspace,
                            proposal: proposal.clone(),
                        },
                        Kind::Preview,
                    );
                }
            }
            Self::PreviewPolicy => {
                let policy = panel::value(world, view.policy)
                    .ok()
                    .and_then(|raw| serde_json::from_str(&raw).ok());
                if let (Some((workspace, _)), Some(policy)) = (selected(world, owner), policy) {
                    act(
                        world,
                        owner,
                        Backend::PreviewPolicy { workspace, policy },
                        Kind::Preview,
                    );
                }
            }
            Self::Preview => {
                let change = panel::value(world, view.change)
                    .ok()
                    .and_then(|raw| serde_json::from_str(&raw).ok());
                if let (Some((workspace, _)), Some(change)) = (selected(world, owner), change) {
                    act(
                        world,
                        owner,
                        Backend::Preview { workspace, change },
                        Kind::Preview,
                    );
                }
            }
            Self::History => {
                if let Some((workspace, _)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::History {
                            workspace,
                            before: None,
                        },
                        Kind::History,
                    );
                }
            }
            Self::Submit => {
                let text = panel::value(world, view.change).unwrap_or_default();
                match serde_json::from_str::<Change>(&text) {
                    Ok(change) => {
                        if view.draft_workspace.is_some()
                            && (view.draft_workspace != view.selected
                                || view.draft_host.as_ref() != Some(&view.source))
                        {
                            status(
                                world,
                                owner,
                                "Connect to the draft's original host and workspace before submitting it.",
                            );
                            return;
                        }
                        if !can_propose(view, &change) {
                            status(
                                world,
                                owner,
                                "Reconnect with the authority required by this draft before submitting it.",
                            );
                            return;
                        }
                        if let Some((workspace, current)) = selected(world, owner) {
                            let base_revision = view.draft_base.unwrap_or(current);
                            act(
                                world,
                                owner,
                                Backend::Propose {
                                    workspace,
                                    request_id: nucleus::new_uid("request"),
                                    base_revision,
                                    change,
                                },
                                Kind::DraftChange,
                            );
                        }
                    }
                    Err(error) => status(world, owner, &error.to_string()),
                }
            }
            Self::Delete => {
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    return;
                };
                view.delete_confirmation = !view.delete_confirmation;
                if view.delete_confirmation {
                    status(
                        world,
                        owner,
                        "Delete this hosted workspace and its review history? Click Delete hosted workspace again to confirm.",
                    );
                    return;
                }
                if let Some((workspace, expected_revision)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::Delete {
                            workspace,
                            expected_revision,
                        },
                        Kind::Delete,
                    );
                }
            }
            Self::CreateRecord => {
                let text = panel::value(world, view.name).unwrap_or_default();
                propose(
                    world,
                    owner,
                    Change::CreateRecord {
                        draft: engine::record_creation::Draft {
                            head: text,
                            ..Default::default()
                        },
                        placement: nucleus::new_uid("placement"),
                        geometry: Geometry {
                            position: [200.0, 0.0],
                            size: [300.0, 100.0],
                        },
                    },
                );
            }
            Self::DeleteRecord => {
                let record = selected_element(world, owner)
                    .and_then(|element| first_record(&element.component))
                    .map(str::to_owned)
                    .unwrap_or_else(|| panel::value(world, view.name).unwrap_or_default());
                propose(
                    world,
                    owner,
                    Change::DeleteRecord {
                        record: record.trim().into(),
                    },
                );
            }
            Self::Allow(operation) => {
                let field = view.policy;
                let Ok(raw) = panel::value(world, field) else {
                    return;
                };
                let Ok(mut policy) = serde_json::from_str::<engine::workspace_sync::Policy>(&raw)
                else {
                    return;
                };
                use protein::authority::{MutationGrant, Operation, Property};
                let properties = match operation {
                    Operation::Create | Operation::Restore => std::collections::BTreeSet::from([
                        Property::Kind,
                        Property::Organ,
                        Property::Head,
                        Property::Body,
                        Property::Quantity,
                        Property::Slug,
                        Property::Unit,
                        Property::Place,
                    ]),
                    Operation::Update => std::collections::BTreeSet::from([Property::Quantity]),
                    Operation::Delete => std::collections::BTreeSet::from([Property::Slug]),
                };
                policy.ceiling.grants.push(MutationGrant {
                    operation: *operation,
                    selector: protein::Predicate::KindEq("plain".into()),
                    properties,
                    assertions_add: Vec::new(),
                    assertions_remove: Vec::new(),
                });
                let raw = match serde_json::to_string_pretty(&policy) {
                    Ok(raw) => raw,
                    Err(error) => {
                        status(
                            world,
                            owner,
                            &format!("Could not write policy draft: {error}"),
                        );
                        return;
                    }
                };
                let Some(mut editor) = world.get_mut::<EditableText>(field) else {
                    status(world, owner, "The policy editor is unavailable.");
                    return;
                };
                *editor = crate::native::sand::editable(&raw);
                status(
                    world,
                    owner,
                    "Grant added to the policy draft. Use Protein to restrict its selector, then preview and propose the policy.",
                );
            }
            Self::ConfigureArea | Self::ChangeRecord => {
                let quantity = panel::value(world, view.quantity).unwrap_or_default();
                let tokens = |entity| {
                    panel::value(world, entity)
                        .unwrap_or_default()
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                };
                let changes = engine::area_transition::RecordChanges {
                    quantity: (!quantity.trim().is_empty()).then(|| quantity.trim().into()),
                    assert: tokens(view.assertion_add),
                    retract: tokens(view.assertion_remove),
                    assign: tokens(view.assignment_add),
                    unassign: tokens(view.assignment_remove),
                };
                let Some(element) = selected_element(world, owner) else {
                    status(
                        world,
                        owner,
                        "Select one shared Area or Record on the canvas first.",
                    );
                    return;
                };
                let change = if matches!(self, Self::ConfigureArea)
                    && matches!(
                        element.component,
                        LayoutComponent::Builtin {
                            state: ComponentState::Area { .. }
                        }
                    ) {
                    Change::Area {
                        element: element.id.clone(),
                        changes,
                    }
                } else if matches!(self, Self::ChangeRecord) {
                    let Some(record) = first_record(&element.component).map(str::to_owned) else {
                        status(world, owner, "Select a Record.");
                        return;
                    };
                    Change::ChangeRecord { record, changes }
                } else {
                    status(world, owner, "Select an Area.");
                    return;
                };
                propose(world, owner, change);
            }
            Self::AddText | Self::AddRecord | Self::AddArea => {
                let reference = panel::value(world, view.name).unwrap_or_default();
                let component = match self {
                    Self::AddRecord if nucleus::valid_uid(reference.trim(), "r") => {
                        ComponentState::Record {
                            record: reference.trim().into(),
                            mode: Default::default(),
                            start_call: None,
                        }
                    }
                    Self::AddRecord => {
                        status(
                            world,
                            owner,
                            "Enter an existing Record UID in the name / Record field.",
                        );
                        return;
                    }
                    Self::AddArea => ComponentState::Area {
                        immunity: Default::default(),
                        strength: 0,
                    },
                    _ => ComponentState::Text { text: reference },
                };
                let position = world
                    .get::<crate::native::canvas::CanvasView>(view.root)
                    .map_or([0.0; 2], |camera| camera.center.to_array());
                propose(
                    world,
                    owner,
                    Change::Add {
                        element: Element {
                            id: nucleus::new_uid("placement"),
                            component: LayoutComponent::Builtin { state: component },
                            geometry: Geometry {
                                position,
                                size: [360.0, 240.0],
                            },
                        },
                    },
                );
            }
            Self::Review(proposal, approve) => {
                if let Some((workspace, expected_revision)) = selected(world, owner) {
                    act(
                        world,
                        owner,
                        Backend::Review {
                            workspace,
                            proposal: proposal.clone(),
                            request_id: nucleus::new_uid("request"),
                            expected_revision,
                            approve: *approve,
                        },
                        Kind::Change,
                    );
                }
            }
            Self::ReadSelector => open_selector(world, owner, None),
            Self::WriteSelector => {
                let index = panel::value(world, view.grant_index)
                    .ok()
                    .and_then(|value| value.parse::<usize>().ok())
                    .and_then(|index| index.checked_sub(1));
                if let Some(index) = index {
                    open_selector(world, owner, Some(index));
                }
            }
        }
    }
}

fn can_propose(view: &WorkspaceSync, change: &Change) -> bool {
    !view.permitted
        && if matches!(change, Change::Policy { .. }) {
            view.reviewable
        } else {
            view.editable
        }
}

fn propose(world: &mut World, owner: Entity, change: Change) {
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    if !can_propose(view, &change) {
        status(
            world,
            owner,
            "This view cannot change the hosted workspace.",
        );
        return;
    }
    if let Some((workspace, base_revision)) = selected(world, owner) {
        act(
            world,
            owner,
            Backend::Propose {
                workspace,
                request_id: nucleus::new_uid("request"),
                base_revision,
                change,
            },
            Kind::Change,
        );
    }
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    let body = panel::column(world, sand);
    label(
        world,
        body,
        "Workspace Sync · one host, shared topology, personal camera",
    );
    let host = panel::field(world, body, "Host Organ UID (empty = this Organ)", "");
    panel::button(
        world,
        body,
        body,
        "Connect / reload workspaces",
        Control::Connect,
    );
    let username = panel::field(world, body, "Remote username, if requested", "");
    let password = panel::field(world, body, "Remote password", "");
    world
        .entity_mut(password)
        .remove::<(
            crate::native::token_style::TextToken,
            crate::native::sand_text::SandText,
        )>()
        .insert(TextColor(Color::NONE));
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(password) {
        node.set_role(accesskit::Role::PasswordInput);
    }
    let password_mask = world
        .spawn((
            Text::new(""),
            world
                .resource::<crate::native::theme::Typography>()
                .text(14.0),
            crate::native::token_style::text(crate::native::tokens::Token::Ink),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(6),
                top: px(6),
                ..default()
            },
            ChildOf(password),
        ))
        .id();
    panel::button(world, body, body, "Sign in", Control::Login);
    panel::button(world, body, body, "Sign out", Control::Logout);
    let status = label(world, body, "Connect to list hosted workspaces.");
    let workspace_status = label(world, body, "No shared workspace selected.");
    let catalogue = panel::column(world, body);
    panel::button(
        world,
        body,
        body,
        "Open a separate permitted view",
        Control::Permitted,
    );
    let name = panel::field(
        world,
        body,
        "Workspace name, Text content or Record UID",
        "",
    );
    let policy = panel::field(
        world,
        body,
        "Workspace policy",
        &json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}).to_string(),
    );
    let mut editor = world
        .get_mut::<EditableText>(policy)
        .expect("panel::field attaches an editable policy field");
    editor.allow_newlines = true;
    editor.visible_lines = Some(5.0);
    editor.max_characters = Some(65536);
    panel::button(
        world,
        body,
        body,
        "Edit read ceiling with Protein",
        Control::ReadSelector,
    );
    let grant_index = panel::field(
        world,
        body,
        "Write grant number for Protein selector editing",
        "1",
    );
    panel::button(
        world,
        body,
        body,
        "Edit write grant selector with Protein",
        Control::WriteSelector,
    );
    panel::button(
        world,
        body,
        body,
        "Preview policy admission and suspended Areas",
        Control::PreviewPolicy,
    );
    let audience_actors = panel::field(world, body, "Audience Person UIDs (comma separated)", "");
    let audience_roles = panel::field(world, body, "Audience Role IDs (comma separated)", "");
    for (key, title) in [
        ("viewers", "view"),
        ("editors", "edit"),
        ("managers", "manage"),
    ] {
        let controls = panel::row(world, body);
        panel::button(
            world,
            controls,
            body,
            &format!("Selected Actors/Roles may {title}"),
            Control::Audience(key, false),
        );
        panel::button(
            world,
            controls,
            body,
            &format!("Anyone with capabilities may {title}"),
            Control::Audience(key, true),
        );
    }
    let required_capabilities =
        panel::field(world, body, "Required capabilities (comma separated)", "");
    panel::button(
        world,
        body,
        body,
        "Set required capabilities in policy draft",
        Control::RequiredCapabilities,
    );
    let grants = panel::row(world, body);
    panel::button(
        world,
        grants,
        body,
        "Add quantity grant",
        Control::Allow(protein::authority::Operation::Update),
    );
    panel::button(
        world,
        grants,
        body,
        "Add creation grant",
        Control::Allow(protein::authority::Operation::Create),
    );
    panel::button(
        world,
        grants,
        body,
        "Add deletion grant",
        Control::Allow(protein::authority::Operation::Delete),
    );
    panel::button(
        world,
        grants,
        body,
        "Add restoration grant",
        Control::Allow(protein::authority::Operation::Restore),
    );
    panel::button(
        world,
        body,
        body,
        "Create hosted workspace",
        Control::Create,
    );
    panel::button(world, body, body, "Propose policy change", Control::Policy);
    let controls = panel::row(world, body);
    panel::button(world, controls, body, "Add Text", Control::AddText);
    panel::button(world, controls, body, "Add Record", Control::AddRecord);
    panel::button(world, controls, body, "Add Area", Control::AddArea);
    panel::button(
        world,
        controls,
        body,
        "Propose new Record",
        Control::CreateRecord,
    );
    panel::button(
        world,
        controls,
        body,
        "Propose Record deletion",
        Control::DeleteRecord,
    );
    let quantity = panel::field(
        world,
        body,
        "Quantity operation · examples +=1, -=1, =5",
        "",
    );
    let assertion_add = panel::field(
        world,
        body,
        "Assert concepts · comma separated names or UIDs",
        "",
    );
    let assertion_remove = panel::field(world, body, "Retract concepts", "");
    let assignment_add = panel::field(world, body, "Assign People · comma separated UIDs", "");
    let assignment_remove = panel::field(world, body, "Unassign People", "");
    panel::button(
        world,
        body,
        body,
        "Propose selected Area behavior",
        Control::ConfigureArea,
    );
    panel::button(
        world,
        body,
        body,
        "Propose changes to selected Record",
        Control::ChangeRecord,
    );
    let change = panel::field(
        world,
        body,
        "Workspace operation / offline draft",
        "{\"operation\":\"rename\",\"name\":\"Workspace\"}",
    );
    let mut editor = world
        .get_mut::<EditableText>(change)
        .expect("panel::field attaches an editable draft field");
    editor.allow_newlines = true;
    editor.visible_lines = Some(5.0);
    editor.max_characters = Some(65536);
    panel::button(
        world,
        body,
        body,
        "Save draft on this computer",
        Control::SaveDraft,
    );
    panel::button(world, body, body, "Recover local drafts", Control::Drafts);
    label(
        world,
        body,
        "Move shared elements directly. Area recipes and conflicting changes wait for review. Area and Remove operations use the placement IDs shown on the canvas.",
    );
    panel::button(world, body, body, "Submit operation", Control::Submit);
    panel::button(
        world,
        body,
        body,
        "Preview operation and Record consequences",
        Control::Preview,
    );
    panel::button(
        world,
        body,
        body,
        "Review changes and history",
        Control::History,
    );
    panel::button(
        world,
        body,
        body,
        "Delete hosted workspace",
        Control::Delete,
    );
    panel::button(
        world,
        body,
        body,
        "Allow assertion additions in selected ceiling grant",
        Control::AllowTags(false),
    );
    panel::button(
        world,
        body,
        body,
        "Allow assertion removals in selected ceiling grant",
        Control::AllowTags(true),
    );
    policy::mount(world, body, body, policy, true);
    editor::mount(world, body);
    publishing::mount(world, body, root, sand);
    let history = panel::column(world, body);
    world.entity_mut(body).insert(WorkspaceSync {
        root,
        sand,
        source: Source::Local,
        host,
        name,
        policy,
        change,
        username,
        password,
        password_mask,
        status,
        workspace_status,
        catalogue,
        history,
        selected: None,
        revision: 0,
        subscribed: false,
        editable: false,
        reviewable: false,
        permitted: false,
        layout: Default::default(),
        replica_workspace: None,
        replicas: Default::default(),
        pending: None,
        draft_base: None,
        draft_workspace: None,
        draft_host: None,
        draft_text: "{\"operation\":\"rename\",\"name\":\"Workspace\"}".into(),
        policy_snapshot: None,
        delete_confirmation: false,
        grant_index,
        quantity,
        assertion_add,
        assertion_remove,
        assignment_add,
        assignment_remove,
        audience_actors,
        audience_roles,
        required_capabilities,
    });
    body
}

pub(crate) fn open_organ(world: &mut World, root: Entity, organ: &str) {
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |camera| camera.center);
    let sand = crate::native::sand_store::spawn_sand(
        world,
        root,
        workspace,
        crate::native::sand_store::SandKind::Sync,
        "",
        position,
    );
    let owners: Vec<_> = world
        .query::<(Entity, &WorkspaceSync)>()
        .iter(world)
        .filter(|(_, view)| view.sand == sand)
        .map(|(owner, view)| (owner, view.host))
        .collect();
    for (owner, field) in owners {
        if let Some(mut editor) = world.get_mut::<EditableText>(field) {
            *editor = crate::native::sand::editable(organ);
            Control::Connect.apply(world, owner);
        }
    }
}

fn receive_local(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::native::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages.iter().map(|message| message.as_ref()) {
        receive(world, &Source::Local, &message);
    }
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    let owners: Vec<_> = world
        .query::<(Entity, &WorkspaceSync)>()
        .iter(world)
        .filter(|(_, view)| {
            &view.source == source
                || *source == Source::Local
                    && view
                        .pending
                        .as_ref()
                        .is_some_and(|(_, kind)| matches!(kind, Kind::Draft | Kind::Drafts))
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            continue;
        };
        match message {
            ServerMessage::Workspace { id, workspace }
                if id == &subscription(owner)
                    && !view.permitted
                    && workspace["hosted_workspace"].as_str() == view.selected.as_deref() =>
            {
                install(world, owner, workspace)
            }
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == &records_subscription(owner) =>
            {
                record_labels(world, owner, rows)
            }
            ServerMessage::ActionOk {
                id, created, data, ..
            } => {
                let Some((_, kind)) = view.pending.as_ref().filter(|(pending, _)| pending == id)
                else {
                    continue;
                };
                let kind = *kind;
                let catalogue = view.catalogue;
                let history_panel = view.history;
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    continue;
                };
                view.pending = None;
                status(world, owner, "Request completed.");
                match kind {
                    Kind::Preview => {
                        review::preview(world, owner, data.as_ref().unwrap_or(&Value::Null))
                    }
                    Kind::Import => {
                        publishing::report(world, owner, data.as_ref().unwrap_or(&Value::Null))
                    }
                    Kind::Capabilities => publishing::capabilities(
                        world,
                        owner,
                        data.as_ref().unwrap_or(&Value::Null),
                    ),
                    Kind::Draft => status(
                        world,
                        owner,
                        if data
                            .as_ref()
                            .is_some_and(|data| data["state"] == "draft_discarded")
                        {
                            "Draft discarded."
                        } else {
                            "Draft saved on this computer. It has no effects until the host accepts it."
                        },
                    ),
                    Kind::Drafts => {
                        let parent = history_panel;
                        panel::clear(world, parent);
                        for draft in data
                            .as_ref()
                            .and_then(|data| data["drafts"].as_array())
                            .into_iter()
                            .flatten()
                        {
                            panel::button(
                                world,
                                parent,
                                owner,
                                &format!(
                                    "{} · revision {}",
                                    draft["workspace"].as_str().unwrap_or("?"),
                                    draft["base_revision"]
                                ),
                                Control::LoadDraft(draft.clone()),
                            );
                            if let Some(uid) = draft["uid"].as_str() {
                                panel::button(
                                    world,
                                    parent,
                                    owner,
                                    "Discard this draft",
                                    Control::DiscardDraft(uid.into()),
                                );
                            }
                        }
                    }
                    Kind::List => {
                        let parent = catalogue;
                        panel::clear(world, parent);
                        for row in data
                            .as_ref()
                            .and_then(|data| data["workspaces"].as_array())
                            .into_iter()
                            .flatten()
                        {
                            if let Some(uid) = row["uid"].as_str() {
                                panel::button(
                                    world,
                                    parent,
                                    owner,
                                    row["name"].as_str().unwrap_or(uid),
                                    Control::Select(uid.into()),
                                );
                            }
                        }
                    }
                    Kind::Create => {
                        if let Some(uid) = created {
                            Control::Select(uid.clone()).apply(world, owner);
                        }
                    }
                    Kind::Inspect => {
                        if let Some(snapshot) = data {
                            install(world, owner, snapshot);
                        }
                    }
                    Kind::History => history(world, owner, data.as_ref().unwrap_or(&Value::Null)),
                    Kind::Delete => {
                        clear_replica(world, owner);
                        if let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) {
                            view.selected = None;
                        }
                    }
                    Kind::Change | Kind::DraftChange => {
                        if matches!(kind, Kind::DraftChange)
                            && let Some(mut view) = world.get_mut::<WorkspaceSync>(owner)
                        {
                            view.draft_base = None;
                            view.draft_workspace = None;
                            view.draft_host = None;
                        }
                        status(
                            world,
                            owner,
                            &format!(
                                "Host saved: {}. Pending changes need review.",
                                data.as_ref()
                                    .map_or("?", |data| data["state"].as_str().unwrap_or("?"))
                            ),
                        );
                    }
                }
            }
            ServerMessage::Error { id, message, .. }
                if id == "connection"
                    || id == &subscription(owner)
                    || view
                        .pending
                        .as_ref()
                        .is_some_and(|(pending, _)| pending == id) =>
            {
                let clear = id == "connection" || id == &subscription(owner);
                if clear {
                    let parent = view.history;
                    let policy = view.policy;
                    let password = view.password;
                    clear_replica(world, owner);
                    panel::clear(world, parent);
                    for field in [policy, password] {
                        if let Some(mut editor) = world.get_mut::<EditableText>(field) {
                            *editor = crate::native::sand::editable("");
                        }
                    }
                } else {
                    restore_positions(world, owner);
                }
                let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                    continue;
                };
                view.pending = None;
                if clear {
                    view.subscribed = false;
                    view.editable = false;
                    view.reviewable = false;
                }
                status(world, owner, message);
            }
            _ => {}
        }
    }
}

fn clear_replica(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let entities: Vec<_> = view.replicas.values().copied().collect();
    let workspace_status = view.workspace_status;
    panel::status(world, workspace_status, "No shared workspace connected.");
    for entity in entities {
        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).despawn();
        }
    }
    if let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) {
        view.replicas.clear();
    }
}

fn record_labels(world: &mut World, owner: Entity, rows: &[Value]) {
    editor::records(world, owner, rows);
    let _presentation = engine::performance::Span::new("native-workspace-replica-labels");
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let mut indexed = None;
    let changes: Vec<_> = view
        .layout
        .elements
        .iter()
        .filter_map(|element| {
            let entity = *view.replicas.get(&element.id)?;
            let field = *world.get::<Children>(entity)?.get(1)?;
            let previous = world.get::<Text>(field)?;
            let uid = first_record(&element.component)?;
            let by_uid = indexed.get_or_insert_with(|| {
                let mut by_uid = HashMap::new();
                for row in rows {
                    if let Some(uid) = row["uid"].as_str() {
                        by_uid.entry(uid).or_insert(row);
                    }
                }
                by_uid
            });
            let row = by_uid.get(uid)?;
            let parts = [
                row["head"].as_str().unwrap_or(""),
                "\n",
                row["body"].as_str().unwrap_or(""),
                "\nQuantity: ",
                row["quantity"].as_str().unwrap_or("?"),
            ];
            let unchanged = parts
                .iter()
                .try_fold(previous.0.as_str(), |rest, part| rest.strip_prefix(*part))
                .is_some_and(str::is_empty);
            (!unchanged).then(|| (field, parts.concat()))
        })
        .collect();
    engine::performance::add(
        "native-workspace-replica-labels-updated",
        changes.len() as u64,
    );
    for (field, text) in changes {
        if let Some(mut value) = world.get_mut::<Text>(field) {
            value.0 = text;
        }
    }
}

fn history(world: &mut World, owner: Entity, data: &Value) {
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let parent = view.history;
    let reviewable = view.reviewable;
    panel::clear(world, parent);
    for change in data["changes"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{} · {} · base {} · applied {}",
                change["status"].as_str().unwrap_or("?"),
                change["actor"].as_str().unwrap_or("local owner"),
                change["base_revision"],
                change["revision"]
            ),
        );
        review::change(world, parent, &change["change"]);
        panel::button(
            world,
            parent,
            owner,
            "Revise and resubmit as my change",
            Control::Revise(change["change"].clone()),
        );
        if change["status"] == "pending"
            && reviewable
            && let Some(uid) = change["uid"].as_str()
        {
            let controls = panel::row(world, parent);
            panel::button(
                world,
                controls,
                owner,
                "Preview as original Actor",
                Control::PreviewProposal(uid.into()),
            );
            panel::button(
                world,
                controls,
                owner,
                "Approve against current revision",
                Control::Review(uid.into(), true),
            );
            panel::button(
                world,
                controls,
                owner,
                "Reject",
                Control::Review(uid.into(), false),
            );
        }
    }
    if let Some(cursor) = data["next_cursor"].as_str() {
        panel::button(
            world,
            parent,
            owner,
            "Older changes",
            Control::OlderHistory(cursor.into()),
        );
    }
    panel::button(world, parent, owner, "Latest changes", Control::History);
}

fn protect_passwords(views: Query<&WorkspaceSync>, mut fields: Query<&mut EditableText>) {
    for view in &views {
        if let Ok(field) = fields.get_mut(view.password) {
            crate::controls::protect_password(field);
        }
    }
}

fn maintain(
    world: &mut World,
    mut owners: Local<Option<bevy::ecs::query::QueryState<Entity, With<WorkspaceSync>>>>,
) {
    let _maintenance = engine::performance::Span::new("native-workspace-maintenance");
    world.init_resource::<ReplicaFeeds>();
    let stale: Vec<_> = world
        .resource::<ReplicaFeeds>()
        .0
        .keys()
        .copied()
        .filter(|owner| world.get::<WorkspaceSync>(*owner).is_none())
        .collect();
    for owner in stale {
        let Some((source, replicas)) = world.resource_mut::<ReplicaFeeds>().0.remove(&owner) else {
            continue;
        };
        let mut done = true;
        for id in [subscription(owner), records_subscription(owner)] {
            match &source {
                Source::Organ(organ) => {
                    crate::native::protein_area::auxiliary_unsubscribe(world, organ, id)
                }
                Source::Local => {
                    if let Some(sender) =
                        crate::native::protein_area::auxiliary_sender(world, &source)
                    {
                        if matches!(
                            sender.try_send(ClientMessage::Unsubscribe { id }),
                            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
                        ) {
                            done = false;
                        }
                    }
                }
            }
        }
        for replica in replicas {
            if let Ok(entity) = world.get_entity_mut(replica) {
                entity.despawn();
            }
        }
        if !done {
            world
                .resource_mut::<ReplicaFeeds>()
                .0
                .insert(owner, (source, Vec::new()));
        }
    }
    let owners: Vec<_> = owners
        .get_or_insert_with(|| world.query_filtered::<Entity, With<WorkspaceSync>>())
        .iter(world)
        .collect();
    for owner in owners {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            continue;
        };
        let password = view.password;
        let mask = view.password_mask;
        let count = world
            .get::<EditableText>(password)
            .map_or(0, |text| text.value().chars().take(32).count());
        let mask_text = "••••••••••••••••••••••••••••••••";
        panel::status(world, mask, &mask_text[..count * '•'.len_utf8()]);
        if let Some(field) = world.get_mut::<EditableText>(password) {
            crate::controls::protect_password(field);
        }
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            continue;
        };
        let source = view.source.clone();
        let draft = {
            let value = world
                .get::<EditableText>(view.change)
                .filter(|text| !text.is_composing())
                .map(EditableText::value);
            match value {
                Some(value) if value != view.draft_text.as_str() => Some(value.to_string()),
                None if !view.draft_text.is_empty() => Some(String::new()),
                _ => None,
            }
        };
        if let Some(draft) = draft {
            engine::performance::add("native-workspace-drafts-prepared", 1);
            let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                continue;
            };
            view.draft_text = draft;
            if view.draft_base.is_none() && view.revision > 0 {
                view.draft_base = Some(view.revision);
                view.draft_workspace = view.selected.clone();
                view.draft_host = Some(view.source.clone());
            }
        }
        crate::native::protein_area::ensure_auxiliary(world, &source);
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            continue;
        };
        if view.pending.is_some() || view.permitted || !view.subscribed {
            continue;
        }
        let change = view.layout.elements.iter().find_map(|element| {
            let entity = view.replicas.get(&element.id)?;
            let Some(item) = world.get::<crate::native::canvas::CanvasItem>(*entity) else {
                return Some(Change::Remove {
                    element: element.id.clone(),
                });
            };
            let expected = replica::canvas_item(&element.geometry);
            if item.position != expected.position {
                Some(Change::Move {
                    element: element.id.clone(),
                    position: item.position.to_array(),
                })
            } else if item.size != expected.size {
                Some(Change::Resize {
                    element: element.id.clone(),
                    size: item.size.as_dvec2().to_array(),
                })
            } else {
                None
            }
        });
        if let Some(change) = change {
            engine::performance::add("native-workspace-replica-differences", 1);
            if view.editable {
                propose(world, owner, change);
            } else {
                replica::restore(world, owner, replica::Restore::Viewer);
            }
        }
    }
}

#[derive(Clone)]
struct UseSelector(Entity, Option<usize>);
impl Action for UseSelector {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(castle) = world.get::<crate::native::protein_castle::ProteinCastle>(owner) else {
            return;
        };
        let Ok(query) = castle.draft.compile_shared() else {
            return;
        };
        if query.source != protein::Source::Record {
            return;
        }
        let Some(field) = world.get::<WorkspaceSync>(self.0).map(|view| view.policy) else {
            return;
        };
        let Some(mut policy) = panel::value(world, field)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            return;
        };
        let selector = match serde_json::to_value(protein::Predicate::All(query.filter.clone())) {
            Ok(selector) => selector,
            Err(error) => {
                status(world, self.0, &format!("Could not write selector: {error}"));
                return;
            }
        };
        if let Some(index) = self.1 {
            let Some(grant) = policy["ceiling"]["grants"]
                .as_array_mut()
                .and_then(|grants| grants.get_mut(index))
            else {
                return;
            };
            grant["selector"] = selector;
        } else {
            policy["ceiling"]["read"] = selector;
        }
        let raw = match serde_json::to_string_pretty(&policy) {
            Ok(raw) => raw,
            Err(error) => {
                status(
                    world,
                    self.0,
                    &format!("Could not write policy draft: {error}"),
                );
                return;
            }
        };
        let Some(mut editor) = world.get_mut::<EditableText>(field) else {
            status(world, self.0, "The policy editor is unavailable.");
            return;
        };
        *editor = crate::native::sand::editable(&raw);
        status(
            world,
            self.0,
            "Selector updated in the draft. Submit it for review to change the host.",
        );
    }
}

fn open_selector(world: &mut World, owner: Entity, grant: Option<usize>) {
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let root = view.root;
    let field = view.policy;
    let Some(policy) = panel::value(world, field)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return;
    };
    let Ok(selector) = serde_json::from_value::<protein::Predicate>(grant.map_or_else(
        || policy["ceiling"]["read"].clone(),
        |index| policy["ceiling"]["grants"][index]["selector"].clone(),
    )) else {
        return;
    };
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::native::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |camera| camera.center);
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![selector],
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let castle = crate::native::protein_castle::spawn(
        world,
        root,
        workspace,
        position,
        crate::native::protein_castle::ProteinDraft::from_protein(
            "Workspace read ceiling".into(),
            String::new(),
            query,
        ),
    );
    panel::button(
        world,
        castle,
        castle,
        "Use this selector",
        UseSelector(owner, grant),
    );
}
