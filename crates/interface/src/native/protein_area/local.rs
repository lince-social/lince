use super::*;
use tokio::sync::mpsc::Sender;

pub(super) fn owns(state: &State) -> bool {
    state.remote.is_none()
        && state
            .applied
            .as_ref()
            .is_some_and(|config| matches!(config.source, Source::Local))
}

pub(super) fn interrupt(state: &mut State) -> Vec<(String, Entity)> {
    state.ready = false;
    state.refreshing = false;
    state.login = false;
    state.login_pending = false;
    state.thread_requests = 0;
    state.pending.clear();
    state.actions.drain().collect()
}

pub(super) fn failed(world: &mut World, actions: Vec<(String, Entity)>, message: &str) {
    for (request, editor) in actions {
        history::finished(world, &request, None, false, false);
        rows::action_finished(world, editor, Some(message.into()));
    }
}

pub(super) fn sendable(state: &State) -> bool {
    state.ready
        || owns(state)
            && matches!(
                state.pending.front(),
                Some(ClientMessage::Subscribe { .. } | ClientMessage::Unsubscribe { .. })
            )
}

pub(super) fn synchronize(world: &mut World) {
    let sender = world
        .get_non_send::<CellBridge>()
        .filter(|bridge| !bridge.outgoing.is_closed() && !bridge.incoming.is_closed())
        .map(|bridge| bridge.outgoing.clone());
    if let Some(sender) = sender {
        observe(world, sender);
    }
}

