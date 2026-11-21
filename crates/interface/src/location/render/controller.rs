use bevy::prelude::*;
use nucleus::location::{SourceKind, Status};

use crate::controls;

use super::super::{CopyReferencesAvailable, Field, Intent, Panel};
use super::{button, input, label};

pub(super) fn draw(world: &mut World, entity: Entity, panel: &mut Panel) {
    let Some(context) = &panel.context else {
        return;
    };
    if panel.person.is_empty() {
        label(world, entity, "Choose the Person controlling this location");
        for choice in &context.people {
            button(
                world,
                entity,
                entity,
                &choice.label,
                Intent::Person(choice.uid.clone()),
            );
        }
    } else {
        let name = context
            .people
            .iter()
            .find(|choice| choice.uid == panel.person)
            .map_or(panel.person.as_str(), |choice| choice.label.as_str());
        label(world, entity, &format!("Controller: {name}"));
        if world.contains_resource::<CopyReferencesAvailable>() {
            button(
                world,
                entity,
                entity,
                "Copy private location reference",
                Intent::CopyReference,
            );
        }
        label(
            world,
            entity,
            &format!(
                "Location reference: {}\nViewing endpoint: {}",
                panel.record, context.authority_node_id
            ),
        );
        if let Some(settings) = &panel.settings {
            label(world, entity, "Selected source device");
            if let Some(active) = &context.settings
                && panel
                    .view
                    .as_ref()
                    .is_some_and(|(view, _)| view.status != Status::Stopped)
            {
                let name = context
                    .devices
                    .iter()
                    .find(|device| device.uid == active.source_cell_uid)
                    .map_or(active.source_cell_uid.as_str(), |device| {
                        device.label.as_str()
                    });
                label(world, entity, &format!("Current session source: {name}"));
            }
            let row = controls::row(world, entity);
            for device in &context.devices {
                button(
                    world,
                    row,
                    entity,
                    &format!(
                        "{}{}",
                        if settings.source_cell_uid == device.uid {
                            "✓ "
                        } else {
                            ""
                        },
                        device.label
                    ),
                    Intent::Device(device.clone()),
                );
            }
            button(
                world,
                entity,
                entity,
                match settings.source_kind {
                    SourceKind::Device => "Selected source: device location",
                    SourceKind::Manual => "Selected source: manually reported location",
                },
                Intent::Mode,
            );
            panel.fields.push((
                Field::Minutes,
                input(
                    world,
                    entity,
                    "Next session duration in minutes (1–1440)",
                    &panel.minutes,
                ),
            ));
            panel.fields.push((
                Field::Transfer,
                input(
                    world,
                    entity,
                    "Next session: stop when this Transfer ends (optional UID)",
                    &panel.transfer,
                ),
            ));
            label(
                world,
                entity,
                "Named Person recipients · Organ rules apply separately",
            );
            label(
                world,
                entity,
                "Stop before changing the active source or recipients.",
            );
            for person in &context.people {
                if person.uid == panel.person {
                    continue;
                }
                button(
                    world,
                    entity,
                    entity,
                    &format!(
                        "{}{}",
                        if settings.recipients.contains(&person.uid) {
                            "✓ "
                        } else {
                            ""
                        },
                        person.label
                    ),
                    Intent::Recipient(person.uid.clone()),
                );
            }
            if settings.source_kind == SourceKind::Manual {
                panel.fields.push((
                    Field::Latitude,
                    input(world, entity, "Reported latitude", &panel.latitude),
                ));
                panel.fields.push((
                    Field::Longitude,
                    input(world, entity, "Reported longitude", &panel.longitude),
                ));
                if panel.view.as_ref().is_some_and(|(view, _)| {
                    view.source_kind == Some(SourceKind::Manual)
                        && view.status != Status::AwaitingApproval
                        && view.session_uid.is_some()
                }) && context
                    .settings
                    .as_ref()
                    .is_some_and(|active| active.source_node_id == settings.source_node_id)
                {
                    button(
                        world,
                        entity,
                        entity,
                        "Update reported position",
                        Intent::Manual,
                    );
                }
            }
            let row = controls::row(world, entity);
            button(world, row, entity, "Start location", Intent::Start);
            if panel.view.as_ref().is_some_and(|(view, _)| {
                view.session_uid.is_some() && view.status != Status::Stopped
            }) {
                button(
                    world,
                    row,
                    entity,
                    "Review source handover",
                    Intent::ReviewHandover,
                );
            }
            if settings.source_node_id == context.current_node_id
                && context.settings.as_ref().is_some_and(|active| {
                    active.source_cell_uid == settings.source_cell_uid
                        && active.source_kind == settings.source_kind
                })
                && panel
                    .view
                    .as_ref()
                    .is_some_and(|(view, _)| view.status == Status::AwaitingApproval)
            {
                button(world, row, entity, "Approve this device", Intent::Approve);
            }
            button(world, row, entity, "Stop", Intent::Stop);
            if let Some(review) = &panel.handover {
                let source = context
                    .devices
                    .iter()
                    .find(|device| device.uid == review.settings.source_cell_uid)
                    .map_or(review.settings.source_cell_uid.as_str(), |device| {
                        device.label.as_str()
                    });
                label(
                    world,
                    entity,
                    &format!(
                        "Replace the current source with {source} ({}) for {} minutes. Mode: {:?}. Recipients: {}. Transfer end condition: {}. The previous session ends; the replacement needs approval on its device. No position or approval is copied.",
                        review.settings.source_node_id,
                        review.settings.duration_seconds / 60,
                        review.settings.source_kind,
                        review.settings.recipients.join(", "),
                        review.settings.transfer_uid.as_deref().unwrap_or("none")
                    ),
                );
                button(
                    world,
                    entity,
                    entity,
                    "Confirm reviewed handover",
                    Intent::ConfirmHandover,
                );
                button(
                    world,
                    entity,
                    entity,
                    "Cancel handover review",
                    Intent::CancelHandover,
                );
            }
            if panel
                .view
                .as_ref()
                .is_some_and(|(view, _)| view.session_uid.is_some())
            {
                label(world, entity, "Private conversation invitation");
                panel.fields.push((
                    Field::InviteRecipient,
                    input(
                        world,
                        entity,
                        "Recipient Person record",
                        &panel.invite_recipient,
                    ),
                ));
                panel.fields.push((
                    Field::InviteConversation,
                    input(
                        world,
                        entity,
                        "Private conversation record",
                        &panel.invite_conversation,
                    ),
                ));
                panel.fields.push((
                    Field::InviteMinutes,
                    input(
                        world,
                        entity,
                        "Invitation lifetime in minutes",
                        &panel.invite_minutes,
                    ),
                ));
                panel.fields.push((
                    Field::InviteRadius,
                    input(
                        world,
                        entity,
                        "Minimum shared radius in metres (blank for permitted exact position)",
                        &panel.invite_radius,
                    ),
                ));
                label(
                    world,
                    entity,
                    "The reference contains no coordinates. It does not grant Record access; the named recipient must authenticate and still pass current visibility rules.",
                );
                button(
                    world,
                    entity,
                    entity,
                    "Review private invitation",
                    Intent::ReviewInvitation,
                );
            }
            if let Some(review) = &panel.invitation_review {
                let grant = review
                    .invitation
                    .grant
                    .as_ref()
                    .expect("Scoped invitation review");
                let left = review
                    .remaining_millis
                    .saturating_sub(review.received.elapsed_millis());
                label(
                    world,
                    entity,
                    &format!(
                        "Send Record {} at {} to Person {} in private conversation {}. Radius: {}. At most {} seconds remain. The ordinary conversation and delivery permissions apply.",
                        review.invitation.record_uid,
                        review.invitation.authority_node_id,
                        grant.recipient,
                        panel.invite_conversation,
                        grant.radius_metres.map_or_else(
                            || "permitted exact position".into(),
                            |radius| format!("{radius} metres")
                        ),
                        left / 1000
                    ),
                );
                button(
                    world,
                    entity,
                    entity,
                    "Send reviewed private invitation",
                    Intent::SendInvitation,
                );
                button(
                    world,
                    entity,
                    entity,
                    "Cancel invitation review",
                    Intent::CancelInvitation,
                );
            }
            if panel.transfer != panel.record {
                button(
                    world,
                    entity,
                    entity,
                    "Save current position as Record place",
                    Intent::SavePlace,
                );
            }
            label(
                world,
                entity,
                "Saving retains this position with its separate saved-place visibility policy.",
            );
        }
        button(
            world,
            entity,
            entity,
            "Stop all my location sharing",
            Intent::StopAll,
        );
    }
}
