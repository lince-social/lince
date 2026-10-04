mod editor;
pub(crate) mod policy;
mod publishing;
mod review;

use crate::{actions::Action, protein_area::Source, sand_panel as panel};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use engine::workspace_sync::{Change, Client, Command as Backend, Element, Layout, Request};
use nucleus::{
    canvas::{Component as LayoutComponent, Geometry},
    component::ComponentState,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

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
    layout: Layout,
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
                    receive_local.after(crate::cell_bridge::ReceiveCell),
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
    if let Some(entity) = world.get::<WorkspaceSync>(owner).map(|view| view.status)
        && let Some(mut label) = world.get_mut::<Text>(entity)
    {
        label.0 = text.into();
    }
}

fn label(world: &mut World, parent: Entity, text: &str) -> Entity {
    crate::edit_mode::label(world, parent, text, 14.0)
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
    if crate::laboratory::active(world) {
        return Err("Workspace Sync is unavailable in the Laboratory".into());
    }
    let source = world
        .get::<WorkspaceSync>(owner)
        .ok_or("Workspace panel unavailable")?
        .source
        .clone();
    crate::protein_area::ensure_auxiliary(world, &source);
    if let (Source::Organ(organ), ClientMessage::Unsubscribe { id }) = (&source, &message) {
        crate::protein_area::auxiliary_unsubscribe(world, organ, id.clone());
        return Ok(());
    }
    let sender = crate::protein_area::auxiliary_sender(world, &source)
        .ok_or("Connect or sign in to this Organ first")?;
    sender
        .try_send(message.clone())
        .map_err(|error| error.to_string())?;
    crate::protein_area::auxiliary_subscribed(world, &source, &message);
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
            world.get_mut::<WorkspaceSync>(owner).unwrap().pending = Some((id, kind));
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
    let mut editor = crate::sand::editable(text);
    editor.visible_lines = Some(5.0);
    editor.max_characters = Some(65536);
    editor
}

fn local_draft_request(world: &mut World, owner: Entity, command: Backend, kind: Kind) {
    let id = nucleus::new_uid("request");
    let result = panel::send(
        world,
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
        Ok(()) => world.get_mut::<WorkspaceSync>(owner).unwrap().pending = Some((id, kind)),
        Err(error) => status(world, owner, &error),
    }
}

fn selected(world: &World, owner: Entity) -> Option<(String, i64)> {
    world
        .get::<WorkspaceSync>(owner)
        .and_then(|view| Some((view.selected.clone()?, view.revision)))
}

