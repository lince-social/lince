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
    if let Some(composition) = composition::capture(world, entity) {
        return ContentKind::Composition { composition };
    }
    if let Some(content) = world.get::<Content>(entity) {
        if !matches!(content.0, ContentKind::Native { .. })
            && !matches!(&content.0, ContentKind::Builtin { state } if follows_native_state(state))
        {
            return content.0.clone();
        }
    }
    if let Some(placed) = world
        .get::<crate::component_push::Placed>(entity)
        .filter(|placed| !follows_native_state(&placed.component))
    {
        let state = crate::component_push::composition::capture(world, entity)
            .map(|composition| nucleus::component::ComponentState::Composition { composition })
            .unwrap_or_else(|| placed.component.clone());
        return ContentKind::Builtin { state };
    }
    if let Some(record) = world.get::<crate::workspace::RecordPlacement>(entity) {
        return ContentKind::Builtin {
            state: nucleus::component::ComponentState::Record {
                record: record.0.clone(),
                mode: nucleus::component::RecordMode::Full,
                start_call: None,
            },
        };
    }
    if let Some(sand) = world.get::<StoredSand>(entity) {
        let mut settings = BTreeMap::from([(
            "texts".into(),
            serde_json::json!(
                serde_json::to_string(&crate::sand_text::snapshot(world, entity)).unwrap()
            ),
        )]);
        settings.insert(
            "appearance".into(),
            serde_json::json!(
                serde_json::to_string(&crate::token_style::overrides(world, entity)).unwrap()
            ),
        );
        if let Some(timer) = world.get::<crate::work_timer::LocalTimer>(entity) {
            settings.insert(
                "timer".into(),
                serde_json::json!(serde_json::to_string(timer).unwrap()),
            );
        }
        if let Some(clock) = world.get::<crate::time_castle::TimeSettings>(entity) {
            settings.insert("time_castle".into(), serde_json::json!(serde_json::to_string(&clock.0).unwrap()));
        }
        if let Some(todo) = crate::todo::snapshot(world, entity) {
            settings.insert(
                "todo".into(),
                serde_json::json!(serde_json::to_string(&todo).unwrap()),
            );
        }
        if let Some(selection) = sand.content.and_then(|panel| lince_interface::visibility::selection(world, panel)) {
            settings.insert("visibility".into(), serde_json::json!(serde_json::to_string(&selection).unwrap()));
        }
        if matches!(
            sand.kind,
            SandKind::Text | SandKind::EditableText | SandKind::Square
        ) {
            if let Some(text) = crate::sand_text::snapshot(world, entity).first() {
                for (id, value) in &text.area.settings.0 {
                    settings.insert(id.clone(), serde_json::to_value(value).unwrap());
                }
            }
            settings.insert(
                "text".into(),
                serde_json::json!(
                    crate::sand_text::snapshot(world, entity)
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
    if let Some(castle) = world.get::<crate::protein_castle::ProteinCastle>(entity) {
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
    if let Some(instinct) = world.get::<crate::instinct::Instinct>(entity) {
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
    let state = if let Some(area) = world.get::<crate::area::InfluenceArea>(entity) {
        if let Some(config) = &area.protein {
            if let Some(uid) = config.draft.query["where"]
                .as_array()
                .and_then(|filters| filters.iter().find_map(|filter| filter["uid_eq"].as_str()))
                .filter(|uid| nucleus::valid_uid(uid, "r"))
            {
                Some(Builtin::Record {
                    record: uid.into(),
                    mode: nucleus::component::RecordMode::Full,
                    start_call: None,
                })
            } else {
                None
            }
        } else {
            Some(Builtin::Area {
                immunity: match area.immunity {
                    crate::area_effects::Immunity::None => nucleus::component::Immunity::None,
                    crate::area_effects::Immunity::External => {
                        nucleus::component::Immunity::External
                    }
                    crate::area_effects::Immunity::Internal => {
                        nucleus::component::Immunity::Internal
                    }
                    crate::area_effects::Immunity::All => nucleus::component::Immunity::All,
                    crate::area_effects::Immunity::Containment => {
                        nucleus::component::Immunity::Containment
                    }
                    crate::area_effects::Immunity::Isolation => {
                        nucleus::component::Immunity::Isolation
                    }
                },
                strength: area.strength as i32,
            })
        }
    } else if let Some(castle) = world.get::<crate::karma_castle::KarmaCastle>(entity) {
        Some(Builtin::Karma {
            search: castle.search.clone(),
        })
    } else if let Some(castle) = world.get::<crate::frequency_castle::FrequencyCastle>(entity) {
        Some(Builtin::Frequency {
            search: castle.search.clone(),
        })
    } else if let Some(castle) = world.get::<crate::transfer_castle::TransferCastle>(entity) {
        Some(Builtin::Transfer {
            search: castle.search.clone(),
        })
    } else if world.get::<crate::calendar::CalendarSand>(entity).is_some() {
        Some(Builtin::Calendar)
    } else {
        None
    };
    if let Some(state) = state {
        return ContentKind::Builtin { state };
    }
    ContentKind::Native {
        kind: if world.get::<crate::kanban::Kanban>(entity).is_some() {
            "kanban"
        } else if world
            .get::<crate::simulation_castle::SimulationCastle>(entity)
            .is_some()
        {
            "simulation"
        } else if world.get::<crate::drawing::NativeDrawing>(entity).is_some() {
            "drawing"
        } else if world
            .get::<crate::recorder_castle::RecorderCastle>(entity)
            .is_some()
        {
            "recorder"
        } else if world
            .get::<crate::document_viewer::DocumentViewer>(entity)
            .is_some()
        {
            "document"
        } else if world.get::<crate::media_sand::MediaSand>(entity).is_some() {
            "media"
        } else if world.get::<crate::ide::Ide>(entity).is_some() {
            "ide"
        } else if world
            .get::<crate::file_explorer::FileExplorer>(entity)
            .is_some()
        {
            "explorer"
        } else if world
            .get::<crate::shader_castle::ShaderCastle>(entity)
            .is_some()
        {
            "shader"
        } else if world
            .get::<crate::assertion_castle::AssertionCastle>(entity)
            .is_some()
        {
            "assertion"
        } else if world.get::<crate::layout::LayoutBox>(entity).is_some() {
            "layout"
        } else if world
            .get::<crate::topology::assets::ImportedAsset>(entity)
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
                        && world.get::<WorkspaceMember>(*entity).is_none_or(|member| {
                            world.get::<Workspaces>(root).is_some_and(|spaces| {
                                spaces.entries.iter().any(|space| space.id == member.0)
                            })
                        })
                        && world
                            .get::<crate::external_drop::Preview>(*entity)
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
        .get::<crate::canvas_selection::SandSelection>(root)
        .map(|selection| selection.0.as_slice())
        .unwrap_or_default();
    let camera = world.get::<CanvasView>(root).copied().unwrap_or_default();
    let workspaces = spaces
        .entries
        .iter()
        .map(|space| {
            let view = if space.id == spaces.active {
                world
                    .get::<crate::topology::view::View>(root)
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
            let placement = crate::sand_placement::Placement::capture(world, entity);
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
                selected: selected.contains(&entity),
                group: placement
                    .group
                    .map(|group| group.0.iter().map(|byte| format!("{byte:02x}")).collect()),
                metadata: api::Metadata {
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
        active_workspace: spaces.active,
        workspaces,
        placements,
        next_offset: None,
    })
}
