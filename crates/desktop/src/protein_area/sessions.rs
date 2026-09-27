use super::*;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Connecting,
    Locked,
    SigningIn,
    Ready,
    Disconnected,
}

pub(super) struct Session {
    remote: Option<Remote>,
    pub(super) phase: Phase,
    status: String,
    retry_at: Option<Instant>,
    subscriptions: HashSet<String>,
    pending: VecDeque<ClientMessage>,
}

impl Session {
    pub(super) fn connected(remote: Remote) -> Self {
        Self {
            remote: Some(remote),
            phase: Phase::Connecting,
            status: "Connecting".into(),
            retry_at: None,
            subscriptions: Default::default(),
            pending: Default::default(),
        }
    }

    fn open(world: &World, organ: &str) -> Self {
        match connect_organ(world, organ) {
            Ok(remote) => Self::connected(remote),
            Err(error) => {
                retry_wake(world);
                Self {
                    remote: None,
                    phase: Phase::Disconnected,
                    status: error,
                    retry_at: Some(Instant::now() + Duration::from_secs(5)),
                    subscriptions: Default::default(),
                    pending: Default::default(),
                }
            }
        }
    }
}

pub(super) fn attach(world: &mut World, organ: &str, state: &mut State) {
    if !world.resource::<Runtime>().sessions.contains_key(organ) {
        let session = Session::open(world, organ);
        world
            .resource_mut::<Runtime>()
            .sessions
            .insert(organ.into(), session);
    }
    let session = &world.resource::<Runtime>().sessions[organ];
    state.remote = Some(organ.into());
    state.ready = session.phase == Phase::Ready;
    state.login = matches!(session.phase, Phase::Locked | Phase::SigningIn);
    state.login_pending = session.phase == Phase::SigningIn;
    state.status = session.status.clone();
}

pub(super) fn sender(
    world: &World,
    organ: &str,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    let session = world.get_resource::<Runtime>()?.sessions.get(organ)?;
    (session.phase == Phase::Ready).then_some(())?;
    let remote = session.remote.as_ref()?;
    (!remote.outgoing.is_closed()).then(|| remote.outgoing.clone())
}

pub(super) fn unsubscribe(world: &mut World, organ: &str, id: String) {
    if let Some(session) = world.resource_mut::<Runtime>().sessions.get_mut(organ) {
        if session.subscriptions.remove(&id) {
            session.pending.push_back(ClientMessage::Unsubscribe { id });
        }
    }
}

pub(super) fn subscribed(world: &mut World, organ: &str, message: &ClientMessage) {
    if let ClientMessage::Subscribe { id, .. } = message {
        if let Some(session) = world.resource_mut::<Runtime>().sessions.get_mut(organ) {
            session.subscriptions.insert(id.clone());
        }
    }
}

pub(super) fn login(
    world: &mut World,
    organ: &str,
    username: String,
    password: String,
) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let session = runtime
        .sessions
        .get_mut(organ)
        .ok_or("Connect to the Organ first.")?;
    if session.phase != Phase::Locked {
        return Err("Wait for the Organ to request a login.".into());
    }
    session
        .remote
        .as_ref()
        .ok_or("Connection closed. Connect again.")?
        .outgoing
        .try_send(ClientMessage::LiveLogin { username, password })
        .map_err(|error| match error {
            tokio::sync::mpsc::error::TrySendError::Full(_) => "Connection busy. Try again.",
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                "Connection closed. Connect again."
            }
        })?;
    session.phase = Phase::SigningIn;
    session.status = "Waiting for the Organ to accept the login.".into();
    let mut owners = Vec::new();
    for (owner, state) in &mut runtime.areas {
        if state.remote.as_deref() == Some(organ) {
            state.login_pending = true;
            state.status = "Waiting for the Organ to accept the login.".into();
            owners.push(*owner);
        }
    }
    drop(runtime);
    for owner in owners {
        login::clear_passwords(world, owner);
    }
    Ok(())
}

pub(super) fn reconnect(world: &mut World, organ: &str) {
    if !world
        .resource::<Runtime>()
        .sessions
        .get(organ)
        .is_some_and(|s| s.phase == Phase::Disconnected)
    {
        return;
    }
    let session = Session::open(world, organ);
    world
        .resource_mut::<Runtime>()
        .sessions
        .insert(organ.into(), session);
    let owners: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .iter()
        .filter(|(_, state)| state.remote.as_deref() == Some(organ))
        .filter_map(|(owner, state)| state.applied.clone().map(|config| (*owner, config)))
        .collect();
    for (owner, config) in owners {
        start(world, owner, config);
    }
}

