use super::*;
use crate::actions::Action;
use crate::scoped_events::{EventListener, SandEvent};

#[derive(Component)]
struct Preview(Entity);

#[derive(Component)]
struct PreviewHidden {
    owner: Entity,
    visibility: Visibility,
}

#[derive(Clone)]
struct Close;
impl Action for Close {
    fn apply(&self, world: &mut World, entity: Entity) {
        close(world, entity);
    }
}

fn close(world: &mut World, owner: Entity) {
    if let Some(preview) = world.entity_mut(owner).take::<Preview>()
        && world.get_entity(preview.0).is_ok()
    {
        world.despawn(preview.0);
    }
    let hidden: Vec<_> = world.query_filtered::<(Entity, &PreviewHidden), bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>>()
        .iter(world).filter(|(_, hidden)| hidden.owner == owner).map(|(entity, hidden)| (entity, hidden.visibility)).collect();
    for (entity, visibility) in hidden {
        world
            .entity_mut(entity)
            .remove::<(PreviewHidden, bevy::ecs::entity_disabling::Disabled)>()
            .insert(visibility);
    }
}

fn hide_rows(world: &mut World, owner: Entity) {
    for entity in crate::protein_area::row_entities(world, owner) {
        if world.get_entity(entity).is_ok() && world.get::<PreviewHidden>(entity).is_none() {
            let visibility = world.get::<Visibility>(entity).copied().unwrap_or_default();
            world.entity_mut(entity).insert((
                PreviewHidden { owner, visibility },
                Visibility::Hidden,
                bevy::ecs::entity_disabling::Disabled,
            ));
        }
    }
}

#[derive(Clone)]
struct Origin(String);
impl Action for Origin {
    fn apply(&self, world: &mut World, owner: Entity) {
        crate::karma_castle::open_rule(world, owner, &self.0);
    }
}

pub(super) fn listeners(world: &mut World) {
    let areas: Vec<_> = world
        .query::<(Entity, &crate::area::InfluenceArea)>()
        .iter(world)
        .map(|(entity, area)| {
            (
                entity,
                area.protein
                    .as_ref()
                    .is_some_and(|config| config.listen_record_selection),
            )
        })
        .collect();
    for (entity, enabled) in areas {
        if let Some(preview) = world.get::<Preview>(entity) {
            if world.get_entity(preview.0).is_err() {
                close(world, entity);
            } else {
                hide_rows(world, entity);
            }
        }
        let mut names = world
            .get::<EventListener>(entity)
            .map(|listener| listener.0.clone())
            .unwrap_or_default();
        let present = names.iter().any(|name| name == model::RECORD_SELECTED);
        if enabled != present {
            names.retain(|name| name != model::RECORD_SELECTED);
            if enabled {
                names.push(model::RECORD_SELECTED.into());
            }
            world.entity_mut(entity).insert(EventListener(names));
        }
    }
}

fn dirty(world: &mut World, owner: Entity) -> bool {
    if crate::protein_area::selection_pending(world, owner)
        || crate::schedule_editor::pending(world, owner)
    {
        return true;
    }
    world
        .query::<(
            &crate::protein_area::RecordBinding,
            Option<&crate::sand::Unsaved>,
            Option<&bevy::text::EditableText>,
        )>()
        .iter(world)
        .any(|(binding, unsaved, text)| {
            binding.area == owner
                && (unsaved.is_some_and(|unsaved| unsaved.0)
                    || text
                        .is_some_and(|text| text.is_composing() || !text.pending_edits.is_empty())
                    || crate::record_binding::pending(world, binding))
        })
}

