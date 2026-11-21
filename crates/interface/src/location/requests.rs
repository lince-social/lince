use std::time::{Duration, Instant};

use bevy::prelude::*;
use nucleus::location::{Command, Context, DEFAULT_DURATION_SECONDS, Settings, SourceKind, View};
use serde::Deserialize;

use crate::controls;

use super::{
    DraftScope, InvitationReview, Panel, Pending, Request, UiTick,
    actions::observer_command,
    capture, clear_private_actor,
    clock::Stamp,
    render::{draw, position_text},
    saved::{queue_saved, receive_saved},
};

pub(super) fn queue(world: &mut World, panel: &mut Panel, command: Command, then: Vec<Command>) {
    let id = nucleus::new_uid("location-ui");
    panel.pending = Some(Pending {
        id: id.clone(),
        command: command.clone(),
        then,
        sent: Stamp::for_world(world),
        save_place: false,
        scope: Some(DraftScope::of(panel)),
    });
    panel.next_poll = Instant::now() + Duration::from_secs(3);
    world.write_message(Request {
        owner: panel.owner,
        id,
        command,
    });
}

/// Delivers an owned reply and returns whether a panel recognized its request ID.
pub fn receive(world: &mut World, id: &str, data: Result<serde_json::Value, String>) -> bool {
    receive_ref(world, id, data.as_ref().map_err(String::as_str))
}

