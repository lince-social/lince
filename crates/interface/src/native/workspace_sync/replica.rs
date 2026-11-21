// non-lean-yet
// Scope: struct Snapshot, fn canvas_item, fn install, enum Restore.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 62, 311, 322,
//   355.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   115.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 315.

use super::*;
use serde::Deserialize;
use std::ops::ControlFlow;

struct Snapshot<'a> {
    revision: i64,
    name: &'a str,
    permitted: bool,
    editable: bool,
    reviewable: bool,
    policy: Option<&'a Value>,
    policy_unavailable: bool,
    participants: usize,
}

pub(super) fn canvas_item(geometry: &Geometry) -> crate::native::canvas::CanvasItem {
    crate::native::canvas::CanvasItem {
        position: DVec2::from_array(geometry.position),
        size: Vec2::new(geometry.size[0] as f32, geometry.size[1] as f32),
    }
}

pub(super) fn install(world: &mut World, owner: Entity, value: &Value) {
    let layout = {
        let _decode = engine::performance::Span::new("native-workspace-snapshot-decoding");
        engine::performance::add("native-workspace-snapshots-decoded", 1);
        Layout::deserialize(&value["layout"])
    };
    let Ok(layout) = layout else {
        status(world, owner, "Unsupported workspace snapshot");
        return;
    };
    let Some(revision) = value["revision"].as_i64() else {
        return;
    };
    apply(
        world,
        owner,
        Arc::new(layout),
        Snapshot {
            revision,
            name: value["name"].as_str().unwrap_or("Workspace"),
            permitted: value["separate_view"] == true,
            editable: value["can_edit"] == true,
            reviewable: value["can_review"] == true,
            policy: value.get("policy").filter(|policy| !policy.is_null()),
            policy_unavailable: value["policy_unavailable"] == true,
            participants: value["participants"].as_array().map_or(0, Vec::len),
        },
    );
}

pub(super) enum Restore {
    Current,
    Viewer,
}

pub(super) fn restore(world: &mut World, owner: Entity, mode: Restore) {
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let layout = view.layout.clone();
    let current = matches!(mode, Restore::Current);
    let snapshot = Snapshot {
        revision: view.revision,
        name: "Workspace",
        permitted: current && view.permitted,
        editable: current && view.editable,
        reviewable: current && view.reviewable,
        policy: None,
        policy_unavailable: false,
        participants: 0,
    };
    apply(world, owner, layout, snapshot);
}

struct ItemPlan {
    entity: Option<Entity>,
    reuse_content: bool,
}

struct Reconciliation<'a> {
    ids: HashSet<&'a str>,
    removed: Vec<Entity>,
    items: Vec<ItemPlan>,
}

impl<'a> Reconciliation<'a> {
    fn new(view: &WorkspaceSync, layout: &'a Arc<Layout>) -> Self {
        let _prepare = engine::performance::Span::new("native-workspace-replica-preparation");
        let shared = Arc::ptr_eq(&view.layout, layout);
        let ids: HashSet<_> = layout
            .elements
            .iter()
            .map(|element| element.id.as_str())
            .collect();
        let removed = view
            .replicas
            .iter()
            .filter(|(id, _)| !ids.contains(id.as_str()))
            .map(|(_, entity)| *entity)
            .collect();
        let mut previous: Vec<_> = if shared {
            Vec::new()
        } else {
            view.layout.elements.iter().collect()
        };
        previous.sort_unstable_by(|left, right| left.id.cmp(&right.id));
        let items = layout
            .elements
            .iter()
            .map(|element| {
                let reuse_content = shared || {
                    let first = previous.partition_point(|old| old.id < element.id);
                    let same_presentation = previous[first..]
                        .iter()
                        .take_while(|old| old.id == element.id)
                        .any(|old| {
                            old.component == element.component && {
                                !matches!(element.component, LayoutComponent::Composition { .. })
                                    || old.geometry.size == element.geometry.size
                            }
                        });
                    same_presentation
                        && view.layout.disabled_controls.contains(&element.id)
                            == layout.disabled_controls.contains(&element.id)
                };
                ItemPlan {
                    entity: view.replicas.get(&element.id).copied(),
                    reuse_content,
                }
            })
            .collect();
        engine::performance::add(
            "native-workspace-replica-items-prepared",
            layout.elements.len() as u64,
        );
        Self {
            ids,
            removed,
            items,
        }
    }
}