fn observe(world: &mut World, sender: Sender<ClientMessage>) {
    let previous = world
        .resource_mut::<Runtime>()
        .local_sender
        .replace(sender.clone());
    let changed = previous
        .as_ref()
        .is_some_and(|old| !old.same_channel(&sender));
    if previous
        .as_ref()
        .is_some_and(|old| old.same_channel(&sender))
    {
        return;
    }
    let owners: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .iter()
        .filter(|(_, state)| {
            state
                .applied
                .as_ref()
                .is_some_and(|config| config.enabled && matches!(config.source, Source::Local))
                && state.remote.is_none()
                && state.subscription.is_some()
                && (changed || !state.ready && state.pending.is_empty())
        })
        .map(|(owner, state)| {
            (
                *owner,
                state.applied.clone().unwrap(),
                state.thread_limits.clone(),
            )
        })
        .collect();
    for (owner, config, limits) in owners {
        let mut protein = match query(world, owner, &config) {
            Ok(protein) => protein,
            Err(error) => {
                let state = world
                    .resource_mut::<Runtime>()
                    .into_inner()
                    .areas
                    .get_mut(&owner)
                    .unwrap();
                state.ready = false;
                state.status = error;
                continue;
            }
        };
        if let Some(threads) = protein.include.threads.as_mut() {
            threads.message_limits = limits;
        }
        let subscription = id(world);
        let state = world
            .resource_mut::<Runtime>()
            .into_inner()
            .areas
            .get_mut(&owner)
            .unwrap();
        let actions = interrupt(state);
        state.subscription = Some(subscription.clone());
        state.status = "Loading".into();
        state.pending.push_back(ClientMessage::Subscribe {
            id: subscription,
            protein,
        });
        failed(
            world,
            actions,
            "The connection changed. Refresh before saving again.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity, Entity, String) {
        let mut world = World::new();
        world.init_resource::<Runtime>();
        let owner = world.spawn_empty().id();
        let uid = nucleus::new_uid("r");
        let editor = world
            .spawn(bevy::text::EditableText::new("Unsaved Record draft"))
            .id();
        world.resource_mut::<Runtime>().areas.insert(
            owner,
            State {
                applied: Some(Config {
                    enabled: true,
                    ..Config::records()
                }),
                ready: true,
                subscription: Some("original-subscription".into()),
                data: vec![serde_json::json!({"uid":uid,"head":"Original"})],
                order: std::sync::Arc::new(vec![uid.clone()]),
                row_entities: HashMap::from([(uid.clone(), editor)]),
                ..default()
            },
        );
        (world, owner, editor, uid)
    }

    #[test]
    fn stopped_local_channel_keeps_record_entities_and_drafts_but_denies_mutations() {
        let (mut world, owner, editor, uid) = fixture();
        execute_action(
            &mut world,
            &RecordBinding {
                area: owner,
                uid: uid.clone(),
                source: Source::Local,
            },
            editor,
            engine::actions::Action::DeleteRecord {
                target: uid.clone(),
            },
        )
        .unwrap();
        assert_eq!(world.resource::<Runtime>().areas[&owner].actions.len(), 1);
        receive(
            &mut world,
            owner,
            ServerMessage::Error {
                id: crate::native::cell_bridge::CONNECTION.into(),
                message: "The local connection stopped".into(),
                code: Some("connection_closed".into()),
            },
        );
        let state = &world.resource::<Runtime>().areas[&owner];
        assert!(!state.ready);
        assert!(state.pending.is_empty());
        assert!(state.actions.is_empty());
        assert!(!state.dirty);
        assert_eq!(state.row_entities[&uid], editor);
        assert_eq!(state.data[0]["head"], "Original");
        assert_eq!(state.order.as_slice(), [uid.clone()]);
        assert_eq!(
            world
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Unsaved Record draft"
        );
        assert!(
            execute_action(
                &mut world,
                &RecordBinding {
                    area: owner,
                    uid: uid.clone(),
                    source: Source::Local
                },
                editor,
                engine::actions::Action::DeleteRecord { target: uid }
            )
            .is_err()
        );
    }

    #[test]
    fn local_disconnect_reports_one_notice_for_multiple_cached_areas() {
        use crate::native::{cell_bridge::CellMessage, notifications};
        let (mut world, first, _, uid) = fixture();
        let second = world.spawn_empty().id();
        world.resource_mut::<Runtime>().areas.insert(
            second,
            State {
                applied: Some(Config {
                    enabled: true,
                    ..Config::records()
                }),
                ready: true,
                subscription: Some("second-subscription".into()),
                data: vec![serde_json::json!({"uid":uid,"head":"Original"})],
                ..default()
            },
        );
        let log = cell::Diagnostics::default();
        world.insert_resource(notifications::Notifications::new(log.clone()));
        world.init_resource::<Messages<CellMessage>>();
        let message = ServerMessage::Error {
            id: crate::native::cell_bridge::CONNECTION.into(),
            message: "The connection to this Cell stopped. Your drafts are still here.".into(),
            code: Some("connection_closed".into()),
        };
        let mut schedule = Schedule::default();
        schedule.add_systems(notifications::cell_connection);
        for occurrence in 1..=2 {
            world.write_message(CellMessage(message.clone()));
            for owner in [first, second] {
                receive(&mut world, owner, message.clone());
                let state = &world.resource::<Runtime>().areas[&owner];
                assert!(!state.ready);
                assert_eq!(state.data[0]["uid"], uid);
            }
            schedule.run(&mut world);
            let notices = log.snapshot().1;
            assert_eq!(notices.len(), 1);
            assert_eq!(notices[0].source, "interface::connection");
            assert_eq!(notices[0].occurrences, occurrence);
            schedule.run(&mut world);
            assert_eq!(log.snapshot().1[0].occurrences, occurrence);
        }
    }

    #[test]
    fn new_local_channel_replays_only_fresh_subscriptions_and_rejects_stale_snapshots() {
        let (mut world, owner, editor, uid) = fixture();
        let (first, _first_receiver) = tokio::sync::mpsc::channel(1);
        observe(&mut world, first);
        let state = world
            .resource_mut::<Runtime>()
            .into_inner()
            .areas
            .get_mut(&owner)
            .unwrap();
        state.pending.push_back(ClientMessage::Act {
            id: "old-write".into(),
            action: engine::actions::Action::DeleteRecord {
                target: uid.clone(),
            },
        });
        let (second, _second_receiver) = tokio::sync::mpsc::channel(1);
        observe(&mut world, second.clone());
        let state = &world.resource::<Runtime>().areas[&owner];
        assert!(!state.ready);
        assert!(sendable(state));
        assert_eq!(state.pending.len(), 1);
        let ClientMessage::Subscribe { id, .. } = state.pending.front().unwrap() else {
            panic!("A mutation was replayed")
        };
        let fresh = id.clone();
        assert_ne!(fresh, "original-subscription");
        observe(&mut world, second);
        assert_eq!(world.resource::<Runtime>().areas[&owner].pending.len(), 1);
        receive(
            &mut world,
            owner,
            ServerMessage::Snapshot {
                id: "original-subscription".into(),
                rows: vec![],
            },
        );
        receive(
            &mut world,
            owner,
            ServerMessage::SessionAuthenticated {
                id: "authentication".into(),
                session_id: "replacement".into(),
                person: "owner".into(),
                key_id: "owner-key".into(),
            },
        );
        assert!(!world.resource::<Runtime>().areas[&owner].ready);
        assert_eq!(
            world.resource::<Runtime>().areas[&owner].row_entities[&uid],
            editor
        );
        receive(
            &mut world,
            owner,
            ServerMessage::Snapshot {
                id: fresh,
                rows: vec![serde_json::json!({"uid":uid,"head":"Recovered"})],
            },
        );
        assert!(world.resource::<Runtime>().areas[&owner].ready);
        assert_eq!(
            world.resource::<Runtime>().areas[&owner].data[0]["head"],
            "Recovered"
        );
        assert_eq!(
            world
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Unsaved Record draft"
        );
    }

    #[test]
    fn permission_errors_and_remote_authentication_still_clear_unavailable_rows() {
        for remote in [false, true] {
            let (mut world, owner, _, _) = fixture();
            if remote {
                let state = world
                    .resource_mut::<Runtime>()
                    .into_inner()
                    .areas
                    .get_mut(&owner)
                    .unwrap();
                state.remote = Some("remote".into());
                state.applied.as_mut().unwrap().source = Source::Organ("remote".into());
            }
            receive(
                &mut world,
                owner,
                ServerMessage::Error {
                    id: if remote {
                        "connection"
                    } else {
                        "original-subscription"
                    }
                    .into(),
                    message: "Permission denied".into(),
                    code: Some("permission_denied".into()),
                },
            );
            let state = &world.resource::<Runtime>().areas[&owner];
            assert!(!state.ready);
            assert!(state.data.is_empty());
            assert!(state.order.is_empty());
            assert!(state.dirty);
        }
    }
}