/// Delivers a borrowed reply and returns whether a panel recognized its request ID.
pub fn receive_ref(world: &mut World, id: &str, data: Result<&serde_json::Value, &str>) -> bool {
    if receive_saved(world, id, data) {
        return true;
    }
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let stamp = Stamp::for_world(world);
    let _reply = nucleus::performance::Span::new("location-reply-application");
    nucleus::performance::add("location-replies-accepted", 1);
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("location panel");
    capture(world, &mut panel);
    let Some(pending) = panel.pending.take() else {
        world.entity_mut(entity).insert(panel);
        draw(world, entity);
        return true;
    };
    if pending
        .scope
        .as_ref()
        .is_some_and(|scope| !scope.matches(&panel))
    {
        panel.handover = None;
        panel.invitation_review = None;
        panel.view = None;
        panel.notice =
            "The private source draft changed. Refresh location controls before continuing".into();
        world.entity_mut(entity).insert(panel);
        draw(world, entity);
        return true;
    }
    let mut redraw = false;
    match data {
        Err(error) => {
            panel.notice = error.to_owned();
            panel.view = None;
        }
        Ok(data) => {
            if let Command::Configure { settings } | Command::Handover { settings, .. } =
                &pending.command
                && let Some(context) = &mut panel.context
            {
                context.settings = Some(settings.clone());
            }
            if let Command::PrepareInvitation {
                person,
                record_uid,
                recipient,
                duration_seconds,
                radius_metres,
            } = &pending.command
            {
                let invitation = nucleus::location::Invitation::deserialize(&data["invitation"]);
                match invitation {
                    Ok(invitation)
                        if invitation.validate().is_ok()
                            && &invitation.record_uid == record_uid
                            && panel.context.as_ref().is_some_and(|context| {
                                context.authority_node_id == invitation.authority_node_id
                            })
                            && invitation.grant.as_ref().is_some_and(|grant| {
                                &grant.issuer == person
                                    && &grant.recipient == recipient
                                    && &grant.radius_metres == radius_metres
                            })
                            && data["remaining_millis"].as_u64().is_some_and(|remaining| {
                                remaining > 0 && remaining <= u64::from(*duration_seconds) * 1000
                            }) =>
                    {
                        let remaining_millis = data["remaining_millis"]
                            .as_u64()
                            .unwrap_or(0)
                            .saturating_sub(pending.sent.elapsed_millis());
                        panel.invitation_review = Some(InvitationReview {
                            invitation,
                            remaining_millis,
                            received: stamp,
                            scope: DraftScope::of(&panel),
                        });
                        panel.notice = "Review the recipient, conversation, precision and remaining lifetime before sending".into();
                        redraw = true;
                    }
                    _ => panel.notice = "The invitation response was invalid".into(),
                }
            } else if matches!(pending.command, Command::SendInvitation { .. }) {
                panel.notice = "Private invitation saved for ordinary conversation delivery. Check the conversation for delivery status".into();
                redraw = true;
            } else if pending.save_place {
                panel.saved_poll = Instant::now();
                panel.notice =
                    "Saved as the Record's place. Its saved-place visibility policy applies."
                        .into();
                panel.next_poll = Instant::now() + Duration::from_secs(5);
            } else if matches!(pending.command, Command::Context { .. }) {
                match Context::deserialize(data) {
                    Ok(mut context) => {
                        context.view.elapse(pending.sent.elapsed_millis());
                        if panel.settings.is_none() || panel.context.is_none() {
                            panel.settings = context.settings.clone().or_else(|| {
                                (!panel.person.is_empty()).then(|| Settings {
                                    record_uid: panel.record.clone(),
                                    controller_uid: panel.person.clone(),
                                    source_cell_uid: context.current_cell_uid.clone(),
                                    source_node_id: context.current_node_id.clone(),
                                    source_kind: SourceKind::Device,
                                    duration_seconds: DEFAULT_DURATION_SECONDS,
                                    recipients: Vec::new(),
                                    transfer_uid: (!panel.transfer.is_empty())
                                        .then(|| panel.transfer.clone()),
                                })
                            });
                            if let Some(settings) = &panel.settings {
                                panel.minutes = (settings.duration_seconds / 60).to_string();
                                if let Some(transfer) = &settings.transfer_uid {
                                    panel.transfer = transfer.clone();
                                }
                            }
                            redraw = true;
                        }
                        panel.view = Some((context.view.clone(), stamp));
                        panel.context = Some(context);
                        panel.notice.clear();
                    }
                    Err(_) => panel.notice = "Location controls returned an invalid reply".into(),
                }
            } else {
                let view = data.get("view").unwrap_or(data);
                if let Ok(mut view) = View::deserialize(view) {
                    view.elapse(pending.sent.elapsed_millis());
                    redraw |= panel.view.as_ref().is_none_or(|(previous, _)| {
                        previous.status != view.status
                            || previous.session_uid != view.session_uid
                            || previous.source_kind != view.source_kind
                            || previous.fix.is_some() != view.fix.is_some()
                    });
                    panel.view = Some((view, stamp));
                }
                panel.notice.clear();
                if !pending.then.is_empty() {
                    let mut then = pending.then;
                    let command = then.remove(0);
                    queue(world, &mut panel, command, then);
                } else {
                    panel.next_poll = Instant::now() + Duration::from_secs(3);
                }
            }
        }
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
    true
}

pub(super) fn maintain(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Panel>>()
        .iter(world)
        .collect();
    if !entities.is_empty()
        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
    {
        let mut tick = world.resource_mut::<UiTick>();
        if tick.0.elapsed() >= Duration::from_secs(1) {
            tick.0 = Instant::now();
            wake.after(Duration::from_secs(1));
        }
    }
    for entity in entities {
        let mut panel = world
            .entity_mut(entity)
            .take::<Panel>()
            .expect("location panel");
        if panel
            .saved_pending
            .as_ref()
            .is_some_and(|(_, _, sent)| sent.elapsed() > Duration::from_secs(12))
        {
            panel.saved_pending = None;
            panel.notice = "Saved location request timed out".into();
            panel.saved_poll = Instant::now() + Duration::from_secs(10);
        }
        if !panel.observer
            && panel.transfer != panel.record
            && panel.saved_pending.is_none()
            && Instant::now() >= panel.saved_poll
        {
            let record = panel.record.clone();
            queue_saved(
                world,
                &mut panel,
                nucleus::vocabulary::SavedLocationCommand::Inspect { record },
            );
        }
        if panel
            .pending
            .as_ref()
            .is_some_and(|pending| pending.sent.elapsed() > Duration::from_secs(12))
        {
            panel.pending = None;
            panel.view = None;
            panel.notice = "Location device is unavailable".into();
            panel.next_poll = Instant::now() + Duration::from_secs(5);
        }
        if panel
            .authentication
            .as_ref()
            .is_some_and(|(_, sent)| sent.elapsed() > Duration::from_secs(50))
        {
            panel.authentication = None;
            panel.notice = "Device authentication timed out".into();
        }
        if panel.authentication.is_none()
            && panel.pending.is_none()
            && Instant::now() >= panel.next_poll
        {
            let command = if panel.observer {
                match observer_command(&panel) {
                    Ok(command) => command,
                    Err(error) => {
                        panel.notice = error;
                        panel.watching = false;
                        world.entity_mut(entity).insert(panel);
                        draw(world, entity);
                        continue;
                    }
                }
            } else if panel.context.is_none() {
                Command::Context {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }
            } else {
                Command::View {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }
            };
            if (!panel.observer && (!panel.person.is_empty() || panel.context.is_none()))
                || (panel.observer && panel.watching)
            {
                queue(world, &mut panel, command, Vec::new());
            }
        }
        if let Some(status) = panel.status_text {
            let text = if !panel.notice.is_empty() {
                panel.notice.clone()
            } else {
                position_text(panel.view.as_ref())
            };
            controls::status(world, status, text);
        }
        world.entity_mut(entity).insert(panel);
    }
}

/// Delivers a login reply to its panel and returns whether the request ID matched.
pub fn authentication_reply(world: &mut World, id: &str, result: Result<String, String>) -> bool {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .authentication
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("location panel");
    capture(world, &mut panel);
    panel.authentication = None;
    match result {
        Ok(person) => {
            if panel.person.is_empty() {
                clear_private_actor(&mut panel);
                panel.person = person;
            }
            panel.handover = None;
            panel.notice =
                "Device authenticated. You can start or view the permitted location.".into();
            panel.context = None;
            panel.pending = None;
            panel.next_poll = Instant::now();
            panel.show_login = false;
            panel.watching = panel.observer;
        }
        Err(error) => panel.notice = error,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
    true
}
