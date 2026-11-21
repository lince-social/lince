use bevy::prelude::*;

use super::super::{Field, Intent, Panel};
use super::{button, input, label};

pub(super) fn draw(world: &mut World, entity: Entity, panel: &mut Panel) {
    if panel.transfer != panel.record {
        label(
            world,
            entity,
            "Saved place · retained independently of live sharing",
        );
        panel.fields.push((
            Field::PlaceLatitude,
            input(world, entity, "Saved latitude", &panel.place_latitude),
        ));
        panel.fields.push((
            Field::PlaceLongitude,
            input(world, entity, "Saved longitude", &panel.place_longitude),
        ));
        button(world, entity, entity, "Set saved place", Intent::SetPlace);
        panel.fields.push((
            Field::Address,
            input(
                world,
                entity,
                "Address Record (@slug or Record ID)",
                &panel.address,
            ),
        ));
        button(
            world,
            entity,
            entity,
            "Find saved addresses",
            Intent::SearchAddress,
        );
        button(
            world,
            entity,
            entity,
            "Link address Record",
            Intent::BindAddress(String::new()),
        );
        for address in &panel.addresses {
            button(
                world,
                entity,
                entity,
                &format!(
                    "Link {} · {:.5}, {:.5}",
                    address.head, address.latitude, address.longitude
                ),
                Intent::BindAddress(address.record_uid.clone()),
            );
        }
        if let Some(location) = &panel.saved_location {
            let status = if let Some(selected) = &location.selected {
                if let Some(radius) = selected.radius_metres {
                    format!("Approximate saved area · radius {radius} m")
                } else {
                    format!(
                        "Using {} · {:.5}, {:.5}",
                        if selected.assertion_uid.is_none() {
                            "this Record's coordinates"
                        } else {
                            &selected.head
                        },
                        selected.latitude,
                        selected.longitude
                    )
                }
            } else if location.unavailable {
                "The selected saved location is unavailable. Choose another explicitly.".into()
            } else if !location.candidates.is_empty() {
                "Choose which saved location to use.".into()
            } else {
                "No saved location. Set coordinates here or link a named address Record.".into()
            };
            label(world, entity, &status);
            for candidate in &location.candidates {
                let title = if candidate.assertion_uid.is_none() {
                    "Use this Record's coordinates".into()
                } else {
                    format!("Use {}", candidate.head)
                };
                button(
                    world,
                    entity,
                    entity,
                    &title,
                    Intent::SelectSaved(candidate.assertion_uid.clone()),
                );
            }
        }
        button(
            world,
            entity,
            entity,
            "Refresh saved location",
            Intent::RefreshSaved,
        );
        label(
            world,
            entity,
            "Setting a place does not start acquisition or live sharing. Manage its audience with Saved place visibility.",
        );
    } else {
        label(
            world,
            entity,
            "Set agreed meeting places in Transfer terms. Live sharing never changes those terms.",
        );
    }
}