fn apply(world: &mut World, owner: Entity, layout: Arc<Layout>, snapshot: Snapshot<'_>) {
    let _application = engine::performance::Span::new("native-workspace-replica-application");
    let revision = snapshot.revision;
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let root = view.root;
    let sand = view.sand;
    let mut replica_workspace = view.replica_workspace;
    if replica_workspace.is_none() {
        if world.get_entity(sand).is_err() {
            return;
        }
        crate::native::workspace::create(world, root);
        replica_workspace = world
            .get::<crate::native::workspace::Workspaces>(root)
            .map(|spaces| spaces.active);
        if let Some(workspace) = replica_workspace {
            let Ok(mut sand) = world.get_entity_mut(sand) else {
                return;
            };
            sand.insert(crate::native::workspace::WorkspaceMember(workspace));
            if let Some(mut spaces) = world.get_mut::<crate::native::workspace::Workspaces>(root)
                && let Some(space) = spaces
                    .entries
                    .iter_mut()
                    .find(|space| space.id == workspace)
            {
                space.name = format!(
                    "{}: {}",
                    if snapshot.permitted {
                        "Permitted view"
                    } else {
                        "Shared"
                    },
                    snapshot.name
                );
            }
        }
    }
    let Some(workspace) = replica_workspace else {
        return;
    };
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let prepared = Reconciliation::new(view, &layout);
    for entity in &prepared.removed {
        if world.get_entity(*entity).is_ok() {
            world.entity_mut(*entity).despawn();
        }
    }
    let mut replacements = Vec::new();
    for (element, update) in layout.elements.iter().zip(&prepared.items) {
        let item = canvas_item(&element.geometry);
        let retained = update
            .entity
            .filter(|entity| world.get_entity(*entity).is_ok());
        let entity = if let Some(entity) = retained {
            if world.get::<crate::native::canvas::CanvasItem>(entity) != Some(&item) {
                world.entity_mut(entity).insert(item);
                engine::performance::add("native-workspace-replica-geometry-writes", 1);
            }
            entity
        } else {
            let entity = world
                .spawn((
                    crate::native::castle::Castle,
                    HostedReplica,
                    crate::native::sand::Square,
                    crate::native::sand::InBox(root),
                    ChildOf(root),
                    crate::native::workspace::WorkspaceMember(workspace),
                    item,
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(8)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    crate::native::token_style::background(crate::native::tokens::Token::Surface),
                ))
                .id();
            entity
        };
        let reuse_content = retained.is_some() && update.reuse_content;
        if !reuse_content {
            engine::performance::add("native-workspace-replica-content-rebuilds", 1);
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
        if !reuse_content
            || !matches!(
                element.component,
                LayoutComponent::Builtin {
                    state: ComponentState::Record { .. }
                }
            )
        {
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
                panel::status(world, field, text);
            }
        }
        if update.entity != Some(entity) {
            replacements.push((element.id.as_str(), entity));
        }
    }
    let (subscribe, selected) = {
        let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
            for (_, entity) in replacements {
                if let Ok(entity) = world.get_entity_mut(entity) {
                    entity.despawn();
                }
            }
            return;
        };
        view.replicas
            .retain(|id, _| prepared.ids.contains(id.as_str()));
        for (id, entity) in replacements {
            view.replicas.insert(id.to_owned(), entity);
        }
        drop(prepared);
        view.layout = layout;
        view.replica_workspace = Some(workspace);
        view.revision = revision;
        view.permitted = snapshot.permitted;
        view.editable = snapshot.editable;
        view.reviewable = snapshot.reviewable;
        let subscribe = !view.permitted && !view.subscribed;
        (
            subscribe,
            subscribe.then(|| view.selected.clone()).flatten(),
        )
    };
    world.init_resource::<ReplicaFeeds>();
    world.resource_scope(|world, mut feeds: Mut<ReplicaFeeds>| {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            return;
        };
        let (source, replicas) = feeds
            .0
            .entry(owner)
            .or_insert_with(|| (view.source.clone(), Vec::new()));
        if *source != view.source {
            source.clone_from(&view.source);
        }
        replicas.clear();
        replicas.extend(view.replicas.values().copied());
    });
    if let Some(policy) = snapshot.policy {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            return;
        };
        let field = view.policy;
        let old = view.policy_snapshot.as_ref();
        let changed = old != Some(policy);
        let snapshot_changed =
            old.is_none_or(|old| !nucleus::json::same_value_encoding(old, policy));
        if snapshot_changed {
            let replace_draft = changed
                && (old.is_none() || {
                    let expected = old.and_then(|value| serde_json::to_string_pretty(value).ok());
                    let current = world
                        .get::<EditableText>(field)
                        .filter(|text| !text.is_composing())
                        .map(EditableText::value);
                    match (current, expected.as_deref()) {
                        (Some(current), Some(expected)) => current == expected,
                        (None, None) => true,
                        _ => false,
                    }
                });
            let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
                return;
            };
            view.policy_snapshot = Some(policy.clone());
            if replace_draft && let Some(mut editor) = world.get_mut::<EditableText>(field) {
                *editor = crate::native::sand::editable(
                    &serde_json::to_string_pretty(policy).unwrap_or_default(),
                );
            }
        }
    }
    if snapshot.policy_unavailable {
        let Some(mut view) = world.get_mut::<WorkspaceSync>(owner) else {
            return;
        };
        let field = view.policy;
        view.policy_snapshot = None;
        if let Some(mut editor) = world.get_mut::<EditableText>(field) {
            *editor = draft_editor("");
        }
    }
    if subscribe && let Some(workspace) = selected {
        let message = ClientMessage::WorkspaceSubscribe {
            id: subscription(owner),
            workspace,
            client: Client::default(),
        };
        if send(world, owner, message).is_ok()
            && let Some(mut view) = world.get_mut::<WorkspaceSync>(owner)
        {
            view.subscribed = true;
        }
    }
    let Some(view) = world.get::<WorkspaceSync>(owner) else {
        return;
    };
    let workspace_status = view.workspace_status;
    let mut targets = Vec::new();
    for element in &view.layout.elements {
        let _: ControlFlow<()> = element.component.visit_records(&mut |uid| {
            targets.push(protein::Predicate::UidEq(uid.into()));
            ControlFlow::Continue(())
        });
    }
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
    if world.get::<Text>(workspace_status).is_some() {
        panel::status(
            world,
            workspace_status,
            format!(
                "{} · revision {revision} · {} connected participants. Camera and selection stay on this computer.",
                if snapshot.permitted {
                    "Separate permitted view; no shared editing"
                } else if snapshot.editable {
                    "Shared editor"
                } else {
                    "Shared viewer"
                },
                snapshot.participants
            ),
        );
    }
}
