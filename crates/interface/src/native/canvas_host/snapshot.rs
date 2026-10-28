use super::*;

fn follows_native_state(state: &nucleus::component::ComponentState) -> bool {
    matches!(
        state,
        nucleus::component::ComponentState::Area { .. }
            | nucleus::component::ComponentState::Karma { .. }
            | nucleus::component::ComponentState::Frequency { .. }
            | nucleus::component::ComponentState::Transfer { .. }
            | nucleus::component::ComponentState::Calendar
    )
}

pub(super) fn capture_content(world: &World, entity: Entity) -> ContentKind {
    if let Some(Content(ContentKind::Authored { base, edits })) = world.get::<Content>(entity) {
        return ContentKind::Authored {
            base: base.clone(),
            edits: edits.clone(),
        };
    }
    capture_base_content(world, entity)
}

fn capture_base_content(world: &World, entity: Entity) -> ContentKind {
    if let Some(effect) = world.get::<crate::native::topology::terrain::Effect>(entity) {
        return crate::native::topology::terrain::component(&effect.0);
    }
    if let Some(composition) = composition::capture(world, entity) {
        return ContentKind::Composition { composition };
    }
    if let Some(content) = world.get::<Content>(entity) {
        if !matches!(
            content.0,
            ContentKind::Native { .. } | ContentKind::Authored { .. }
        ) && !matches!(&content.0, ContentKind::Builtin { state } if follows_native_state(state))
        {
            return content.0.clone();
        }
    }
    if let Some(placed) = world
        .get::<crate::native::component_push::Placed>(entity)
        .filter(|placed| !follows_native_state(&placed.component))
    {
        let state = crate::native::component_push::composition::capture(world, entity)
            .map(|composition| nucleus::component::ComponentState::Composition { composition })
            .unwrap_or_else(|| placed.component.clone());
        return ContentKind::Builtin { state };
    }
    if let Some(record) = world.get::<crate::native::workspace::RecordPlacement>(entity) {
        return ContentKind::Builtin {
            state: nucleus::component::ComponentState::Record {
                record: record.0.clone(),
                mode: nucleus::component::RecordMode::Full,
                start_call: None,
            },
        };
    }
    if let Some(area) = world.get::<crate::native::area::InfluenceArea>(entity) {
        return super::area::component(area);
    }
    if let Some(sand) = world.get::<StoredSand>(entity) {
        if matches!(world.get::<Content>(entity), Some(Content(ContentKind::Native { kind, .. })) if kind == "layout")
        {
            if let Some(component) = super::factories::capture(world, entity) {
                return component;
            }
        }
        let mut settings = BTreeMap::from([(
            "texts".into(),
            serde_json::json!(
                serde_json::to_string(&crate::native::sand_text::snapshot(world, entity)).unwrap()
            ),
        )]);
        settings.insert(
            "appearance".into(),
            serde_json::json!(
                serde_json::to_string(&crate::native::token_style::overrides(world, entity))
                    .unwrap()
            ),
        );
        if let Some(timer) = world.get::<crate::native::work_timer::LocalTimer>(entity) {
            settings.insert(
                "timer".into(),
                serde_json::json!(serde_json::to_string(timer).unwrap()),
            );
        }
        if let Some(clock) = world.get::<crate::native::time_castle::TimeSettings>(entity) {
            settings.insert(
                "time_castle".into(),
                serde_json::json!(serde_json::to_string(&clock.0).unwrap()),
            );
        }
        if let Some(todo) = crate::native::todo::snapshot(world, entity) {
            settings.insert(
                "todo".into(),
                serde_json::json!(serde_json::to_string(&todo).unwrap()),
            );
        }
        if let Some(video) = world.get::<crate::native::video_castle::Settings>(entity) {
            settings.insert(
                "video".into(),
                serde_json::json!(serde_json::to_string(video).unwrap()),
            );
        }
        if let Some(selection) = sand
            .content
            .and_then(|panel| lince_interface::visibility::selection(world, panel))
        {
            settings.insert(
                "visibility".into(),
                serde_json::json!(serde_json::to_string(&selection).unwrap()),
            );
        }
        if matches!(
            sand.kind,
            SandKind::Text | SandKind::EditableText | SandKind::Square
        ) {
            if let Some(text) = crate::native::sand_text::snapshot(world, entity).first() {
                for (id, value) in &text.area.settings.0 {
                    if sand.kind != SandKind::Square {
                        settings.insert(id.clone(), serde_json::to_value(value).unwrap());
                    }
                }
            }
            settings.insert(
                "text".into(),
                serde_json::json!(
                    crate::native::sand_text::snapshot(world, entity)
                        .iter()
                        .map(|text| text.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
        }
        return ContentKind::Native {
            kind: kind_id(sand.kind),
            settings,
            bindings: Vec::new(),
        };
    }
    if let Some(component) = super::factories::capture(world, entity) {
        return component;
    }
    if let Some(component) = super::kanban::capture(world, entity) {
        return component;
    }
    if let Some(castle) = world.get::<crate::native::protein_castle::ProteinCastle>(entity) {
        let query = serde_json::to_string(&castle.draft.query).unwrap();
        return ContentKind::Native {
            kind: "protein".into(),
            settings: BTreeMap::from([
                ("name".into(), serde_json::json!(castle.draft.name)),
                ("slug".into(), serde_json::json!(castle.draft.slug)),
                ("query".into(), serde_json::json!(query)),
            ]),
            bindings: record_references(&castle.draft.query),
        };
    }
    if let Some(instinct) = world.get::<crate::native::instinct::Instinct>(entity) {
        return ContentKind::Native {
            kind: "instinct".into(),
            settings: BTreeMap::from([(
                "page".into(),
                serde_json::json!(instinct.page.clone().unwrap_or_default()),
            )]),
            bindings: Vec::new(),
        };
    }
    use nucleus::component::ComponentState as Builtin;
    let state = if let Some(castle) = world.get::<crate::native::karma_castle::KarmaCastle>(entity)
    {
        Some(Builtin::Karma {
            search: castle.search.clone(),
        })
    } else if let Some(castle) =
        world.get::<crate::native::frequency_castle::FrequencyCastle>(entity)
    {
        Some(Builtin::Frequency {
            search: castle.search.clone(),
        })
    } else if let Some(castle) = world.get::<crate::native::transfer_castle::TransferCastle>(entity)
    {
        Some(Builtin::Transfer {
            search: castle.search.clone(),
        })
    } else if world
        .get::<crate::native::calendar::CalendarSand>(entity)
        .is_some()
    {
        Some(Builtin::Calendar)
    } else {
        None
    };
    if let Some(state) = state {
        return ContentKind::Builtin { state };
    }
    ContentKind::Native {
        kind: if world.get::<crate::native::kanban::Kanban>(entity).is_some() {
            "kanban"
        } else if world
            .get::<crate::native::simulation_castle::SimulationCastle>(entity)
            .is_some()
        {
            "simulation"
        } else if world
            .get::<crate::native::drawing::NativeDrawing>(entity)
            .is_some()
        {
            "drawing"
        } else if world
            .get::<crate::native::recorder_castle::RecorderCastle>(entity)
            .is_some()
        {
            "recorder"
        } else if world
            .get::<crate::native::document_viewer::DocumentViewer>(entity)
            .is_some()
        {
            "document"
        } else if world
            .get::<crate::native::media_sand::MediaSand>(entity)
            .is_some()
        {
            "media"
        } else if world.get::<crate::native::ide::Ide>(entity).is_some() {
            "ide"
        } else if world
            .get::<crate::native::file_explorer::FileExplorer>(entity)
            .is_some()
        {
            "explorer"
        } else if world
            .get::<crate::native::shader_castle::ShaderCastle>(entity)
            .is_some()
        {
            "shader"
        } else if world
            .get::<crate::native::assertion_castle::AssertionCastle>(entity)
            .is_some()
        {
            "assertion"
        } else if world
            .get::<crate::native::layout::LayoutBox>(entity)
            .is_some()
        {
            "layout"
        } else if world
            .get::<crate::native::topology::assets::ImportedAsset>(entity)
            .is_some()
        {
            "import"
        } else {
            "unavailable"
        }
        .into(),
        settings: BTreeMap::new(),
        bindings: Vec::new(),
    }
}

pub(super) fn record_references(value: &serde_json::Value) -> Vec<String> {
    fn visit(value: &serde_json::Value, found: &mut std::collections::BTreeSet<String>) {
        match value {
            serde_json::Value::String(value) if nucleus::valid_uid(value, "r") => {
                found.insert(value.clone());
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, found);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values() {
                    visit(value, found);
                }
            }
            _ => {}
        }
    }
    let mut found = std::collections::BTreeSet::new();
    visit(value, &mut found);
    found.into_iter().collect()
}

pub fn capture(world: &mut World, root: Entity) -> Result<api::Snapshot, String> {
    let entities: Vec<_> = world
        .get::<Children>(root)
        .map(|children| {
            children
                .iter()
                .filter(|entity| {
                    world.get::<CanvasItem>(*entity).is_some()
                        && !crate::native::laboratory::temporary_owner(world, *entity)
                        && super::authoring::owner(world, *entity) == *entity
                        && world.get::<WorkspaceMember>(*entity).is_none_or(|member| {
                            world.get::<Workspaces>(root).is_some_and(|spaces| {
                                spaces.entries.iter().any(|space| space.id == member.0)
                            })
                        })
                        && world
                            .get::<crate::native::canvas_item::Ephemeral>(*entity)
                            .is_none()
                })
                .collect()
        })
        .unwrap_or_default();
    let mut used = std::collections::HashSet::new();
    for entity in &entities {
        let id = world.get::<Identity>(*entity).map(|id| id.0.clone());
        if id
            .as_ref()
            .is_none_or(|id| !nucleus::valid_uid(id, "placement") || !used.insert(id.clone()))
        {
            let id = nucleus::new_uid("placement");
            used.insert(id.clone());
            world.entity_mut(*entity).insert(Identity(id));
        }
    }
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("Canvas has no workspaces.")?;
    let selected = world
        .get::<crate::native::canvas_selection::SandSelection>(root)
        .map(|selection| selection.0.as_slice())
        .unwrap_or_default();
    let camera = world.get::<CanvasView>(root).copied().unwrap_or_default();
    let scope = crate::native::laboratory::scope(world).filter(|(owner, _, _, _)| *owner == root);
    let workspaces = spaces
        .entries
        .iter()
        .filter(|space| scope.is_none_or(|(_, _, temporary, _)| space.id != temporary))
        .map(|space| {
            let view = if space.id == spaces.active {
                world
                    .get::<crate::native::topology::view::View>(root)
                    .copied()
                    .unwrap_or(space.topology)
            } else {
                space.topology
            };
            api::Workspace {
                id: space.id,
                name: space.name.clone(),
                center: if space.id == spaces.active {
                    camera.center.to_array()
                } else {
                    space.center
                },
                zoom: if space.id == spaces.active {
                    camera.zoom
                } else {
                    space.zoom
                },
                view: Some(api::View {
                    spatial: view.spatial,
                    position: view.position,
                    yaw: f64::from(view.yaw),
                    pitch: f64::from(view.pitch),
                    plane: view.plane,
                    selection_depth: view.selection_depth,
                }),
            }
        })
        .collect();
    let placements = entities
        .into_iter()
        .map(|entity| {
            let item = world.get::<CanvasItem>(entity).unwrap();
            let placement = crate::native::sand_placement::Placement::capture(world, entity);
            api::Placement {
                id: world.get::<Identity>(entity).unwrap().0.clone(),
                workspace: world
                    .get::<WorkspaceMember>(entity)
                    .map_or(spaces.entries[0].id, |member| member.0),
                component: capture_content(world, entity),
                geometry: api::Geometry {
                    position: item.position.to_array(),
                    size: item.size.as_dvec2().to_array(),
                },
                selected: selected
                    .iter()
                    .any(|selected| super::authoring::owner(world, *selected) == entity),
                group: placement
                    .group
                    .map(|group| group.0.iter().map(|byte| format!("{byte:02x}")).collect()),
                metadata: api::Metadata {
                    room: placement.room.map(|value| value.0),
                    room_member: placement.room_member.map(|value| value.0),
                    material: placement.material.map(|value| value.0),
                    light: placement.light.map(|value| value.0),
                    binding: placement.binding,
                    follow_slope: placement.follow_slope,
                    order: placement.order,
                    pinned: placement.pinned.map(|pin| api::Pinned {
                        anchor: pin.anchor,
                        scale: pin.scale,
                    }),
                    event_boundary: placement.events.0,
                    spatial: api::Spatial {
                        elevation: placement.spatial.elevation,
                        rotation: placement.spatial.rotation,
                        depth: placement.spatial.depth,
                        world_pinned: placement.spatial.world_pinned,
                    },
                    attachment: placement.attachment.map(|pose| api::Pose {
                        position: pose.position,
                        rotation: pose.rotation,
                    }),
                    group_pose: placement.group_pose.map(|pose| api::Pose {
                        position: pose.position,
                        rotation: pose.rotation,
                    }),
                },
            }
        })
        .collect();
    Ok(api::Snapshot {
        revision: spaces
            .canvas_state
            .as_ref()
            .map_or(0, |state| state.snapshot.revision),
        active_workspace: scope.map_or(spaces.active, |(_, _, temporary, previous)| {
            if spaces.active == temporary {
                previous
            } else {
                spaces.active
            }
        }),
        workspaces,
        placements,
        next_offset: None,
    })
}