pub(super) fn selected(event: On<SandEvent>, mut commands: Commands) {
    if event.name != model::RECORD_SELECTED {
        return;
    }
    let Ok(selection) = serde_json::from_value::<model::Selection>(event.value.clone()) else {
        return;
    };
    let owner = event.entity;
    commands.queue(move |world: &mut World| {
        let Some(config) = world
            .get::<crate::area::InfluenceArea>(owner)
            .and_then(|area| area.protein.as_ref())
        else {
            return;
        };
        if !config.listen_record_selection
            || !config.record_cards
            || !nucleus::valid_uid(&selection.record_uid, "r")
        {
            return;
        }
        if dirty(world, owner) {
            crate::notifications::report(
                world,
                "interface::time-castle",
                "Save the current Record edits before selecting another occurrence.",
            );
            return;
        }
        world
            .entity_mut(owner)
            .insert(SelectedOccurrence(selection.clone()));
        close(world, owner);
        if selection.entry.preview {
            let panel = world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(8),
                        top: px(40),
                        width: px(480),
                        padding: UiRect::all(px(12)),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    ZIndex(90),
                    crate::token_style::background(crate::tokens::Token::Surface),
                    ChildOf(owner),
                ))
                .id();
            crate::edit_mode::label(world, panel, "Projection preview · read only", 18.0);
            let settings = Settings {
                timezone: selection.timezone.clone(),
                ..Settings::default()
            };
            crate::edit_mode::label(
                world,
                panel,
                &format!(
                    "{}\nQuantity {}\n{}\n{}",
                    selection.entry.head,
                    selection.entry.quantity,
                    selection
                        .entry
                        .time
                        .as_ref()
                        .map(|time| {
                            format!(
                                "{}{}",
                                settings.label(time.from_ms),
                                time.until_ms
                                    .map(|end| format!(" → {}", settings.label(end)))
                                    .unwrap_or_default()
                            )
                        })
                        .unwrap_or_default(),
                    ui::origin_label(&selection.entry, &settings)
                ),
                13.0,
            );
            crate::sand_panel::button(world, panel, owner, "Close preview", Close);
            if matches!(selection.source, Source::Local)
                && let Some(rule) = selection.entry.origin["cause"]["occurrence"]["rule_uid"]
                    .as_str()
                    .filter(|uid| nucleus::valid_uid(uid, "rec"))
            {
                crate::sand_panel::button(
                    world,
                    panel,
                    owner,
                    "Open originating rule",
                    Origin(rule.into()),
                );
            }
            world.entity_mut(owner).insert(Preview(panel));
            hide_rows(world, owner);
            return;
        }
        let replacement = crate::full_record::config(&selection.record_uid, selection.source);
        let mut area = world.get_mut::<crate::area::InfluenceArea>(owner).unwrap();
        let config = area.protein.as_mut().unwrap();
        config.source = replacement.source;
        config.draft.query = replacement.draft.query;
        config.enabled = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_recurrence_preview_preserves_the_live_query_and_explains_its_origin() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.add_observer(selected);
        let initial = nucleus::new_uid("r");
        let projected = nucleus::new_uid("r");
        let rule = nucleus::new_uid("rec");
        let mut config = crate::full_record::config(&initial, Source::Local);
        config.listen_record_selection = true;
        let original_query = config.draft.query.to_string();
        let receiver = world
            .spawn(crate::area::InfluenceArea {
                protein: Some(config),
                ..crate::area::InfluenceArea::new(
                    crate::area::AreaShape::Square,
                    bevy::math::DVec2::ZERO,
                    bevy::math::DVec2::splat(500.0),
                )
            })
            .id();
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-03T14:30:00Z")
            .unwrap()
            .timestamp_millis();
        let cause = nucleus::simulation::Cause::Rule {
            occurrence: nucleus::simulation::RuleOccurrence {
                rule_uid: rule.clone(),
                revision: 7,
                event_id: "future-event".into(),
                frequency: None,
                intended_at_ms: Some(at),
            },
            consequence: 0,
        };
        let selection = model::Selection {
            record_uid: projected.clone(),
            source: Source::Local,
            timezone: "UTC".into(),
            entry: Entry {
                id: "projected-occurrence".into(),
                record_uid: projected.clone(),
                head: "Future brushing".into(),
                quantity: "-1".into(),
                category: model::Category::Timed,
                time: Some(nucleus::schedule::TimeRange {
                    from_ms: at,
                    until_ms: Some(at + 600_000),
                }),
                origin: serde_json::json!({"kind":"projection", "cause":cause}),
                preview: true,
                start_date: None,
                due_date: None,
            },
        };
        world.trigger(SandEvent {
            entity: receiver,
            source: receiver,
            name: model::RECORD_SELECTED.into(),
            value: serde_json::to_value(selection).unwrap(),
        });
        world.flush();
        let query = world
            .get::<crate::area::InfluenceArea>(receiver)
            .unwrap()
            .protein
            .as_ref()
            .unwrap()
            .draft
            .query
            .to_string();
        assert_eq!(query, original_query);
        assert!(
            world
                .get::<SelectedOccurrence>(receiver)
                .unwrap()
                .0
                .entry
                .preview
        );
        let text = world
            .query::<&Text>()
            .iter(&world)
            .map(|text| text.0.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Projection preview · read only"));
        assert!(text.contains("Recurring occurrence scheduled for 10-03 14:30:00 +00:00"));
        assert!(text.contains("Open originating rule"));
        assert!(!text.contains(&rule));
        assert!(!text.contains(&projected));
        assert!(!text.contains("intended_at_ms"));
        let panel = world.get::<Preview>(receiver).unwrap().0;
        Close.apply(&mut world, receiver);
        assert!(world.get::<Preview>(receiver).is_none());
        assert!(world.get_entity(panel).is_err());
    }

    #[test]
    fn selection_listener_is_explicit_and_survives_config_serialization() {
        let mut config = crate::protein_area::Config::records();
        assert!(!config.listen_record_selection);
        config.listen_record_selection = true;
        let config: crate::protein_area::Config =
            serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
        assert!(config.listen_record_selection);
        let mut world = World::new();
        let area = crate::area::InfluenceArea {
            protein: Some(config),
            ..crate::area::InfluenceArea::new(
                crate::area::AreaShape::Circle,
                bevy::math::DVec2::ZERO,
                bevy::math::DVec2::splat(500.0),
            )
        };
        let receiver = world.spawn(area).id();
        listeners(&mut world);
        assert_eq!(
            world.get::<EventListener>(receiver).unwrap().0,
            vec![model::RECORD_SELECTED]
        );
        world
            .get_mut::<crate::area::InfluenceArea>(receiver)
            .unwrap()
            .protein
            .as_mut()
            .unwrap()
            .listen_record_selection = false;
        listeners(&mut world);
        assert!(world.get::<EventListener>(receiver).unwrap().0.is_empty());
    }

    #[test]
    fn time_castle_selection_switches_only_an_opted_in_receiver_in_its_workspace() {
        let mut world = World::new();
        world.add_observer(selected);
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let initial = nucleus::new_uid("r");
        let next = nucleus::new_uid("r");
        let mut config = crate::full_record::config(&initial, Source::Local);
        config.listen_record_selection = true;
        let area = crate::area::InfluenceArea {
            protein: Some(config),
            ..crate::area::InfluenceArea::new(
                crate::area::AreaShape::Square,
                bevy::math::DVec2::ZERO,
                bevy::math::DVec2::splat(500.0),
            )
        };
        let receiver = world
            .spawn((
                area.clone(),
                ChildOf(root),
                crate::workspace::WorkspaceMember(1),
            ))
            .id();
        let other = world
            .spawn((area, ChildOf(root), crate::workspace::WorkspaceMember(2)))
            .id();
        let source = world
            .spawn((ChildOf(root), crate::workspace::WorkspaceMember(1)))
            .id();
        listeners(&mut world);
        let selection = model::Selection {
            record_uid: next.clone(),
            source: Source::Local,
            timezone: "UTC".into(),
            entry: Entry {
                id: "event".into(),
                record_uid: next.clone(),
                head: "Next".into(),
                quantity: "-1".into(),
                category: model::Category::Timed,
                time: Some(nucleus::schedule::TimeRange {
                    from_ms: 1000,
                    until_ms: None,
                }),
                origin: serde_json::json!({"kind":"manual"}),
                preview: false,
                start_date: None,
                due_date: None,
            },
        };
        crate::scoped_events::emit(
            &mut world,
            source,
            model::RECORD_SELECTED,
            serde_json::to_value(&selection).unwrap(),
        );
        world.flush();
        let query = |world: &World, owner| {
            world
                .get::<crate::area::InfluenceArea>(owner)
                .unwrap()
                .protein
                .as_ref()
                .unwrap()
                .draft
                .query
                .to_string()
        };
        assert!(query(&world, receiver).contains(&next));
        assert!(query(&world, other).contains(&initial));
        assert_eq!(
            world
                .get::<SelectedOccurrence>(receiver)
                .unwrap()
                .0
                .record_uid,
            next
        );
        assert!(world.get::<SelectedOccurrence>(other).is_none());
        world.spawn((
            crate::protein_area::RecordBinding {
                area: receiver,
                uid: initial,
                source: Source::Local,
            },
            crate::sand::Unsaved(true),
        ));
        assert!(dirty(&mut world, receiver));
        let mut replacement = selection;
        replacement.record_uid = nucleus::new_uid("r");
        replacement.entry.record_uid = replacement.record_uid.clone();
        crate::scoped_events::emit(
            &mut world,
            source,
            model::RECORD_SELECTED,
            serde_json::to_value(replacement).unwrap(),
        );
        world.flush();
        assert!(query(&world, receiver).contains(&next));
        assert_eq!(
            world
                .get::<SelectedOccurrence>(receiver)
                .unwrap()
                .0
                .record_uid,
            next
        );
    }
}