pub(super) fn receive(world: &mut World, organ: &str, message: ServerMessage) {
    let Some(session) = world
        .resource_mut::<Runtime>()
        .into_inner()
        .sessions
        .get_mut(organ)
    else {
        return;
    };
    let failed = matches!(&message, ServerMessage::Error { id, .. } if id == "connection");
    match &message {
        ServerMessage::LiveHello {
            login_required: true,
        } => {
            session.phase = Phase::Locked;
            session.status = "Login required".into();
        }
        ServerMessage::SessionAuthenticated { .. } => {
            session.phase = Phase::Ready;
            session.status = "Loading".into();
            session.retry_at = None;
        }
        ServerMessage::Error { message, .. } if failed => {
            session.phase = Phase::Disconnected;
            session.status = message.clone();
            session.remote = None;
            session.pending.clear();
            session.subscriptions.clear();
            session.retry_at = Some(Instant::now() + Duration::from_secs(5));
        }
        _ if session.phase != Phase::Ready => return,
        _ => {}
    }
    observe(world, &Source::Organ(organ.into()), &message);
    let owners: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .iter()
        .filter(|(_, state)| state.remote.as_deref() == Some(organ))
        .filter(|(_, state)| {
            failed
                || match &message {
                    ServerMessage::SessionAuthenticated { .. }
                    | ServerMessage::LiveHello { .. } => true,
                    ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } => {
                        state.subscription.as_ref() == Some(id)
                    }
                    ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => {
                        state.actions.contains_key(id) || state.subscription.as_ref() == Some(id)
                    }
                    _ => false,
                }
        })
        .map(|(owner, _)| *owner)
        .collect();
    for owner in owners {
        if failed {
            login::clear_passwords(world, owner);
        }
        super::receive(world, owner, message.clone());
    }
    if let ServerMessage::Error { message, .. } = &message {
        if failed {
            crate::notifications::report(world, "Protein Area", message);
            retry_wake(world);
        }
    }
}

pub(super) fn disconnected(world: &mut World, organ: &str) {
    receive(
        world,
        organ,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Connection closed".into(),
            code: None,
        },
    );
}

pub(super) fn update(world: &mut World) {
    let retained: HashSet<_> = world
        .resource::<Runtime>()
        .areas
        .values()
        .filter_map(
            |state| match state.applied.as_ref().map(|config| &config.source) {
                Some(Source::Organ(organ)) => Some(organ.clone()),
                _ => None,
            },
        )
        .collect();
    world
        .resource_mut::<Runtime>()
        .sessions
        .retain(|organ, _| retained.contains(organ));
    let retry: Vec<_> = world
        .resource::<Runtime>()
        .sessions
        .iter()
        .filter(|(organ, session)| {
            session.retry_at.is_some_and(|at| at <= Instant::now())
                && world
                    .resource::<Runtime>()
                    .areas
                    .values()
                    .any(|state| state.remote.as_ref() == Some(*organ))
        })
        .map(|(organ, _)| organ.clone())
        .collect();
    for organ in retry {
        reconnect(world, &organ);
    }
    let organs: Vec<_> = world
        .resource::<Runtime>()
        .sessions
        .keys()
        .cloned()
        .collect();
    for organ in organs {
        let mut messages = Vec::new();
        let mut closed = false;
        if let Some(remote) = world
            .resource_mut::<Runtime>()
            .sessions
            .get_mut(&organ)
            .and_then(|s| s.remote.as_mut())
        {
            for _ in 0..64 {
                match remote.incoming.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                        closed = true;
                        break;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                }
            }
        }
        let failed = messages.iter().any(
            |message| matches!(message, ServerMessage::Error { id, .. } if id == "connection"),
        );
        for message in messages {
            receive(world, &organ, message);
        }
        if closed && !failed {
            disconnected(world, &organ);
        }
        let session = world
            .resource_mut::<Runtime>()
            .into_inner()
            .sessions
            .get_mut(&organ)
            .unwrap();
        if session.phase != Phase::Ready {
            continue;
        }
        let mut closed = false;
        while let Some(message) = session.pending.pop_front() {
            match session.remote.as_ref().unwrap().outgoing.try_send(message) {
                Ok(()) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(message)) => {
                    session.pending.push_front(message);
                    break;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    closed = true;
                    break;
                }
            }
        }
        if !session.pending.is_empty() {
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.ring();
            }
        }
        if closed {
            disconnected(world, &organ);
        }
    }
}

#[cfg(test)]
mod tests;
