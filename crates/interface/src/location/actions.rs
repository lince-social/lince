use std::time::Instant;

use bevy::prelude::*;
use nucleus::location::{Command, Fix, Settings, SourceKind, Status};

use crate::controls;

use super::{
    AuthenticationRequest, CopyReferenceRequest, DraftScope, Field, HandoverReview, Intent, Panel,
    Pending, SavePlaceRequest, capture, clear_private_actor, clock::Stamp, render::draw,
    requests::queue, saved::queue_saved,
};

fn configured_settings(panel: &Panel) -> Result<Settings, String> {
    let mut settings = panel.settings.clone().ok_or("Choose your Person first")?;
    if settings.controller_uid != panel.person {
        return Err("Load settings for the selected Person first".into());
    }
    let minutes = panel
        .minutes
        .trim()
        .parse::<u32>()
        .map_err(|_| "Enter a duration in whole minutes")?;
    settings.duration_seconds = minutes.checked_mul(60).ok_or("Duration is too large")?;
    settings.transfer_uid =
        (!panel.transfer.trim().is_empty()).then(|| panel.transfer.trim().into());
    settings.validate().map_err(str::to_string)?;
    Ok(settings)
}

pub(super) fn observer_command(panel: &Panel) -> Result<Command, String> {
    if let Some(invitation) = panel
        .invitation
        .as_ref()
        .filter(|invitation| invitation.grant.is_some())
    {
        if invitation.record_uid != panel.record || invitation.authority_node_id != panel.node {
            return Err("Reload the scoped invitation before viewing a changed reference".into());
        }
        if panel.person.is_empty() {
            return Err("Authenticate the named recipient to open this invitation".into());
        }
        Ok(Command::ObserveInvitation {
            person: panel.person.clone(),
            invitation: invitation.clone(),
        })
    } else {
        Ok(Command::Observe {
            person: panel.person.clone(),
            record_uid: panel.record.clone(),
            node_id: panel.node.clone(),
        })
    }
}

pub(super) fn apply(world: &mut World, entity: Entity, intent: Intent) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    capture(world, &mut panel);
    let redraw = match operate(world, entity, &mut panel, intent) {
        Ok(redraw) => redraw,
        Err(error) => {
            panel.notice = error;
            false
        }
    };
    if let Some(status) = panel.status_text {
        controls::status(world, status, &panel.notice);
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
}

