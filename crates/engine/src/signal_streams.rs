pub mod ingress;
mod source;

use crate::{Engine, EngineError};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::watch;

pub const MAX_PAYLOAD: usize = 2 * 1024 * 1024;
pub const MAX_SIGNALS: usize = 32;
pub const MAX_CONSUMERS: usize = 32;

pub fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "signal_stream_invalid",
        message: message.into(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub epoch: String,
    pub sequence: u64,
    pub captured_at: i64,
    pub content_type: String,
}

#[derive(Clone, Debug)]
pub struct Value {
    pub metadata: Metadata,
    pub bytes: Arc<[u8]>,
    pub received: Instant,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub epoch: String,
    pub consumers: usize,
    pub status: String,
    pub value: Option<Arc<Value>>,
}

struct Inner {
    closed: bool,
    publisher: bool,
    sequence: u64,
    source_started: bool,
}

pub struct Channel {
    pub config: store::signal_streams::Configuration,
    state: watch::Sender<Snapshot>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
pub struct Runtime {
    channels: Mutex<HashMap<String, Arc<Channel>>>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for channel in self.channels.get_mut().unwrap().values() {
            channel.close();
        }
    }
}

pub struct Subscription {
    channel: Arc<Channel>,
    pub updates: watch::Receiver<Snapshot>,
}

pub struct Publisher {
    channel: Arc<Channel>,
    retain_on_drop: bool,
    pub demand: watch::Receiver<Snapshot>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut inner = self.channel.inner.lock().unwrap();
        if inner.closed {
            return;
        }
        self.channel.state.send_modify(|state| {
            state.consumers = state.consumers.saturating_sub(1);
            if state.consumers == 0 {
                state.epoch = nucleus::new_uid("stream");
                state.value = None;
                state.status = "Idle".into();
                inner.sequence = 0;
            }
        });
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        let mut inner = self.channel.inner.lock().unwrap();
        inner.publisher = false;
        if inner.closed {
            return;
        }
        if self.retain_on_drop {
            return;
        }
        self.channel.state.send_modify(|state| {
            state.value = None;
            state.epoch = nucleus::new_uid("stream");
            inner.sequence = 0;
            state.status = if state.consumers == 0 {
                "Idle"
            } else {
                "Source disconnected"
            }
            .into();
        });
    }
}

impl Channel {
    fn new(config: store::signal_streams::Configuration) -> Self {
        let (state, _) = watch::channel(Snapshot {
            epoch: nucleus::new_uid("stream"),
            consumers: 0,
            status: "Idle".into(),
            value: None,
        });
        Self {
            config,
            state,
            inner: Mutex::new(Inner {
                closed: false,
                publisher: false,
                sequence: 0,
                source_started: false,
            }),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.borrow().clone()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.lock().unwrap().closed
    }

    fn close(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.closed = true;
        self.state.send_modify(|state| {
            state.epoch = nucleus::new_uid("stream");
            state.consumers = 0;
            state.value = None;
            state.status = "Configuration changed".into();
        });
    }

    pub fn subscribe(self: &Arc<Self>) -> Result<Subscription, EngineError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Err(invalid("Stream configuration changed"));
        }
        if self.snapshot().consumers >= MAX_CONSUMERS {
            return Err(invalid("Too many stream consumers"));
        }
        self.state.send_modify(|state| {
            if state.consumers == 0 {
                state.epoch = nucleus::new_uid("stream");
                inner.sequence = 0;
                state.status = "Waiting for source".into();
            }
            state.consumers += 1;
        });
        drop(inner);
        source::start(self.clone());
        Ok(Subscription {
            channel: self.clone(),
            updates: self.state.subscribe(),
        })
    }

    pub fn publisher(self: &Arc<Self>) -> Result<Publisher, EngineError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Err(invalid("Stream configuration changed"));
        }
        if inner.publisher {
            return Err(invalid("This Signal already has a producer"));
        }
        inner.publisher = true;
        Ok(Publisher {
            channel: self.clone(),
            retain_on_drop: false,
            demand: self.state.subscribe(),
        })
    }
}

impl Publisher {
    pub fn snapshot(&self) -> Snapshot {
        self.channel.snapshot()
    }

