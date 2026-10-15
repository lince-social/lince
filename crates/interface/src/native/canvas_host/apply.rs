use super::*;

pub(super) fn apply(
    world: &mut World,
    root: Entity,
    before: &api::Snapshot,
    after: &api::Snapshot,
) -> Result<(), String> {
    super::rooms::validate(world, root, before, after)?;
    if after.placements.iter().any(|placement| {
        placement.group.as_ref().is_some_and(|group| {
            group.len() != 32 || !group.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    }) {
        return Err("Invalid native group identity.".into());
    }
    let mut entities: HashMap<_, _> = world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            world
                .get::<Identity>(*entity)
                .map(|id| (id.0.clone(), *entity))
        })
        .collect();
    let before_map: HashMap<_, _> = before
        .placements
        .iter()
        .map(|placement| (placement.id.as_str(), placement))
        .collect();
    let after_ids: std::collections::HashSet<_> = after
        .placements
        .iter()
        .map(|placement| placement.id.as_str())
        .collect();
    let after_map: HashMap<_, _> = after
        .placements
        .iter()
        .map(|placement| (placement.id.as_str(), placement))
        .collect();
    for placement in &after.placements {
        if crate::native::topology::terrain::is_effect(&placement.component) {
            crate::native::topology::terrain::definition(&placement.component)?;
        }
        if let Some(old) = before_map.get(placement.id.as_str())
            && old.component != placement.component
            && authoring::compatible(&old.component, &placement.component)
            && let Some(entity) = entities.get(&placement.id)
        {
            authoring::validate(world, *entity, &placement.component)?;
        }
        if let Some(old) = before_map.get(placement.id.as_str())
            && old.component != placement.component
            && area::is_area(&old.component)
            && area::is_area(&placement.component)
            && let Some(entity) = entities.get(&placement.id)
        {
            let configured =
                area::validate(world, *entity, &placement.component, &placement.geometry)?;
            if world
                .get::<crate::native::area::InfluenceArea>(*entity)
                .is_some_and(|existing| existing.protein != configured.protein)
                && authoring::protected_draft(world, *entity)
            {
                return Err(
                    "Save the Record draft before changing its producer configuration.".into(),
                );
            }
            authoring::validate(world, *entity, &placement.component)?;
        }
    }
    for placement in &before.placements {
        if entities.get(&placement.id).is_some_and(|entity| {
            world
                .get::<crate::native::workspace_sync::HostedReplica>(*entity)
                .is_some()
        }) {
            let permitted = after_map.get(placement.id.as_str()).is_some_and(|next| {
                next.component == placement.component
                    && next.geometry == placement.geometry
                    && next.workspace == placement.workspace
                    && next.group == placement.group
                    && next.metadata == placement.metadata
            });
            if !permitted {
                return Err("Use Workspace Sync with the acting Actor to edit hosted placements. Local canvas automation cannot use another session's host authority.".into());
            }
        }
    }
    for placement in &before.placements {
        if after_map.get(placement.id.as_str()).is_some_and(|next| {
            next.component == placement.component
                || authoring::compatible(&placement.component, &next.component)
                || area::is_area(&placement.component) && area::is_area(&next.component)
                || crate::native::topology::terrain::is_effect(&placement.component)
                    && crate::native::topology::terrain::is_effect(&next.component)
        }) {
            continue;
        }
        if let Some(entity) = entities.get(&placement.id) {
            if crate::native::ide::protect(world, Some(*entity)) {
                return Err("Save or resolve the editor's file draft before replacing or removing its canvas placement.".into());
            }
            if authoring::protected_draft(world, *entity) {
                return Err(
                    "Save the Record draft before replacing or removing its canvas placement."
                        .into(),
                );
            }
            let mut pending = vec![*entity];
            while let Some(entity) = pending.pop() {
                if let (Some(record), Some(text)) = (
                    world.get::<crate::native::record_view::RecordEditor>(entity),
                    world.get::<bevy::text::EditableText>(entity),
                ) {
                    if record.pending.is_some()
                        || crate::native::record_view::pending_text(text)
                        || text.value().to_string() != record.confirmed
                    {
                        return Err("Save the Record draft before replacing or removing its canvas placement.".into());
                    }
                }
                if let Some(children) = world.get::<Children>(entity) {
                    pending.extend(children.iter());
                }
            }
        }
    }
    let mut added = Vec::new();
    for placement in &after.placements {
        let old = before_map.get(placement.id.as_str()).copied();
        let retained = old.is_none()
            && world.get::<Workspaces>(root).is_some_and(|spaces| {
                spaces
                    .trash
                    .iter()
                    .any(|entry| entry.workspace.id == placement.workspace)
            })
            && entities
                .get(&placement.id)
                .is_some_and(|entity| capture_content(world, *entity) == placement.component);
        if retained {
            continue;
        }
        if old.is_none_or(|old| {
            old.component != placement.component
                && !authoring::compatible(&old.component, &placement.component)
                && !(area::is_area(&old.component) && area::is_area(&placement.component))
                && !(crate::native::topology::terrain::is_effect(&old.component)
                    && crate::native::topology::terrain::is_effect(&placement.component))
        }) {
            match spawn(
                world,
                root,
                placement.workspace,
                DVec2::from_array(placement.geometry.position),
                &placement.component,
            ) {
                Ok(entity) => added.push((placement, entity)),
                Err(error) => {
                    for (_, entity) in added {
                        authoring::discard(world, entity);
                    }
                    return Err(error);
                }
            }
        }
    }
    if !added.is_empty() {
        for (placement, entity) in &added {
            if let Some(old) = entities.get(&placement.id) {
                let overrides = crate::native::token_style::overrides(world, *old);
                if !matches!(&placement.component, ContentKind::Native { settings, .. } if settings.contains_key("appearance"))
                {
                    crate::native::token_style::set_overrides(world, *entity, overrides);
                }
            }
        }
        let mut canonical = after.clone();
        let indices: HashMap<_, _> = canonical
            .placements
            .iter()
            .enumerate()
            .map(|(index, placement)| (placement.id.clone(), index))
            .collect();
        for (placement, entity) in &added {
            canonical.placements[indices[&placement.id]].component =
                capture_content(world, *entity);
        }
        if let Err(error) = api::State::new(canonical, registry()) {
            for (_, entity) in added {
                authoring::discard(world, entity);
            }
            return Err(error);
        }
    }
    for (placement, entity) in added {
        crate::native::topology::rooms::prepare_placement(world, entity);
        if let Some(old) = entities.insert(placement.id.clone(), entity) {
            crate::native::record_view::hide_placement(world, root, old);
            authoring::discard(world, old);
        }
        world
            .entity_mut(entity)
            .insert(Identity(placement.id.clone()));
    }
    for placement in &after.placements {
        if let Some(old) = before_map.get(placement.id.as_str())
            && old.component != placement.component
            && authoring::compatible(&old.component, &placement.component)
        {
            authoring::reconcile(world, entities[&placement.id], &placement.component)?;
        } else if let Some(old) = before_map.get(placement.id.as_str())
            && old.component != placement.component
            && area::is_area(&old.component)
            && area::is_area(&placement.component)
        {
            let entity = entities[&placement.id];
            let area = area::validate(world, entity, &placement.component, &placement.geometry)?;
            area::apply(world, entity, area, &placement.component);
            authoring::reconcile(world, entity, &placement.component)?;
        } else if let Some(old) = before_map.get(placement.id.as_str())
            && old.component != placement.component
            && crate::native::topology::terrain::is_effect(&old.component)
            && crate::native::topology::terrain::is_effect(&placement.component)
        {
            let entity = entities[&placement.id];
            let effect = crate::native::topology::terrain::definition(&placement.component)?;
            world
                .entity_mut(entity)
                .insert((
                    crate::native::topology::terrain::Effect(effect),
                    Content(placement.component.clone()),
                ))
                .remove::<crate::native::topology::terrain::Preview>();
        }
    }
    let removed_workspaces: std::collections::HashSet<_> = before
        .workspaces
        .iter()
        .filter(|space| !after.workspaces.iter().any(|next| next.id == space.id))
        .map(|space| space.id)
        .collect();
    let restored_workspaces: std::collections::HashSet<_> =
        after.workspaces.iter().map(|space| space.id).collect();
    let mut previous = world.get::<Workspaces>(root).unwrap().entries.clone();
    previous.extend(
        world
            .get::<Workspaces>(root)
            .unwrap()
            .trash
            .iter()
            .map(|entry| entry.workspace.clone()),
    );
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        spaces
            .trash
            .retain(|entry| !restored_workspaces.contains(&entry.workspace.id));
        for workspace in previous
            .iter()
            .filter(|space| removed_workspaces.contains(&space.id))
        {
            spaces
                .trash
                .push(crate::native::workspace::TrashedWorkspace {
                    workspace: workspace.clone(),
                    deleted_at: crate::native::workspace::unix_time(),
                });
        }
    }
    for old in &before.placements {
        if !after_ids.contains(old.id.as_str()) && !removed_workspaces.contains(&old.workspace) {
            if let Some(entity) = entities.remove(&old.id) {
                crate::native::record_view::hide_placement(world, root, entity);
                authoring::discard(world, entity);
            }
        }
    }
    let entries = after
        .workspaces
        .iter()
        .map(|space| {
            let mut entry = previous
                .iter()
                .find(|entry| entry.id == space.id)
                .cloned()
                .unwrap_or(crate::native::workspace::Workspace {
                    desktop: crate::layout::desktop::Workspace::new(
                        world
                            .get_resource::<crate::native::layout::desktop::DefaultLayout>()
                            .map_or(crate::host::WorkspaceLayout::Free, |mode| mode.0),
                    ),
                    id: space.id,
                    name: space.name.clone(),
                    center: space.center,
                    zoom: space.zoom,
                    topology: Default::default(),
                    colors: Default::default(),
                    color_overrides: [false; 2],
                });
            entry.name = space.name.clone();
            entry.center = space.center;
            entry.zoom = space.zoom;
            if let Some(view) = &space.view {
                entry.topology = crate::native::topology::view::View {
                    spatial: view.spatial,
                    position: view.position,
                    yaw: view.yaw as f32,
                    pitch: view.pitch as f32,
                    plane: view.plane,
                    selection_depth: view.selection_depth,
                };
            }
            entry
        })
        .collect();
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    spaces.entries = entries;
    spaces.active = after.active_workspace;
    let active = spaces
        .entries
        .iter()
        .find(|entry| entry.id == after.active_workspace)
        .unwrap()
        .clone();
    let trashed: std::collections::HashSet<_> = spaces
        .trash
        .iter()
        .map(|entry| entry.workspace.id)
        .collect();
    for saved in spaces.saved_records.values_mut() {
        if !trashed.contains(&saved.workspace)
            && !after
                .workspaces
                .iter()
                .any(|space| space.id == saved.workspace)
        {
            saved.workspace = after.active_workspace;
        }
    }
    let view = CanvasView {
        center: DVec2::from_array(active.center),
        zoom: active.zoom,
    };
    if let Some(mut current) = world.get_mut::<CanvasView>(root) {
        *current = view;
    } else {
        world.entity_mut(root).insert(view);
    }
    world.entity_mut(root).insert(active.topology);
    let mut selection = Vec::new();
    for placement in &after.placements {
        let entity = entities[&placement.id];
        if before_map.get(placement.id.as_str()).is_some_and(|before| {
            before.group != placement.group
                || before.geometry != placement.geometry
                || before.workspace != placement.workspace
        }) {
            crate::native::area_mutation::disarm(
                world,
                entity,
                "Canvas authoring changed placement or grouping; review area effects again.",
            );
        }
        world.entity_mut(entity).insert((
            WorkspaceMember(placement.workspace),
            CanvasItem {
                position: DVec2::from_array(placement.geometry.position),
                size: DVec2::from_array(placement.geometry.size).as_vec2(),
            },
        ));
        if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity) {
            area.center = placement.geometry.position;
            area.size = placement.geometry.size;
        }
        let metadata = &placement.metadata;
        world.entity_mut(entity).remove::<(
            crate::native::topology::rooms::Room,
            crate::native::topology::rooms::Member,
            crate::native::topology::appearance::Material,
            crate::native::topology::appearance::Light,
        )>();
        if let Some(room) = &metadata.room {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::rooms::Room(room.clone()));
        }
        if let Some(member) = &metadata.room_member {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::rooms::Member(member.clone()));
        }
        if let Some(material) = &metadata.material {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::appearance::Material(
                    material.clone(),
                ));
        }
        if let Some(light) = &metadata.light {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::appearance::Light(light.clone()));
        }
        world.entity_mut(entity).remove::<(
            crate::native::topology::terrain::Bound,
            crate::native::topology::terrain::FollowSlope,
        )>();
        if let Some(binding) = &metadata.binding {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::terrain::Bound(binding.clone()));
        }
        if metadata.follow_slope {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::terrain::FollowSlope);
        }
        world.entity_mut(entity).insert((
            ZIndex(metadata.order),
            crate::native::scoped_events::EventBoundary(metadata.event_boundary.clone()),
            crate::native::topology::Spatial {
                elevation: metadata.spatial.elevation,
                rotation: metadata.spatial.rotation,
                depth: metadata.spatial.depth,
                world_pinned: metadata.spatial.world_pinned,
            },
        ));
        world.entity_mut(entity).remove::<(
            crate::native::sand_placement::Pinned,
            GlobalZIndex,
            crate::native::topology::Attachment,
            crate::native::topology::groups::GroupPose,
            crate::native::canvas_selection::SandGroup,
        )>();
        if let Some(pin) = &metadata.pinned {
            world.entity_mut(entity).insert((
                crate::native::sand_placement::Pinned {
                    anchor: pin.anchor,
                    scale: pin.scale,
                },
                GlobalZIndex(1),
            ));
        }
        if let Some(pose) = &metadata.attachment {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::Attachment {
                    position: pose.position,
                    rotation: pose.rotation,
                });
        }
        if let Some(pose) = &metadata.group_pose {
            world
                .entity_mut(entity)
                .insert(crate::native::topology::groups::GroupPose {
                    position: pose.position,
                    rotation: pose.rotation,
                });
        }
        if let Some(group) = &placement.group {
            if group.len() == 32 {
                let bytes = (0..16)
                    .map(|index| u8::from_str_radix(&group[index * 2..index * 2 + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Invalid native group identity.")?;
                world
                    .entity_mut(entity)
                    .insert(crate::native::canvas_selection::SandGroup(
                        bytes.try_into().unwrap(),
                    ));
            }
        }
        if placement.selected {
            selection.push(entity);
        }
    }
    world
        .entity_mut(root)
        .insert(crate::native::canvas_selection::SandSelection(selection));
    crate::native::topology::rooms::accept_snapshot(world, root);
    Ok(())
}