fn selected_element(world: &World, owner: Entity) -> Option<Element> {
    let view = world.get::<WorkspaceSync>(owner)?;
    let selected = &world
        .get::<crate::canvas_selection::SandSelection>(view.root)?
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
        .cloned()
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
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let snapshot = json!({"revision":view.revision,"layout":view.layout,"separate_view":view.permitted,"can_edit":view.editable,"can_review":view.reviewable});
    install(world, owner, &snapshot);
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
                let raw = serde_json::to_string_pretty(change).unwrap();
                *world.get_mut::<EditableText>(field).unwrap() = draft_editor(&raw);
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
                *world.get_mut::<EditableText>(field).unwrap() =
                    draft_editor(&serde_json::to_string_pretty(&policy).unwrap());
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
                let view = world.get::<WorkspaceSync>(owner).unwrap();
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
                let view = world.get::<WorkspaceSync>(owner).unwrap();
                let host = view.host;
                let field = view.change;
                let text = serde_json::to_string_pretty(&draft["change"]).unwrap_or_default();
                *world.get_mut::<EditableText>(host).unwrap() =
                    crate::sand::editable(draft["host"].as_str().unwrap_or(""));
                *world.get_mut::<EditableText>(field).unwrap() = draft_editor(&text);
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
                unsubscribe(world, owner);
                let view = world.get::<WorkspaceSync>(owner).unwrap();
                let host = panel::value(world, view.host)
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
                    crate::protein_area::reconnect_auxiliary(world, organ);
                }
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
                    crate::protein_area::auxiliary_login(world, &organ, username, password);
                *world.get_mut::<EditableText>(field).unwrap() = crate::sand::editable("");
                status(
                    world,
                    owner,
                    result.as_ref().err().map_or("Signing in…", String::as_str),
                );
            }
            Self::Logout => {
                if let Source::Organ(organ) = &view.source {
                    let organ = organ.clone();
                    crate::protein_area::logout_organ(world, &organ);
                }
                clear_replica(world, owner);
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
                view.subscribed = false;
                view.editable = false;
                status(world, owner, "Signed out. Reconnect to sign in again.");
            }
            Self::Select(uid) => {
                unsubscribe(world, owner);
                clear_replica(world, owner);
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
                world.get_mut::<WorkspaceSync>(owner).unwrap().permitted = true;
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
                        let view = world.get::<WorkspaceSync>(owner).unwrap();
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
                        if !can_propose(&view, &change) {
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
                if !view.delete_confirmation {
                    world
                        .get_mut::<WorkspaceSync>(owner)
                        .unwrap()
                        .delete_confirmation = true;
                    status(
                        world,
                        owner,
                        "Delete this hosted workspace and its review history? Click Delete hosted workspace again to confirm.",
                    );
                    return;
                }
                world
                    .get_mut::<WorkspaceSync>(owner)
                    .unwrap()
                    .delete_confirmation = false;
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
                    .and_then(|element| {
                        element
                            .component
                            .records()
                            .first()
                            .map(|record| (*record).to_owned())
                    })
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
                let properties = match operation {
                    protein::authority::Operation::Create
                    | protein::authority::Operation::Restore => {
                        vec![
                            "kind", "organ", "head", "body", "quantity", "slug", "unit", "place",
                        ]
                    }
                    protein::authority::Operation::Update => vec!["quantity"],
                    protein::authority::Operation::Delete => vec!["slug"],
                };
                let grant = serde_json::from_value(json!({"operation":operation,"selector":{"kind_eq":"plain"},"properties":properties,"assertions_add":[],"assertions_remove":[]})).unwrap();
                policy.ceiling.grants.push(grant);
                *world.get_mut::<EditableText>(field).unwrap() =
                    crate::sand::editable(&serde_json::to_string_pretty(&policy).unwrap());
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
                        element: element.id,
                        changes,
                    }
                } else if matches!(self, Self::ChangeRecord) {
                    let Some(record) = element
                        .component
                        .records()
                        .first()
                        .map(|record| (*record).to_owned())
                    else {
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
                    .get::<crate::canvas::CanvasView>(view.root)
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
        .remove::<(crate::token_style::TextToken, crate::sand_text::SandText)>()
        .insert(TextColor(Color::NONE));
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(password) {
        node.set_role(accesskit::Role::PasswordInput);
    }
    let password_mask = world
        .spawn((
            Text::new(""),
            world.resource::<crate::theme::Typography>().text(14.0),
            crate::token_style::text(crate::tokens::Token::Ink),
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
    world
        .get_mut::<EditableText>(policy)
        .unwrap()
        .allow_newlines = true;
    world.get_mut::<EditableText>(policy).unwrap().visible_lines = Some(5.0);
    world
        .get_mut::<EditableText>(policy)
        .unwrap()
        .max_characters = Some(65536);
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
    world
        .get_mut::<EditableText>(change)
        .unwrap()
        .allow_newlines = true;
    world.get_mut::<EditableText>(change).unwrap().visible_lines = Some(5.0);
    world
        .get_mut::<EditableText>(change)
        .unwrap()
        .max_characters = Some(65536);
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
        .get::<crate::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |camera| camera.center);
    let sand = crate::sand_store::spawn_sand(
        world,
        root,
        workspace,
        crate::sand_store::SandKind::Sync,
        "",
        position,
    );
    let owners: Vec<_> = world
        .query::<(Entity, &WorkspaceSync)>()
        .iter(world)
        .filter(|(_, view)| view.sand == sand)
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        let field = world.get::<WorkspaceSync>(owner).unwrap().host;
        *world.get_mut::<EditableText>(field).unwrap() = crate::sand::editable(organ);
        Control::Connect.apply(world, owner);
    }
}

fn receive_local(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
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
        let view = world.get::<WorkspaceSync>(owner).unwrap();
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
            } if view
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id) =>
            {
                let kind = view.pending.as_ref().unwrap().1;
                let catalogue = view.catalogue;
                world.get_mut::<WorkspaceSync>(owner).unwrap().pending = None;
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
                        let parent = world.get::<WorkspaceSync>(owner).unwrap().history;
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
                        world.get_mut::<WorkspaceSync>(owner).unwrap().selected = None;
                    }
                    Kind::Change | Kind::DraftChange => {
                        if matches!(kind, Kind::DraftChange) {
                            let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
                    clear_replica(world, owner);
                    let view = world.get::<WorkspaceSync>(owner).unwrap();
                    let parent = view.history;
                    let policy = view.policy;
                    let password = view.password;
                    panel::clear(world, parent);
                    *world.get_mut::<EditableText>(policy).unwrap() = crate::sand::editable("");
                    *world.get_mut::<EditableText>(password).unwrap() = crate::sand::editable("");
                } else {
                    restore_positions(world, owner);
                }
                let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
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
    if let Some(mut text) = world.get_mut::<Text>(workspace_status) {
        text.0 = "No shared workspace connected.".into();
    }
    for entity in entities {
        if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).despawn();
        }
    }
    world
        .get_mut::<WorkspaceSync>(owner)
        .unwrap()
        .replicas
        .clear();
}