    pub async fn publish(&self, metadata: Metadata, bytes: Vec<u8>) -> Result<bool, EngineError> {
        if self.channel.is_closed() {
            return Err(invalid("Stream configuration changed"));
        }
        if bytes.len() > MAX_PAYLOAD || metadata.sequence == 0 {
            return Err(invalid("Invalid payload size or sequence"));
        }
        let before = self.snapshot();
        if before.consumers == 0 {
            return Ok(false);
        }
        if metadata.epoch != before.epoch {
            return Err(invalid("Expired stream epoch"));
        }
        let expected = match self.channel.config.value_type.as_str() {
            "scalar" => "text/decimal",
            _ => "image/jpeg",
        };
        if metadata.content_type != expected {
            return Err(invalid("Unsupported content type for Signal"));
        }
        if expected == "text/decimal" {
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid decimal"))?;
            if text.len() > 64 {
                return Err(invalid("Decimal too long"));
            }
            let number = nucleus::DecimalValue::parse_inferred(text)
                .map_err(|_| invalid("Invalid decimal"))?;
            if number.to_string() != text {
                return Err(invalid("Use a canonical decimal"));
            }
        } else {
            let payload = bytes.clone();
            tokio::task::spawn_blocking(move || decode_jpeg(&payload).map(|_| ()))
                .await
                .map_err(|_| invalid("Image worker failed"))??;
        }
        let mut inner = self.channel.inner.lock().unwrap();
        if inner.closed {
            return Err(invalid("Stream configuration changed"));
        }
        let current = self.snapshot();
        if current.consumers == 0 {
            return Ok(false);
        }
        if metadata.epoch != current.epoch || metadata.sequence <= inner.sequence {
            return Err(invalid("Expired epoch or repeated sequence"));
        }
        inner.sequence = metadata.sequence;
        self.channel.state.send_modify(|state| {
            state.status = "Live".into();
            state.value = Some(Arc::new(Value {
                metadata,
                bytes: bytes.into(),
                received: Instant::now(),
            }));
        });
        Ok(true)
    }
}

pub fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), EngineError> {
    if bytes.len() > MAX_PAYLOAD {
        return Err(invalid("Frame too large"));
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1920);
    limits.max_image_height = Some(1080);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| invalid("Invalid JPEG frame"))?
        .into_rgba8();
    Ok((decoded.width(), decoded.height(), decoded.into_raw()))
}

impl Runtime {
    pub fn scalar(&self, signal: &str) -> Result<nucleus::DecimalValue, EngineError> {
        let channels = self.channels.lock().unwrap();
        let value = channels
            .get(signal)
            .and_then(|channel| channel.snapshot().value)
            .ok_or_else(|| invalid("Memory-only Signal has no current value"))?;
        if value.metadata.content_type != "text/decimal" {
            return Err(invalid("Signal is not numeric"));
        }
        nucleus::DecimalValue::parse_inferred(
            std::str::from_utf8(&value.bytes).map_err(|_| invalid("Invalid numeric value"))?,
        )
        .map_err(|_| invalid("Invalid numeric value"))
    }
    pub fn invalidate(&self, signal: &str) {
        if let Some(channel) = self.channels.lock().unwrap().remove(signal) {
            channel.close();
        }
    }

    fn channel(
        &self,
        config: store::signal_streams::Configuration,
    ) -> Result<Arc<Channel>, EngineError> {
        let mut channels = self.channels.lock().unwrap();
        if let Some(channel) = channels.get(&config.signal_uid) {
            if channel.config == config {
                return Ok(channel.clone());
            }
            channel.close();
        }
        channels.retain(|_, channel| {
            let keep = channel.snapshot().consumers > 0
                || (channel.config.source_url.is_empty()
                    && channel.inner.lock().unwrap().publisher);
            if !keep {
                channel.close();
            }
            keep
        });
        if channels.len() >= MAX_SIGNALS {
            return Err(invalid("Live Signal capacity reached"));
        }
        let uid = config.signal_uid.clone();
        let channel = Arc::new(Channel::new(config));
        channels.insert(uid, channel.clone());
        Ok(channel)
    }
}

impl Engine {
    pub async fn publish_stream_http(
        &self,
        signal: &str,
        token: &str,
        submission: crate::signal_endpoints::Submission,
    ) -> Result<bool, EngineError> {
        self.access_scope(
            false,
            self.publish_stream_http_inner(signal, token, submission),
        )
        .await
    }

