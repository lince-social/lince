use super::*;

#[derive(Component)]
struct Publisher {
    workspace: Entity,
    captured: Option<(String, Value, Value, Source)>,
}

#[derive(Clone)]
enum Publish {
    Validate,
    Accept,
    Capabilities,
}

impl Action for Publish {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<WorkspaceSync>(owner) else {
            return;
        };
        if matches!(self, Self::Capabilities) {
            act(world, owner, Backend::Capabilities, Kind::Capabilities);
            return;
        }
        let name = panel::value(world, view.name).unwrap_or_default();
        let policy = match panel::value(world, view.policy)
            .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(|error| error.to_string()))
        {
            Ok(policy) => policy,
            Err(error) => {
                status(world, owner, &error);
                return;
            }
        };
        let source = view.source.clone();
        if matches!(self, Self::Accept) {
            let Some((captured_name, captured_policy, layout, captured_source)) = world
                .get::<Publisher>(owner)
                .and_then(|publisher| publisher.captured.clone())
            else {
                status(world, owner, "Validate the local workspace first.");
                return;
            };
            if name != captured_name || policy != captured_policy || source != captured_source {
                status(
                    world,
                    owner,
                    "The host, name or policy changed. Validate again before publishing.",
                );
                return;
            }
            act(
                world,
                owner,
                Backend::Publish {
                    name,
                    policy,
                    layout,
                },
                Kind::Create,
            );
            return;
        }
        let root = view.root;
        let sand = view.sand;
        let field = world.get::<Publisher>(owner).unwrap().workspace;
        let workspace = panel::value(world, field)
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        let Some(workspace) = workspace else {
            status(world, owner, "Enter an existing local workspace number.");
            return;
        };
        if world
            .query::<&WorkspaceSync>()
            .iter(world)
            .any(|view| view.replica_workspace == Some(workspace))
        {
            status(
                world,
                owner,
                "Choose a local workspace. Shared replicas are already hosted.",
            );
            return;
        }
        let snapshot = match crate::canvas_host::capture(world, root) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                status(world, owner, &error);
                return;
            }
        };
        if !snapshot
            .workspaces
            .iter()
            .any(|space| space.id == workspace)
        {
            status(world, owner, "Local workspace unavailable.");
            return;
        }
        let excluded = world
            .get::<crate::canvas_host::Identity>(sand)
            .map(|identity| identity.0.clone());
        let areas = world
            .query::<(&crate::canvas_host::Identity, &crate::area::InfluenceArea)>()
            .iter(world)
            .map(|(identity, area)| (identity.0.clone(), area.clone()))
            .collect::<HashMap<_, _>>();
        let mut layout = Layout::default();
        for placement in snapshot.placements.into_iter().filter(|placement| {
            placement.workspace == workspace && Some(&placement.id) != excluded.as_ref()
        }) {
            let mut component = placement.component;
            if let Some(area) = areas.get(&placement.id) {
                if !area.changes.leave.is_empty()
                    || area.change_filter.is_some()
                    || !area.rules.is_empty()
                    || area.filter.is_some()
                    || area.protein.is_some()
                    || area.sound.is_some()
                    || area.sorting.is_some()
                    || area.strength != 0.0
                    || !matches!(area.shape, crate::area::AreaShape::Square)
                {
                    component = LayoutComponent::Native {
                        kind: "unsupported-area-behavior".into(),
                        settings: Default::default(),
                        bindings: vec![],
                    };
                } else if !area.changes.enter.is_empty() {
                    layout
                        .areas
                        .insert(placement.id.clone(), area.changes.enter.clone());
                    if !area.enabled || area.paused || !area.changes_enabled {
                        layout.disabled_areas.insert(placement.id.clone());
                    }
                }
            }
            if placement.group.is_some()
                || placement.metadata.pinned.is_some()
                || placement.metadata.attachment.is_some()
                || placement.metadata.group_pose.is_some()
                || !placement.metadata.event_boundary.is_empty()
                || placement.metadata.spatial != nucleus::canvas::Spatial::default()
            {
                component = LayoutComponent::Native {
                    kind: "unsupported-spatial-or-group-state".into(),
                    settings: Default::default(),
                    bindings: vec![],
                };
            }
            layout.elements.push(Element {
                id: placement.id,
                component,
                geometry: placement.geometry,
            });
        }
        let layout = serde_json::to_value(layout).unwrap();
        world.get_mut::<Publisher>(owner).unwrap().captured =
            Some((name.clone(), policy.clone(), layout.clone(), source));
        act(
            world,
            owner,
            Backend::ValidateImport {
                name,
                policy,
                layout,
            },
            Kind::Import,
        );
    }
}

pub(super) fn mount(world: &mut World, owner: Entity, root: Entity, _: Entity) {
    let active = world
        .get::<crate::workspace::Workspaces>(root)
        .map_or(0, |spaces| spaces.active);
    let workspace = panel::field(
        world,
        owner,
        "Local workspace number to publish",
        &active.to_string(),
    );
    world.entity_mut(owner).insert(Publisher {
        workspace,
        captured: None,
    });
    panel::button(
        world,
        owner,
        owner,
        "Show host components and action adapters",
        Publish::Capabilities,
    );
    panel::button(
        world,
        owner,
        owner,
        "Validate local workspace for publishing",
        Publish::Validate,
    );
    label(
        world,
        owner,
        "Publishing copies the supported topology and declared effects. Cameras, selection and local workspace stay on this computer. Unsupported native state must be removed or converted explicitly.",
    );
}

pub(super) fn report(world: &mut World, owner: Entity, data: &Value) {
    let parent = world.get::<WorkspaceSync>(owner).unwrap().history;
    panel::clear(world, parent);
    label(
        world,
        parent,
        &format!("Publishing preflight · {} elements", data["elements"]),
    );
    for rejected in data["rejected"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{}: {}",
                rejected["element"].as_str().unwrap_or("?"),
                rejected["reason"].as_str().unwrap_or("Unsupported")
            ),
        );
    }
    for key in ["layout_error", "policy_error"] {
        if let Some(message) = data[key].as_str() {
            label(world, parent, message);
        }
    }
    for converted in data["converted"].as_array().into_iter().flatten() {
        label(
            world,
            parent,
            &format!(
                "{}: {}",
                converted["element"].as_str().unwrap_or("?"),
                converted["notice"].as_str().unwrap_or_default()
            ),
        );
    }
    if data["ready"] == true {
        panel::button(
            world,
            parent,
            owner,
            "Publish this validated snapshot",
            Publish::Accept,
        );
    } else {
        label(
            world,
            parent,
            "Fix the rejected elements and validate again. No workspace was published.",
        );
    }
}

pub(super) fn capabilities(world: &mut World, owner: Entity, data: &Value) {
    let parent = world.get::<WorkspaceSync>(owner).unwrap().history;
    panel::clear(world, parent);
    label(
        world,
        parent,
        &format!(
            "Host components: {}",
            data["components"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    label(
        world,
        parent,
        &format!(
            "Declared actions: {}",
            data["actions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    for limitation in data["limitations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        label(world, parent, limitation);
    }
}
