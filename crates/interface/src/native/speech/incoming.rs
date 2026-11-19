use super::*;
use crate::native::cell_bridge::{CONNECTION, CellMessage, replies};
use bevy::ecs::{message::MessageCursor, query::QueryState};

pub(crate) struct Receiver {
    wave: replies::Wave,
    query: QueryState<(Entity, &'static Composer)>,
    owners: Vec<Entity>,
}

impl FromWorld for Receiver {
    fn from_world(world: &mut World) -> Self {
        Self {
            wave: Default::default(),
            query: world.query(),
            owners: Vec::new(),
        }
    }
}

impl Receiver {
    pub(super) fn receive(&mut self, world: &mut World, cursor: &mut MessageCursor<CellMessage>) {
        let _span = nucleus::performance::Span::new("speech-reply-delivery");
        let Some(messages) = world.get_resource::<Messages<CellMessage>>() else {
            return;
        };
        if self.query.iter(world).next().is_none() {
            cursor.clear(messages);
            self.wave = Default::default();
            self.owners = Vec::new();
            return;
        }
        self.wave.capture(cursor, messages, |message| {
            matches!(
                message,
                cell::ServerMessage::Speech { .. } | cell::ServerMessage::Error { .. }
            )
        });
        for sequence in self.wave.drain() {
            let event = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
            if let cell::ServerMessage::Error { id, .. } = event.as_ref()
                && id == CONNECTION
            {
                self.owners.clear();
                self.owners
                    .extend(self.query.iter(world).map(|(owner, _)| owner));
                if self.owners.is_empty() {
                    continue;
                }
                let event = event.clone();
                nucleus::performance::add("speech-replies-retained", 1);
                let cell::ServerMessage::Error { message, .. } = event.as_ref() else {
                    unreachable!()
                };
                for owner in self.owners.drain(..) {
                    world.entity_mut(owner).remove::<Recording>();
                    let mut composer = world.get_mut::<Composer>(owner).unwrap();
                    composer.pending = None;
                    composer.job = None;
                    if let Some(saved) = composer.saved.as_mut() {
                        saved.ready = false;
                    }
                    let status = composer.status;
                    panel::status(world, status, format!("Dictation stopped: {message}"));
                }
                continue;
            }
            let id = match event.as_ref() {
                cell::ServerMessage::Speech { id, .. } | cell::ServerMessage::Error { id, .. } => {
                    id
                }
                _ => unreachable!(),
            };
            let owner = self
                .query
                .iter(world)
                .find(|(_, composer)| composer.pending.as_deref() == Some(id.as_str()))
                .map(|(entity, _)| entity);
            let Some(owner) = owner else { continue };
            let event = event.clone();
            nucleus::performance::add("speech-replies-retained", 1);
            let result = match event.as_ref() {
                cell::ServerMessage::Speech { status, .. } => Ok(status),
                cell::ServerMessage::Error { message, .. } => Err(message),
                _ => unreachable!(),
            };
            apply(world, owner, result);
        }
    }

    pub(super) fn owners(&mut self, world: &World) -> std::vec::Drain<'_, Entity> {
        self.owners.clear();
        self.owners
            .extend(self.query.iter(world).map(|(owner, _)| owner));
        if self.owners.is_empty() {
            self.owners = Vec::new();
        }
        self.owners.drain(..)
    }
}

fn apply(world: &mut World, owner: Entity, result: Result<&Status, &String>) {
    let mut composer = world.get_mut::<Composer>(owner).unwrap();
    composer.pending = None;
    let status_entity = composer.status;
    match result {
        Err(error) => {
            composer.job = None;
            panel::status(world, status_entity, error);
        }
        Ok(saved) => {
            if let Some(local) = &saved.local {
                let selected = if saved.mode == cell::speech::Mode::Local {
                    "selected"
                } else {
                    "available"
                };
                let detail = if local.busy {
                    "working"
                } else if local.loaded {
                    "loaded"
                } else if local.present {
                    "installed; loads on first use"
                } else {
                    "model not installed"
                };
                let label = composer.local_status;
                drop(composer);
                panel::status(
                    world,
                    label,
                    format!(
                        "{} · {selected} · {detail} · {} / {} MB acquired",
                        saved.local_selection.label(),
                        local.acquired_bytes / 1_000_000,
                        local.required_bytes / 1_000_000
                    ),
                );
                composer = world.get_mut::<Composer>(owner).unwrap();
            }
            if let Some(job) = saved
                .job
                .as_ref()
                .filter(|job| composer.job.as_deref() == Some(&job.id))
            {
                if !job.pending {
                    composer.job = None;
                    let setup = job.purpose != cell::speech::Purpose::Transcription;
                    if !setup {
                        composer.transcript = job.text.clone();
                    }
                    if let Some(error) = &job.error {
                        panel::status(world, status_entity, error);
                    } else if setup {
                        panel::status(
                            world,
                            status_entity,
                            job.text.as_deref().unwrap_or(&saved.detail),
                        );
                    } else if let Err(error) = insert(world, owner) {
                        panel::status(world, status_entity, error);
                    }
                    let _ = panel::send(
                        world,
                        owner,
                        cell::ClientMessage::Speech {
                            id: nucleus::new_uid("speech-release"),
                            request: Request::Cancel {
                                job: job.id.clone(),
                            },
                        },
                    );
                } else {
                    composer.next = Instant::now() + Duration::from_millis(250);
                    panel::status(
                        world,
                        status_entity,
                        if job.purpose == cell::speech::Purpose::ModelSetup {
                            "Preparing local English speech · your draft is unchanged"
                        } else if saved
                            .local
                            .as_ref()
                            .is_some_and(|local| local.busy && !local.loaded)
                        {
                            "Loading local English speech · your message remains unsent"
                        } else {
                            "Transcribing · your message remains unsent"
                        },
                    );
                }
                world.get_mut::<Composer>(owner).unwrap().saved = Some(saved.clone());
            } else {
                let cloud_label = composer.cloud_label;
                composer.cloud = saved
                    .endpoint
                    .as_ref()
                    .is_some_and(|endpoint| endpoint.allow_cloud);
                composer.saved = Some(saved.clone());
                let endpoint_fields = composer.endpoint_fields;
                let key_label = composer.key_label;
                if let Some(endpoint) = &saved.endpoint {
                    composer.requires_key = endpoint.requires_key;
                    let enabled = endpoint.requires_key;
                    for (field, value) in endpoint_fields[..2]
                        .iter()
                        .zip([&endpoint.url, &endpoint.model])
                    {
                        world
                            .get_mut::<EditableText>(*field)
                            .unwrap()
                            .editor
                            .set_text(value);
                    }
                    panel::status(
                        world,
                        key_label,
                        if enabled {
                            "Endpoint API key required: yes"
                        } else {
                            "Endpoint API key required: no"
                        },
                    );
                }
                panel::status(
                    world,
                    cloud_label,
                    if saved
                        .endpoint
                        .as_ref()
                        .is_some_and(|endpoint| endpoint.allow_cloud)
                    {
                        "Cloud transcription: allowed"
                    } else {
                        "Cloud transcription: off"
                    },
                );
                panel::status(world, status_entity, &saved.detail);
            }
        }
    }
}
