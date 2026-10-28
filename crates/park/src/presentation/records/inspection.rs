use super::*;
use bevy::math::DVec2;
use lince_interface::host::WindowInspection;
use native::full_record::window::{Creation, CreationState, Metadata, WindowRecord};

pub(crate) struct Inspection {
    pub target: WindowInspection,
    pub uid: String,
    pub title: String,
    pub app_id: String,
}

impl Records {
    pub(super) fn creation_state(&self, placement: [u8; 16], uid: &str) -> Option<CreationState> {
        let identity = PlacementId(placement);
        let Some(profile) = self.profile(identity) else {
            return Some(CreationState::Unavailable(
                "The original window Record association is unavailable.".into(),
            ));
        };
        if profile.uid.as_deref() == Some(uid) {
            return None;
        }
        if profile.creation.as_deref() != Some(uid) {
            return Some(CreationState::Unavailable(
                "The window Record association changed. This inspector keeps its original target."
                    .into(),
            ));
        }
        if let Some(error) = self.errors.get(&identity) {
            return Some(CreationState::Failed(format!(
                "Could not create the window Record: {error}"
            )));
        }
        if !self.intents_durable {
            return Some(CreationState::Failed("Could not save the window profile. Make its location writable, then retry Record creation.".into()));
        }
        if self.sender.as_ref().is_none_or(|sender| sender.is_closed()) {
            return Some(CreationState::Failed(
                "The local Cell is disconnected. Reconnect it, then retry Record creation.".into(),
            ));
        }
        Some(CreationState::Pending)
    }
}