fn install(world: &mut World, owner: Entity, snapshot: &Value) {
    let Ok(layout) = serde_json::from_value::<Layout>(snapshot["layout"].clone()) else {
        status(world, owner, "Unsupported workspace snapshot");
        return;
    };
    let Some(revision) = snapshot["revision"].as_i64() else {
        return;
    };
    let view = world.get::<WorkspaceSync>(owner).unwrap();
    let root = view.root;
    let sand = view.sand;
    let mut replica_workspace = view.replica_workspace;
    if replica_workspace.is_none() {
        crate::workspace::create(world, root);
        replica_workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active);
        if let Some(workspace) = replica_workspace {
            world
                .entity_mut(sand)
                .insert(crate::workspace::WorkspaceMember(workspace));
            if let Some(mut spaces) = world.get_mut::<crate::workspace::Workspaces>(root)
                && let Some(space) = spaces
                    .entries
                    .iter_mut()
                    .find(|space| space.id == workspace)
            {
                space.name = format!(
                    "{}: {}",
                    if snapshot["separate_view"] == true {
                        "Permitted view"
                    } else {
                        "Shared"
                    },
                    snapshot["name"].as_str().unwrap_or("Workspace")
                );
            }
        }
    }
    let Some(workspace) = replica_workspace else {
        return;
    };
    let previous = world.get::<WorkspaceSync>(owner).unwrap().replicas.clone();
    let previous_layout = world.get::<WorkspaceSync>(owner).unwrap().layout.clone();
    for (id, entity) in &previous {
        if !layout.elements.iter().any(|element| &element.id == id)
            && world.get_entity(*entity).is_ok()
        {
            world.entity_mut(*entity).despawn();
        }
    }
    let mut replicas = HashMap::new();
    for element in &layout.elements {
        let item = crate::canvas::CanvasItem {
            position: DVec2::from_array(element.geometry.position),
            size: Vec2::new(
                element.geometry.size[0] as f32,
                element.geometry.size[1] as f32,
            ),
        };
        let entity = if let Some(entity) = previous
            .get(&element.id)
            .filter(|entity| world.get_entity(**entity).is_ok())
        {
            world.entity_mut(*entity).insert(item);
            *entity
        } else {
            let entity = world
                .spawn((
                    crate::castle::Castle,
                    HostedReplica,
                    crate::sand::Square,
                    crate::sand::InBox(root),
                    ChildOf(root),
                    crate::workspace::WorkspaceMember(workspace),
                    item,
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    crate::token_style::background(crate::tokens::Token::Surface),
                ))
                .id();
            label(world, entity, &element.id);
            label(world, entity, "");
            entity
        };
        let unchanged = previous_layout.elements.iter().any(|old| {
            old.id == element.id
                && old.component == element.component
                && previous_layout.disabled_controls.contains(&element.id)
                    == layout.disabled_controls.contains(&element.id)
        });
        if !unchanged {
            panel::clear(world, entity);
            label(world, entity, &element.id);
            label(world, entity, "");
            if matches!(
                element.component,
                LayoutComponent::Composition { .. }
                    | LayoutComponent::Builtin {
                        state: ComponentState::Button { .. }
                    }
            ) {
                editor::presentation(
                    world,
                    owner,
                    entity,
                    element,
                    layout.disabled_controls.contains(&element.id),
                );
            }
        }
        let same_record = previous_layout.elements.iter().any(|old| {
            old.id == element.id
                && old.component == element.component
                && matches!(
                    old.component,
                    LayoutComponent::Builtin {
                        state: ComponentState::Record { .. }
                    }
                )
        });
        if !same_record {
            let text = match &element.component {
                LayoutComponent::Builtin {
                    state: ComponentState::Text { text },
                } => text.as_str(),
                LayoutComponent::Builtin {
                    state: ComponentState::Record { record, .. },
                } => record.as_str(),
                _ if layout.disabled_areas.contains(&element.id) => {
                    "Area · behavior suspended until reviewed under the current policy"
                }
                LayoutComponent::Builtin {
                    state: ComponentState::Area { .. },
                } => "Area · approved effects run on the host",
                _ => "Shared composition / declared control",
            };
            if let Some(children) = world.get::<Children>(entity)
                && children.len() > 1
            {
                let field = children[1];
                if let Some(mut value) = world.get_mut::<Text>(field) {
                    value.0 = text.into();
                }
            }
        }
        replicas.insert(element.id.clone(), entity);
    }
    world.init_resource::<ReplicaFeeds>();
    let source = world.get::<WorkspaceSync>(owner).unwrap().source.clone();
    world
        .resource_mut::<ReplicaFeeds>()
        .0
        .insert(owner, (source, replicas.values().copied().collect()));
    let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
    view.layout = layout;
    view.replicas = replicas;
    view.replica_workspace = Some(workspace);
    view.revision = revision;
    view.permitted = snapshot["separate_view"] == true;
    view.editable = snapshot["can_edit"] == true;
    view.reviewable = snapshot["can_review"] == true;
    let subscribe = !view.permitted && !view.subscribed;
    let selected = view.selected.clone();
    if let Some(policy) = snapshot.get("policy").filter(|policy| !policy.is_null()) {
        let field = view.policy;
        let old = view.policy_snapshot.clone();
        view.policy_snapshot = Some(policy.clone());
        if old.as_ref() != Some(policy)
            && (old.is_none()
                || panel::value(world, field).ok().as_ref()
                    == old
                        .as_ref()
                        .and_then(|value| serde_json::to_string_pretty(value).ok())
                        .as_ref())
        {
            *world.get_mut::<EditableText>(field).unwrap() =
                crate::sand::editable(&serde_json::to_string_pretty(policy).unwrap_or_default());
        }
    }
    if snapshot["policy_unavailable"] == true {
        let field = world.get::<WorkspaceSync>(owner).unwrap().policy;
        *world.get_mut::<EditableText>(field).unwrap() = draft_editor("");
        world
            .get_mut::<WorkspaceSync>(owner)
            .unwrap()
            .policy_snapshot = None;
    }
    if subscribe && let Some(workspace) = selected {
        let message = ClientMessage::WorkspaceSubscribe {
            id: subscription(owner),
            workspace,
            client: Client::default(),
        };
        if send(world, owner, message).is_ok() {
            world.get_mut::<WorkspaceSync>(owner).unwrap().subscribed = true;
        }
    }
    let targets: Vec<_> = world
        .get::<WorkspaceSync>(owner)
        .unwrap()
        .layout
        .elements
        .iter()
        .flat_map(|element| {
            element
                .component
                .records()
                .into_iter()
                .map(|uid| protein::Predicate::UidEq(uid.into()))
        })
        .collect();
    if !targets.is_empty() {
        let protein = protein::Protein {
            source: protein::Source::Record,
            filter: vec![protein::Predicate::Any(targets)],
            fields: Some(vec![
                "uid".into(),
                "head".into(),
                "body".into(),
                "quantity".into(),
            ]),
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: Some(256),
        };
        let _ = send(
            world,
            owner,
            ClientMessage::Subscribe {
                id: records_subscription(owner),
                protein,
            },
        );
    }
    let workspace_status = world.get::<WorkspaceSync>(owner).unwrap().workspace_status;
    if let Some(mut text) = world.get_mut::<Text>(workspace_status) {
        text.0 = format!(
            "{} · revision {revision} · {} connected participants. Camera and selection stay on this computer.",
            if snapshot["separate_view"] == true {
                "Separate permitted view; no shared editing"
            } else if snapshot["can_edit"] == true {
                "Shared editor"
            } else {
                "Shared viewer"
            },
            snapshot["participants"].as_array().map_or(0, Vec::len)
        );
    }
}

