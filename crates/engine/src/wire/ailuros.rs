use super::Wire;
use crate::{
    Engine, EngineError,
    ailuros::{self, StreamProof},
    signal_streams::{
        ingress::{Credential, PublisherSession},
        invalid,
    },
};
use ailuros_protocol::{Codec, Frame, HEADER_BYTES, Kind, Phase};
use futures::{SinkExt, StreamExt};
use iroh::endpoint::{Connection, SendStream};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio_util::codec::{FramedRead, FramedWrite};

struct SessionConnection(Connection);

impl Drop for SessionConnection {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"hardware session ended");
    }
}

async fn control(
    writer: &mut FramedWrite<SendStream, Codec>,
    value: Value,
) -> Result<(), EngineError> {
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid("Invalid hardware reply"))?;
    tokio::time::timeout(
        Duration::from_secs(2),
        writer.send(Frame::new(Kind::Control, bytes)),
    )
    .await
    .map_err(|_| invalid("Hardware peer stopped reading"))?
    .map_err(|_| invalid("Hardware connection closed"))
}

async fn enabled(engine: &Engine) -> Result<(), EngineError> {
    let settings = engine.ailuros_receiver_settings().await?;
    if settings.enabled && settings.iroh {
        Ok(())
    } else {
        Err(EngineError::Forbidden(
            "Hardware iroh receiver disabled".into(),
        ))
    }
}

impl Wire {
    pub(super) async fn serve_ailuros_connection(
        &self,
        connection: Connection,
    ) -> Result<(), EngineError> {
        let closing = SessionConnection(connection);
        enabled(&self.engine).await?;
        let auth = self
            .ailuros_auth_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| invalid("Hardware authentication busy"))?;
        let (send, receive) = tokio::time::timeout(
            Duration::from_secs(ailuros::AUTH_TIMEOUT_SECONDS),
            closing.0.open_bi(),
        )
        .await
        .map_err(|_| invalid("Hardware stream timed out"))?
        .map_err(|_| invalid("Hardware connection closed"))?;
        let mut reader = FramedRead::with_capacity(receive, Codec::default(), HEADER_BYTES);
        let mut writer = FramedWrite::new(send, Codec::default());
        let (audience, challenge) = self.engine.ailuros_stream_challenge().await?;
        control(
            &mut writer,
            json!({"type":"challenge","audience":audience,"challenge":challenge,
            "expires_in":ailuros::AUTH_TIMEOUT_SECONDS,"protocol":"lince-ailuros-stream-v1",
            "server_time":nucleus::execution::now().timestamp()}),
        )
        .await?;
        let identity =
            tokio::time::timeout(Duration::from_secs(ailuros::AUTH_TIMEOUT_SECONDS), async {
                let frame = reader
                    .next()
                    .await
                    .ok_or_else(|| invalid("Missing hardware proof"))?
                    .map_err(|_| invalid("Invalid hardware authentication frame"))?;
                if frame.kind != Kind::Control {
                    return Err(invalid("Hardware proof required"));
                }
                let proof: StreamProof = serde_json::from_slice(&frame.payload)
                    .map_err(|_| invalid("Invalid hardware proof"))?;
                self.engine
                    .authenticate_ailuros_stream(&proof, &challenge, &proof.signal)
                    .await
            })
            .await
            .map_err(|_| invalid("Hardware authentication timed out"))??;
        let mut session = PublisherSession::open(
            &self.engine,
            identity.signal(),
            Credential::Device(identity.clone()),
        )
        .await?;
        drop(auth);
        reader.decoder_mut().set_phase(Phase::Streaming);
        writer.encoder_mut().set_phase(Phase::Streaming);
        control(
            &mut writer,
            json!({"type":"authenticated","max_frame_bytes":ailuros::STREAM_MAX_PAYLOAD,
            "max_fps":ailuros::STREAM_MAX_FPS,"max_width":1920,"max_height":1080}),
        )
        .await?;
        let mut demand = session.demand_changes();
        let mut authority = self.engine.watch_query_changes();
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            if let Some(message) = session.demand()? {
                control(&mut writer, message).await?;
            }
            tokio::select! {
                _ = authority.changed() => {
                    enabled(&self.engine).await?;
                    session.authorize(&self.engine).await?;
                }
                _ = demand.changed() => {}
                _ = timer.tick() => {
                    if session.expired() { return Err(invalid("Hardware heartbeat timed out")); }
                    enabled(&self.engine).await?;
                    session.authorize(&self.engine).await?;
                    control(&mut writer, json!({"type":"ping"})).await?;
                }
                frame = reader.next() => {
                    let Some(frame) = frame else { return Ok(()) };
                    let frame = frame.map_err(|_| invalid("Invalid hardware stream frame"))?;
                    session.received(Instant::now())?;
                    match frame.kind {
                        Kind::Control => {
                            let message: Value = serde_json::from_slice(&frame.payload).map_err(|_| invalid("Invalid hardware control"))?;
                            if message.as_object().is_none_or(|fields| fields.len() != 1) || message["type"] != "pong" {
                                return Err(invalid("Unexpected hardware control"));
                            }
                        }
                        Kind::Metadata => {
                            let text = std::str::from_utf8(&frame.payload).map_err(|_| invalid("Invalid hardware metadata"))?;
                            session.metadata(text)?;
                        }
                        Kind::Data => {
                            let receipt = session.publish(&self.engine, &frame.payload).await?;
                            control(&mut writer, receipt).await?;
                        }
                    }
                }
                _ = closing.0.accept_bi() => return Err(invalid("Unexpected hardware stream")),
                _ = closing.0.accept_uni() => return Err(invalid("Unexpected hardware stream")),
            }
        }
    }
}
