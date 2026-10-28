use super::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowRecord {
    pub placement: [u8; 16],
    pub uid: String,
    pub title: String,
    pub app_id: String,
    pub creating: bool,
}

impl WindowRecord {
    pub(crate) fn valid(&self, config: &Config) -> bool {
        if !nucleus::valid_uid(&self.uid, "r")
            || self.placement == [0; 16]
            || self.title.len() > 4096
            || self.app_id.len() > 4096
            || config.source != Source::Local
            || !config.record_cards
            || config.fiote
            || config.command.is_some()
        {
            return false;
        }
        fn matches(predicates: &[protein::Predicate], uid: &str) -> bool {
            match predicates {
                [protein::Predicate::UidEq(value) | protein::Predicate::RecordEq(value)] => {
                    value == uid
                }
                [protein::Predicate::All(children)] => matches(children, uid),
                _ => false,
            }
        }
        config.draft.compile().is_ok_and(|query| {
            query.source == protein::Source::Record
                && query.aggregate.is_none()
                && query.limit == Some(1)
                && matches(&query.filter, &self.uid)
        })
    }

    pub fn metadata(&self) -> Metadata {
        Metadata {
            placement: self.placement,
            uid: self.uid.clone(),
            title: self.title.clone(),
            app_id: self.app_id.clone(),
            live: false,
            state: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CreationState {
    Pending,
    Failed(String),
    Unavailable(String),
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct Creation {
    state: CreationState,
    retry: bool,
}

impl Creation {
    pub fn new(state: CreationState) -> Self {
        Self {
            state,
            retry: false,
        }
    }

    pub fn set_state(&mut self, state: CreationState) {
        self.state = state;
    }

    pub fn state(&self) -> &CreationState {
        &self.state
    }

    pub fn retry_requested(&self) -> bool {
        self.retry
    }

    pub fn take_retry(&mut self) -> bool {
        std::mem::take(&mut self.retry)
    }

    pub fn retry_available(&self) -> bool {
        matches!(self.state, CreationState::Failed(_)) && !self.retry
    }

    pub(crate) fn request_retry(&mut self) {
        if self.retry_available() {
            self.retry = true;
        }
    }

    pub(crate) fn message(&self) -> String {
        match &self.state {
            CreationState::Pending => "Creating window Record…".into(),
            CreationState::Failed(message) | CreationState::Unavailable(message) => {
                message.chars().take(1024).collect()
            }
        }
    }
}

pub(crate) fn restore(world: &mut World, owner: Entity, config: &Config) {
    let Some(window) = config
        .window_record
        .as_ref()
        .filter(|window| window.valid(config))
    else {
        return;
    };
    if world
        .get::<Metadata>(owner)
        .is_none_or(|metadata| metadata.uid != window.uid || metadata.placement != window.placement)
    {
        world.entity_mut(owner).insert(window.metadata());
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
}