impl Presentation {
    fn inspection_root(&mut self, target: WindowInspection) -> Option<Entity> {
        let root = self.canvas_root().ok()?;
        (target.valid()
            && root.to_bits() == target.owner
            && self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)?
                .entries
                .iter()
                .any(|space| space.id == target.workspace))
        .then_some(root)
    }

    pub(crate) fn inspect_window(&mut self, target: WindowInspection) -> Result<(), ProbeFailure> {
        if self.suspended() {
            return Ok(());
        }
        if let Err(error) = self.try_inspect_window(target) {
            native::notifications::report(self.app.world(), "Window properties", &error.to_string());
        }
        Ok(())
    }

    pub(in crate::presentation) fn try_inspect_window(&mut self, target: WindowInspection) -> Result<(), ProbeFailure> {
        if self.suspended() { return Err(input_failure("The Park view is inactive.")); }
        let identity = PlacementId(target.placement);
        let result = self
            .inspection_root(target)
            .ok_or("The original inspection workspace is unavailable.")
            .and_then(|_| {
                let window = self
                    .windows
                    .get(&target.surface)
                    .filter(|_| self.identities.get(&target.surface) == Some(&identity))
                    .ok_or("The selected window is no longer available.")?;
                let (title, app_id) = identity_metadata(self.app.world(), window.entity);
                self.records
                    .as_mut()
                    .ok_or("Park is not connected to a Cell.")?
                    .inspect(target, title, app_id)
            });
        result.map_err(input_failure)
    }

    pub(in crate::presentation) fn open_inspection(&mut self, inspection: Inspection) {
        let Some(root) = self.inspection_root(inspection.target) else {
            native::notifications::report(
                self.app.world(),
                "Window properties",
                "The original inspection workspace is unavailable.",
            );
            return;
        };
        let Some(area) = native::full_record::open_in(
            self.app.world_mut(),
            root,
            inspection.target.workspace,
            DVec2::from_array(inspection.target.position),
            &inspection.uid,
            native::protein_area::Source::Local,
        ) else {
            return;
        };
        let state = self.records.as_ref().map_or_else(
            || {
                Some(CreationState::Unavailable(
                    "Window Record creation requires its original connected Park session.".into(),
                ))
            },
            |records| records.creation_state(inspection.target.placement, &inspection.uid),
        );
        let association = WindowRecord {
            placement: inspection.target.placement,
            uid: inspection.uid,
            title: inspection.title.chars().take(1024).collect(),
            app_id: inspection.app_id.chars().take(1024).collect(),
            creating: state.is_some(),
        };
        self.app
            .world_mut()
            .entity_mut(area)
            .insert(association.metadata());
        self.app
            .world_mut()
            .get_mut::<native::area::InfluenceArea>(area)
            .unwrap()
            .protein
            .as_mut()
            .unwrap()
            .window_record = Some(association);
        if let Some(state) = state {
            self.app
                .world_mut()
                .entity_mut(area)
                .insert(Creation::new(state));
        }
    }

    pub(in crate::presentation) fn refresh_inspections(&mut self) {
        let pending: Vec<_> = self
            .app
            .world_mut()
            .query::<(Entity, &native::area::InfluenceArea)>()
            .iter(self.app.world())
            .filter_map(|(entity, area)| {
                let config = area.protein.as_ref()?;
                let window = config
                    .window_record
                    .as_ref()
                    .filter(|window| window.creating && config.valid())?;
                Some((entity, window.clone()))
            })
            .collect();
        for (area, window) in pending {
            let retry = self
                .app
                .world()
                .get::<Creation>(area)
                .is_some_and(Creation::retry_requested);
            if retry {
                self.app
                    .world_mut()
                    .get_mut::<Creation>(area)
                    .unwrap()
                    .take_retry();
                if let Some(records) = self.records.as_mut()
                    && records
                        .profile(PlacementId(window.placement))
                        .is_some_and(|profile| {
                            profile.creation.as_deref() == Some(window.uid.as_str())
                        })
                {
                    records.retry(PlacementId(window.placement));
                    if let Some(wake) = self.app.world().get_resource::<WakeSignal>() {
                        wake.ring();
                    }
                }
            }
            let state = self.records.as_ref().map_or_else(
                || {
                    Some(CreationState::Unavailable(
                        "Window Record creation requires its original connected Park session."
                            .into(),
                    ))
                },
                |records| records.creation_state(window.placement, &window.uid),
            );
            match state {
                None => {
                    self.app
                        .world_mut()
                        .get_mut::<native::area::InfluenceArea>(area)
                        .unwrap()
                        .protein
                        .as_mut()
                        .unwrap()
                        .window_record
                        .as_mut()
                        .unwrap()
                        .creating = false;
                    self.app.world_mut().entity_mut(area).remove::<Creation>();
                }
                Some(state) => {
                    if self.app.world().get::<Creation>(area).is_none() {
                        self.app
                            .world_mut()
                            .entity_mut(area)
                            .insert(Creation::new(state));
                    } else if self.app.world().get::<Creation>(area).unwrap().state() != &state {
                        self.app
                            .world_mut()
                            .get_mut::<Creation>(area)
                            .unwrap()
                            .set_state(state);
                    }
                }
            }
        }
        let inspections: Vec<_> = self
            .app
            .world_mut()
            .query::<(Entity, &Metadata)>()
            .iter(self.app.world())
            .map(|(entity, metadata)| (entity, metadata.clone()))
            .collect();
        for (area, mut metadata) in inspections {
            let identity = PlacementId(metadata.placement);
            let live = self
                .records
                .as_ref()
                .and_then(|records| records.profile(identity))
                .is_some_and(|profile| {
                    profile.uid.as_ref().or(profile.creation.as_ref()) == Some(&metadata.uid)
                });
            let window = live
                .then(|| {
                    self.identities
                        .iter()
                        .find(|(_, id)| **id == identity)
                        .and_then(|(surface, _)| self.windows.get(surface))
                })
                .flatten();
            metadata.live = window.is_some();
            if let Some(window) = window {
                (metadata.title, metadata.app_id) =
                    identity_metadata(self.app.world(), window.entity);
                metadata.state = live_state(self.app.world(), window.entity, identity);
            }
            if self.app.world().get::<Metadata>(area) != Some(&metadata) {
                self.app.world_mut().entity_mut(area).insert(metadata);
            }
        }
    }
}

fn identity_metadata(world: &World, entity: Entity) -> (String, String) {
    let properties = world.get::<native::area::RecordProperties>(entity);
    let text = |name: &str| {
        properties
            .and_then(|properties| properties.0[name].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let title = properties
        .and_then(|properties| {
            properties
                .0
                .get("window_title")
                .or_else(|| properties.0.get("head"))
        })
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    (title, text("app_id"))
}

fn live_state(world: &World, entity: Entity, identity: PlacementId) -> String {
    let Some(parent) = world.get::<ChildOf>(entity) else {
        return "available".into();
    };
    let Some(member) = world.get::<native::workspace::WorkspaceMember>(entity) else {
        return "available".into();
    };
    let Some(space) = world
        .get::<native::workspace::Workspaces>(parent.parent())
        .and_then(|spaces| spaces.entries.iter().find(|space| space.id == member.0))
    else {
        return "available".into();
    };
    if space.desktop.minimized(identity) {
        return "minimized".into();
    }
    if space.desktop.fullscreen() == Some(identity) {
        return "fullscreen".into();
    }
    if space.desktop.maximized(identity) {
        return "maximized".into();
    }
    if space
        .desktop
        .items()
        .iter()
        .any(|item| item.id == identity && item.floating)
    {
        return "floating".into();
    }
    if space.desktop.mode() == lince_interface::host::WorkspaceLayout::Tiled {
        "tiled".into()
    } else {
        "free canvas".into()
    }
}

#[cfg(test)]
mod tests;
