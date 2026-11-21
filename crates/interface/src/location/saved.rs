use std::time::{Duration, Instant};

use bevy::prelude::*;
use nucleus::vocabulary::{LocationCandidate, SavedLocation, SavedLocationCommand};
use serde::Deserialize;

use crate::controls;

use super::{Panel, SavedLocationRequest, capture, render::draw};

pub(super) fn queue_saved(world: &mut World, panel: &mut Panel, command: SavedLocationCommand) {
    let id = nucleus::new_uid("saved-location-ui");
    panel.saved_pending = Some((id.clone(), command.clone(), Instant::now()));
    panel.saved_poll = Instant::now() + Duration::from_secs(10);
    world.write_message(SavedLocationRequest {
        owner: panel.owner,
        id,
        command,
    });
}

pub(super) fn receive_saved(
    world: &mut World,
    id: &str,
    data: Result<&serde_json::Value, &str>,
) -> bool {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .saved_pending
                .as_ref()
                .is_some_and(|(pending, _, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return false;
    };
    let _reply = nucleus::performance::Span::new("saved-location-reply-application");
    nucleus::performance::add("saved-location-replies-accepted", 1);
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .expect("saved location panel");
    capture(world, &mut panel);
    let Some((_, command, _)) = panel.saved_pending.take() else {
        world.entity_mut(entity).insert(panel);
        draw(world, entity);
        return true;
    };
    let mut redraw = false;
    match data {
        Err(error) => panel.notice = error.to_owned(),
        Ok(data) => {
            if matches!(command, SavedLocationCommand::Search { .. }) {
                match Vec::<LocationCandidate>::deserialize(&data["addresses"]) {
                    Ok(addresses) => {
                        panel.addresses = addresses;
                        panel.notice = "Choose an address Record with saved coordinates".into();
                        redraw = true;
                    }
                    Err(_) => panel.notice = "The saved address response was invalid".into(),
                }
            } else {
                match SavedLocation::deserialize(data) {
                    Ok(location) if location.record_uid == panel.record => {
                        redraw = panel.saved_location.as_ref() != Some(&location);
                        panel.saved_location = Some(location);
                        if !matches!(command, SavedLocationCommand::Inspect { .. }) {
                            panel.addresses.clear();
                            panel.notice = "Saved location updated".into();
                            redraw = true;
                        }
                    }
                    _ => panel.notice = "The saved location response was invalid".into(),
                }
            }
        }
    }
    panel.saved_poll = Instant::now() + Duration::from_secs(10);
    if let Some(status) = panel.status_text {
        controls::status(world, status, &panel.notice);
    }
    world.entity_mut(entity).insert(panel);
    if redraw {
        draw(world, entity);
    }
    true
}