fn record_labels(world: &mut World, owner: Entity, rows: &[Value]) {
    editor::records(world, owner, rows);
    let changes: Vec<_> = world
        .get::<WorkspaceSync>(owner)
        .into_iter()
        .flat_map(|view| {
            view.layout.elements.iter().filter_map(|element| {
                let references = element.component.records();
                let uid = references.first().copied()?;
                let row = rows.iter().find(|row| row["uid"] == uid)?;
                Some((
                    *view.replicas.get(&element.id)?,
                    format!(
                        "{}\n{}\nQuantity: {}",
                        row["head"].as_str().unwrap_or(""),
                        row["body"].as_str().unwrap_or(""),
                        row["quantity"].as_str().unwrap_or("?")
                    ),
                ))
            })
        })
        .collect();
    for (entity, text) in changes {
        if let Some(children) = world.get::<Children>(entity)
            && children.len() > 1
        {
            let label = children[1];
            if let Some(mut value) = world.get_mut::<Text>(label) {
                value.0 = text;
            }
        }
    }
}

fn history(world: &mut World, owner: Entity, data: &Value) {
    let view = world.get::<WorkspaceSync>(owner).unwrap();
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
        if let Ok(mut field) = fields.get_mut(view.password) {
            field.pending_edits.retain(|edit| {
                !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
            });
        }
    }
}