fn operate(
    world: &mut World,
    entity: Entity,
    panel: &mut Panel,
    intent: Intent,
) -> Result<bool, String> {
    if matches!(intent, Intent::Recipient(_))
        && (panel
            .view
            .as_ref()
            .is_some_and(|(view, _)| view.status != Status::Stopped)
            || panel.pending.as_ref().is_some_and(|pending| {
                matches!(
                    pending.command,
                    Command::Configure { .. }
                        | Command::Handover { .. }
                        | Command::Start { .. }
                        | Command::Approve { .. }
                )
            }))
    {
        return Err("Stop the current session before changing its source or recipients".into());
    }
    if matches!(
        intent,
        Intent::Device(_)
            | Intent::Mode
            | Intent::Start
            | Intent::Approve
            | Intent::Manual
            | Intent::ReviewHandover
            | Intent::ConfirmHandover
            | Intent::ReviewInvitation
            | Intent::SendInvitation
            | Intent::Refresh
    ) && panel.pending.is_some()
    {
        return Err("Wait for the current source request".into());
    }
    match intent {
        Intent::ReviewInvitation => {
            let recipient = panel.invite_recipient.trim().to_owned();
            let conversation = panel.invite_conversation.trim();
            if !nucleus::valid_uid(&recipient, "r") || !nucleus::valid_uid(conversation, "r") {
                return Err(
                    "Choose the recipient Person and an ordinary private conversation Record"
                        .into(),
                );
            }
            let minutes = panel
                .invite_minutes
                .trim()
                .parse::<u32>()
                .map_err(|_| "Enter an invitation lifetime in whole minutes")?;
            let duration_seconds = minutes
                .checked_mul(60)
                .ok_or("Invitation duration is too large")?;
            if duration_seconds == 0 || duration_seconds > nucleus::location::MAX_DURATION_SECONDS {
                return Err("Choose an invitation lifetime of 1–1440 minutes".into());
            }
            let radius_metres = if panel.invite_radius.trim().is_empty() {
                None
            } else {
                Some(
                    panel
                        .invite_radius
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| "Enter a radius in whole metres")?,
                )
            };
            if radius_metres.is_some_and(|radius| !nucleus::place::valid_area_radius(radius)) {
                return Err("Choose a supported geographic area radius".into());
            }
            panel.invitation_review = None;
            let command = Command::PrepareInvitation {
                person: panel.person.clone(),
                record_uid: panel.record.clone(),
                recipient,
                duration_seconds,
                radius_metres,
            };
            queue(world, panel, command, vec![]);
            Ok(false)
        }
        Intent::SendInvitation => {
            let review = panel
                .invitation_review
                .take()
                .ok_or("Review the private invitation first")?;
            if !review.scope.matches(panel)
                || review.remaining_millis <= review.received.elapsed_millis()
            {
                return Err("The invitation draft changed or expired; review it again".into());
            }
            let command = Command::SendInvitation {
                person: panel.person.clone(),
                conversation: panel.invite_conversation.trim().into(),
                invitation: review.invitation,
            };
            queue(world, panel, command, vec![]);
            Ok(true)
        }
        Intent::CancelInvitation => {
            panel.invitation_review = None;
            Ok(true)
        }
        Intent::SearchAddress
        | Intent::BindAddress(_)
        | Intent::SelectSaved(_)
        | Intent::RefreshSaved => {
            use nucleus::vocabulary::SavedLocationCommand as Saved;
            if panel.saved_pending.is_some() {
                return Err("Wait for the saved location change to finish".into());
            }
            let command = match intent {
                Intent::SearchAddress => Saved::Search {
                    text: panel.address.clone(),
                },
                Intent::BindAddress(address) => Saved::Bind {
                    record: panel.record.clone(),
                    address: if address.is_empty() {
                        panel.address.trim().into()
                    } else {
                        address
                    },
                },
                Intent::SelectSaved(assertion) => Saved::Select {
                    record: panel.record.clone(),
                    assertion,
                    expected_revision: panel
                        .saved_location
                        .as_ref()
                        .map_or(0, |location| location.revision),
                },
                _ => Saved::Inspect {
                    record: panel.record.clone(),
                },
            };
            queue_saved(world, panel, command);
            panel.notice = "Loading saved location…".into();
            Ok(false)
        }
        Intent::CopyReference => {
            let context = panel
                .context
                .as_ref()
                .ok_or("Load location controls first")?;
            let reference = nucleus::location::Invitation {
                record_uid: panel.record.clone(),
                authority_node_id: context.authority_node_id.clone(),
                grant: None,
            }
            .reference()
            .map_err(str::to_owned)?;
            world.write_message(CopyReferenceRequest {
                panel: entity,
                reference,
            });
            Ok(false)
        }
        Intent::LoadReference => {
            let invitation =
                nucleus::location::Invitation::parse(&panel.reference).map_err(str::to_owned)?;
            panel.record = invitation.record_uid.clone();
            panel.node = invitation.authority_node_id.clone();
            panel.invitation = Some(invitation);
            panel.view = None;
            panel.pending = None;
            panel.authentication = None;
            panel.password.clear();
            panel.username.clear();
            panel.show_login = false;
            panel.watching = false;
            panel.notice = "Reference loaded. View shared location to check current access.".into();
            Ok(true)
        }
        Intent::SetPlace => {
            let latitude: f64 = panel
                .place_latitude
                .trim()
                .parse()
                .map_err(|_| "Enter a valid saved latitude")?;
            let longitude: f64 = panel
                .place_longitude
                .trim()
                .parse()
                .map_err(|_| "Enter a valid saved longitude")?;
            if !latitude.is_finite()
                || !(-90.0..=90.0).contains(&latitude)
                || !longitude.is_finite()
                || !(-180.0..=180.0).contains(&longitude)
            {
                return Err("Choose valid saved coordinates".into());
            }
            let id = nucleus::new_uid("location-ui");
            world.write_message(SavePlaceRequest {
                owner: entity,
                id: id.clone(),
                record: panel.record.clone(),
                latitude,
                longitude,
            });
            panel.pending = Some(Pending {
                id,
                command: Command::View {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                },
                then: vec![],
                sent: Stamp::for_world(world),
                save_place: true,
                scope: None,
            });
            panel.notice = "Saving the Record's place without starting sharing…".into();
            Ok(false)
        }
        Intent::SavePlace => {
            if panel
                .settings
                .as_ref()
                .is_none_or(|settings| settings.controller_uid != panel.person)
            {
                return Err("Only the location controller can save this position".into());
            }
            let (view, received) = panel.view.as_ref().ok_or("Wait for a fresh position")?;
            let fix = view.fix.as_ref().ok_or("Wait for a fresh position")?;
            if view
                .remaining_millis
                .is_none_or(|remaining| remaining <= received.elapsed_millis())
                || view
                    .age_ms
                    .unwrap_or_default()
                    .saturating_add(received.elapsed_millis())
                    >= nucleus::location::MAX_FIX_AGE_MS as u64
            {
                return Err("Wait for a fresh position before saving it".into());
            }
            let id = nucleus::new_uid("location-ui");
            world.write_message(SavePlaceRequest {
                owner: entity,
                id: id.clone(),
                record: panel.record.clone(),
                latitude: fix.latitude,
                longitude: fix.longitude,
            });
            panel.pending = Some(Pending {
                id,
                command: Command::View {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                },
                then: Vec::new(),
                sent: Stamp::for_world(world),
                save_place: true,
                scope: None,
            });
            panel.notice = "Saving the Record's place…".into();
            Ok(false)
        }
        Intent::Person(person) => {
            clear_private_actor(panel);
            panel.person = person;
            panel.context = None;
            panel.next_poll = Instant::now();
            panel.pending = None;
            Ok(true)
        }
        Intent::Device(device) => {
            panel.handover = None;
            let settings = panel
                .settings
                .as_mut()
                .ok_or("Load location settings first")?;
            settings.source_cell_uid = device.uid;
            settings.source_node_id = device.node_id.ok_or("This device has no endpoint")?;
            Ok(true)
        }
        Intent::Recipient(person) => {
            let recipients = &mut panel
                .settings
                .as_mut()
                .ok_or("Load location settings first")?
                .recipients;
            if recipients.contains(&person) {
                recipients.retain(|uid| uid != &person);
            } else {
                recipients.push(person);
            }
            Ok(true)
        }
        Intent::Mode => {
            panel.handover = None;
            let settings = panel
                .settings
                .as_mut()
                .ok_or("Load location settings first")?;
            settings.source_kind = if settings.source_kind == SourceKind::Device {
                SourceKind::Manual
            } else {
                SourceKind::Device
            };
            Ok(true)
        }
        Intent::Start => {
            if panel
                .view
                .as_ref()
                .is_some_and(|(view, _)| view.status != Status::Stopped)
            {
                return Err(
                    "Stop the current session before changing its source or recipients".into(),
                );
            }
            let settings = configured_settings(panel)?;
            let then = vec![Command::Start {
                person: panel.person.clone(),
                record_uid: panel.record.clone(),
            }];
            panel.settings = Some(settings.clone());
            queue(world, panel, Command::Configure { settings }, then);
            Ok(false)
        }
        Intent::ReviewHandover => {
            if panel.pending.is_some() {
                return Err("Wait for the current request".into());
            }
            let session = panel
                .view
                .as_ref()
                .and_then(|(view, _)| view.session_uid.clone())
                .ok_or("Start a location session before handing over")?;
            let settings = configured_settings(panel)?;
            panel.handover = Some(HandoverReview {
                session,
                settings,
                scope: DraftScope::of(panel),
            });
            Ok(true)
        }
        Intent::ConfirmHandover => {
            let review = panel
                .handover
                .take()
                .ok_or("Review the replacement source first")?;
            if !review.scope.matches(panel)
                || panel
                    .view
                    .as_ref()
                    .and_then(|(view, _)| view.session_uid.as_deref())
                    != Some(review.session.as_str())
            {
                return Err("The source or private settings changed; review handover again".into());
            }
            panel.settings = Some(review.settings.clone());
            panel.view = None;
            let then = vec![Command::Context {
                person: panel.person.clone(),
                record_uid: panel.record.clone(),
            }];
            queue(
                world,
                panel,
                Command::Handover {
                    settings: review.settings,
                    expected_session_uid: review.session,
                },
                then,
            );
            panel.notice = "Changing source; approval is required on the replacement device".into();
            Ok(true)
        }
        Intent::CancelHandover => {
            panel.handover = None;
            Ok(true)
        }
        Intent::Approve => {
            let settings = panel
                .settings
                .as_ref()
                .ok_or("Load the source settings first")?;
            if !panel
                .context
                .as_ref()
                .and_then(|context| context.settings.as_ref())
                .is_some_and(|active| {
                    active.source_cell_uid == settings.source_cell_uid
                        && active.source_node_id == settings.source_node_id
                        && active.source_kind == settings.source_kind
                })
            {
                return Err("Review handover before approving a changed source".into());
            }
            let command = Command::Approve {
                person: panel.person.clone(),
                record_uid: panel.record.clone(),
                expected_session_uid: panel
                    .view
                    .as_ref()
                    .and_then(|(view, _)| view.session_uid.clone())
                    .ok_or("Load the pending source session before approving it")?,
            };
            queue(world, panel, command, Vec::new());
            Ok(false)
        }
        Intent::Stop | Intent::StopAll => {
            panel.handover = None;
            panel.invitation_review = None;
            let command = if matches!(intent, Intent::StopAll) {
                Command::StopAll {
                    person: panel.person.clone(),
                }
            } else {
                Command::Stop {
                    person: panel.person.clone(),
                    record_uid: panel.record.clone(),
                }
            };
            panel.view = None;
            panel.notice = "Stopping location…".into();
            queue(world, panel, command, Vec::new());
            Ok(false)
        }
        Intent::Manual => {
            let settings = panel
                .settings
                .as_ref()
                .ok_or("Load the source settings first")?;
            if !panel
                .context
                .as_ref()
                .and_then(|context| context.settings.as_ref())
                .is_some_and(|active| {
                    active.source_cell_uid == settings.source_cell_uid
                        && active.source_kind == SourceKind::Manual
                        && settings.source_kind == SourceKind::Manual
                })
            {
                return Err("Start and approve this manual source first".into());
            }
            let view = &panel
                .view
                .as_ref()
                .ok_or("Start this manual source first")?
                .0;
            let session_uid = view
                .session_uid
                .clone()
                .ok_or("Start this manual source first")?;
            let sequence = view
                .next_sequence
                .unwrap_or(1)
                .max(panel.manual_sequence.saturating_add(1));
            panel.manual_sequence = sequence;
            let fix = Fix {
                sequence,
                latitude: panel
                    .latitude
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a latitude")?,
                longitude: panel
                    .longitude
                    .trim()
                    .parse()
                    .map_err(|_| "Enter a longitude")?,
                accuracy_metres: None,
                captured_at_ms: chrono::Utc::now().timestamp_millis(),
                capture_age_ms: 0,
            };
            fix.validate().map_err(str::to_string)?;
            let command = Command::Publish {
                person: panel.person.clone(),
                record_uid: panel.record.clone(),
                session_uid,
                fix,
            };
            queue(world, panel, command, Vec::new());
            Ok(false)
        }
        Intent::Refresh => {
            if panel.observer {
                if (!panel.person.is_empty() && !nucleus::valid_uid(&panel.person, "r"))
                    || !nucleus::valid_uid(&panel.record, "r")
                    || panel.node.len() != 64
                    || !panel.node.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(
                        "Enter your Person and the shared Record and endpoint identities".into(),
                    );
                }
                panel.watching = true;
            }
            panel.pending = None;
            panel.next_poll = Instant::now();
            Ok(false)
        }
        Intent::Credits => {
            panel.show_credits = !panel.show_credits;
            Ok(true)
        }
        Intent::LoginForm => {
            panel.show_login = !panel.show_login;
            Ok(true)
        }
        Intent::Authenticate => {
            if panel.authentication.is_some() {
                return Err("Wait for the current device login".into());
            }
            if panel.username.trim().is_empty() || panel.password.is_empty() {
                return Err("Enter the Person's username and password".into());
            }
            let node = if panel.observer {
                panel.node.clone()
            } else {
                panel
                    .context
                    .as_ref()
                    .map_or(String::new(), |context| context.authority_node_id.clone())
            };
            let id = nucleus::new_uid("location-auth");
            world.write_message(AuthenticationRequest {
                owner: entity,
                id: id.clone(),
                node,
                record: panel.record.clone(),
                person: panel.person.clone(),
                expected_person: (!panel.observer && !panel.person.is_empty())
                    .then(|| panel.person.clone()),
                username: panel.username.trim().into(),
                password: std::mem::take(&mut panel.password),
            });
            for (field, input) in &panel.fields {
                if matches!(field, Field::Password)
                    && let Some(mut editor) = world.get_mut::<bevy::text::EditableText>(*input)
                {
                    editor.editor.set_text("");
                }
            }
            panel.authentication = Some((id, Instant::now()));
            panel.notice = "Authenticating this device…".into();
            Ok(false)
        }
    }
}
