use super::*;

pub fn reconnect(world: &mut World) -> Result<bool, String> {
    let bridge = world.get_non_send::<CellBridge>();
    if bridge.is_some_and(|bridge| {
        !bridge.outgoing.is_closed()
            && !bridge.incoming.is_closed()
            && !bridge.closed.load(Ordering::Acquire)
    }) {
        return Ok(false);
    }
    if !world.contains_resource::<CellHandle>() {
        return Err("This Cell is unavailable.".into());
    }
    let wake = world
        .get_resource::<WakeSignal>()
        .cloned()
        .ok_or("The interface is not ready to reconnect.")?;
    tokio::runtime::Handle::try_current()
        .map_err(|_| "The local runtime is unavailable.".to_string())?;
    if bridge.is_none_or(|bridge| !bridge.reported_closed) {
        world.write_message(disconnected());
    }
    start(world);
    wake.ring();
    Ok(true)
}

pub(super) fn disconnected() -> CellMessage {
    CellMessage(ServerMessage::Error {
        id: CONNECTION.into(),
        message: "The connection to this Cell stopped. Your drafts are still here.".into(),
        code: Some("connection_closed".into()),
    })
}