fn maintain(world: &mut World) {
    world.init_resource::<ReplicaFeeds>();
    let stale: Vec<_> = world
        .resource::<ReplicaFeeds>()
        .0
        .keys()
        .copied()
        .filter(|owner| world.get::<WorkspaceSync>(*owner).is_none())
        .collect();
    for owner in stale {
        let (source, replicas) = world.resource::<ReplicaFeeds>().0[&owner].clone();
        let mut done = true;
        for id in [subscription(owner), records_subscription(owner)] {
            match &source {
                Source::Organ(organ) => {
                    crate::protein_area::auxiliary_unsubscribe(world, organ, id)
                }
                Source::Local => {
                    if let Some(sender) = crate::protein_area::auxiliary_sender(world, &source) {
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
        if done {
            world.resource_mut::<ReplicaFeeds>().0.remove(&owner);
        } else {
            world
                .resource_mut::<ReplicaFeeds>()
                .0
                .get_mut(&owner)
                .unwrap()
                .1
                .clear();
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<WorkspaceSync>>()
        .iter(world)
        .collect();
    for owner in owners {
        let view = world.get::<WorkspaceSync>(owner).unwrap();
        let password = view.password;
        let mask = view.password_mask;
        let count = world
            .get::<EditableText>(password)
            .map_or(0, |text| text.value().chars().count());
        if let Some(mut text) = world.get_mut::<Text>(mask) {
            text.0 = "•".repeat(count.min(32));
        }
        if let Some(mut field) = world.get_mut::<EditableText>(password) {
            field.pending_edits.retain(|edit| {
                !matches!(edit, bevy::text::TextEdit::Copy | bevy::text::TextEdit::Cut)
            });
        }
        let view = world.get::<WorkspaceSync>(owner).unwrap();
        let source = view.source.clone();
        let draft = panel::value(world, view.change).unwrap_or_default();
        if draft != view.draft_text {
            let mut view = world.get_mut::<WorkspaceSync>(owner).unwrap();
            view.draft_text = draft;
            if view.draft_base.is_none() && view.revision > 0 {
                view.draft_base = Some(view.revision);
                view.draft_workspace = view.selected.clone();
                view.draft_host = Some(view.source.clone());
            }
        }
        crate::protein_area::ensure_auxiliary(world, &source);
        let view = world.get::<WorkspaceSync>(owner).unwrap();
        if view.pending.is_some() || view.permitted || !view.subscribed {
            continue;
        }
        let change = view.layout.elements.iter().find_map(|element| {
            let entity = view.replicas.get(&element.id)?;
            let Some(item) = world.get::<crate::canvas::CanvasItem>(*entity) else {
                return Some(Change::Remove {
                    element: element.id.clone(),
                });
            };
            if item.position.to_array() != element.geometry.position {
                Some(Change::Move {
                    element: element.id.clone(),
                    position: item.position.to_array(),
                })
            } else if item.size.as_dvec2().to_array() != element.geometry.size {
                Some(Change::Resize {
                    element: element.id.clone(),
                    size: item.size.as_dvec2().to_array(),
                })
            } else {
                None
            }
        });
        if let Some(change) = change {
            if view.editable {
                propose(world, owner, change);
            } else {
                let snapshot = json!({"revision":view.revision,"layout":view.layout,"separate_view":false,"can_edit":false,"can_review":false});
                install(world, owner, &snapshot);
            }
        }
    }
}

#[derive(Clone)]
struct UseSelector(Entity, Option<usize>);
impl Action for UseSelector {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(castle) = world.get::<crate::protein_castle::ProteinCastle>(owner) else {
            return;
        };
        let Ok(query) = castle.draft.compile() else {
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
        let selector = serde_json::to_value(protein::Predicate::All(query.filter)).unwrap();
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
        *world.get_mut::<EditableText>(field).unwrap() =
            crate::sand::editable(&serde_json::to_string_pretty(&policy).unwrap());
        status(
            world,
            self.0,
            "Selector updated in the draft. Submit it for review to change the host.",
        );
    }
}

fn open_selector(world: &mut World, owner: Entity, grant: Option<usize>) {
    let view = world.get::<WorkspaceSync>(owner).unwrap();
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
        .get::<crate::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::canvas::CanvasView>(root)
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
    let castle = crate::protein_castle::spawn(
        world,
        root,
        workspace,
        position,
        crate::protein_castle::ProteinDraft::from_protein(
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