    async fn publish_stream_http_inner(
        &self,
        signal: &str,
        token: &str,
        submission: crate::signal_endpoints::Submission,
    ) -> Result<bool, EngineError> {
        let now = nucleus::execution::now().timestamp();
        if submission.issued_at < now.saturating_sub(120)
            || submission.issued_at > now.saturating_add(30)
        {
            return Err(invalid("Reading expired or clock is incorrect"));
        }
        let endpoint = self.stream_token(signal, token).await?;
        let channel = self.stream_channel(signal, None).await?;
        if channel.config.value_type != "scalar" || !channel.config.source_url.is_empty() {
            return Err(invalid(
                "HTTP readings require a scalar Signal without an upstream source",
            ));
        }
        validate_scalar_endpoint(&endpoint, submission.value.as_bytes())?;
        let mut producer = channel.publisher()?;
        producer.retain_on_drop = true;
        producer
            .publish(
                Metadata {
                    epoch: producer.snapshot().epoch,
                    sequence: u64::try_from(submission.sequence)
                        .map_err(|_| invalid("Invalid sequence"))?,
                    captured_at: submission.issued_at.saturating_mul(1000),
                    content_type: "text/decimal".into(),
                },
                submission.value.into_bytes(),
            )
            .await
    }
    pub async fn stream_channel(
        &self,
        signal: &str,
        actor: Option<&str>,
    ) -> Result<Arc<Channel>, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.require_permission_on(&mut tx, actor, "record:read")
            .await?;
        let graph = crate::access::policy_graph_on(&mut tx, &[signal.into()])
            .await
            .map_err(|_| invalid("Signal unavailable"))?;
        if !self
            .readable_on(&mut tx, actor, &graph)
            .await?
            .contains(signal)
            || !store::signal_endpoints::signal_available_on(&mut tx, signal).await?
        {
            return Err(EngineError::Forbidden("Signal unavailable".into()));
        }
        let config = store::signal_streams::get_on(&mut tx, signal)
            .await?
            .ok_or_else(|| invalid("Signal is not memory-only"))?;
        tx.commit().await?;
        let channel = self.signal_streams.channel(config)?;
        Ok(channel)
    }

    pub async fn stream_token(
        &self,
        signal: &str,
        token: &str,
    ) -> Result<store::signal_endpoints::Endpoint, EngineError> {
        let digest = crate::signal_endpoints::token_digest(token)?;
        let endpoint = store::signal_endpoints::token_endpoint(&self.store.pool, &digest)
            .await?
            .ok_or_else(|| EngineError::Forbidden("Signal credential refused".into()))?;
        let mut tx = self.store.pool.begin().await?;
        if endpoint.signal_uid != signal
            || !endpoint.enabled
            || endpoint.expires_at <= nucleus::execution::now().timestamp()
            || !store::signal_endpoints::signal_available_on(&mut tx, signal).await?
        {
            return Err(EngineError::Forbidden("Signal credential refused".into()));
        }
        Ok(endpoint)
    }

    pub async fn configure_stream(
        &self,
        signal: &str,
        value_type: Option<&str>,
        source_url: &str,
        revision: Option<i64>,
        actor: Option<&str>,
    ) -> Result<Option<store::signal_streams::Configuration>, EngineError> {
        if value_type.is_some_and(|kind| !matches!(kind, "scalar" | "jpeg")) {
            return Err(invalid("Choose scalar or jpeg"));
        }
        source::validate_url(source_url)?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        for permission in [
            "record:read",
            "record:update",
            "configuration:update",
            "permission:assign",
        ] {
            self.require_permission_on(&mut tx, actor, permission)
                .await?;
        }
        let graph = crate::access::policy_graph_on(&mut tx, &[signal.into()])
            .await
            .map_err(|_| invalid("Signal unavailable"))?;
        if !self
            .readable_on(&mut tx, actor, &graph)
            .await?
            .contains(signal)
            || !store::signal_endpoints::signal_available_on(&mut tx, signal).await?
        {
            return Err(invalid("Signal unavailable"));
        }
        let old = store::signal_streams::get_on(&mut tx, signal).await?;
        if old.as_ref().map(|config| config.revision) != revision {
            return Err(invalid("Configuration changed; refresh first"));
        }
        let next_revision = old.as_ref().map_or(Ok(1), |old| {
            old.revision
                .checked_add(1)
                .ok_or_else(|| invalid("Configuration revision exhausted"))
        })?;
        let config = value_type.map(|kind| store::signal_streams::Configuration {
            signal_uid: signal.into(),
            value_type: kind.into(),
            source_url: source_url.into(),
            revision: next_revision,
        });
        if let Some(config) = &config {
            store::signal_streams::save_on(&mut tx, config).await?;
        } else {
            store::signal_streams::remove_on(&mut tx, signal).await?;
        }
        tx.commit().await?;
        self.signal_streams.invalidate(signal);
        self.notify_query_changed();
        Ok(config)
    }
}

pub fn validate_scalar_endpoint(
    endpoint: &store::signal_endpoints::Endpoint,
    bytes: &[u8],
) -> Result<(), EngineError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("Invalid decimal"))?;
    if text.len() > 64 {
        return Err(invalid("Decimal too long"));
    }
    let value =
        nucleus::DecimalValue::parse_inferred(text).map_err(|_| invalid("Invalid decimal"))?;
    let minimum = nucleus::DecimalValue::parse_inferred(&endpoint.minimum)
        .map_err(|_| invalid("Invalid endpoint range"))?;
    let maximum = nucleus::DecimalValue::parse_inferred(&endpoint.maximum)
        .map_err(|_| invalid("Invalid endpoint range"))?;
    if value.exact_numeric_cmp(minimum).is_lt() || value.exact_numeric_cmp(maximum).is_gt() {
        return Err(invalid("Reading outside endpoint range"));
    }
    Ok(())
}
