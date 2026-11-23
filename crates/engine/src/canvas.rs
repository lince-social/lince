// non-lean-yet
// Scope: struct CanvasInfo, struct Context, enum ExpectedResponse, struct Call.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 40, 42, 49, 53,
//   180, 186, 187, 190, 207, 243, 244, 245, 246, 308, 313, 314, 320, 328, 341, 345, 366, 381, 533,
//   557.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 85, 323.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites complete:
//   String (74), register_canvas: String (388).
// - [Clippy probe: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones.
//   Sites 186, 190, 244, 246, 328.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 85, 323.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 74, 110, 388.
// - [Clippy probe: clippy::rc_buffer] Check whether a shared slice or str suffices. Sites 128, 159,
//   448.
// - [rustc probe: missing_debug_implementations] type does not implement `std::fmt::Debug`;
//   consider adding `#[derive(Debug)]` or a manual implementation. Sites 15, 21, 59, 91.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   15, 16, 17, 21, 22, 23, 59, 60, 61, 68, 74, 91, 92, 99, 106, 110, 114, 388, 396, 399.

pub mod scene_edits;

use crate::{Engine, EngineError, actions::ActionOutcome};
use nucleus::canvas::{
    Component, Descriptor, Mutation, Request, Response, scene::operation as scene,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, serde::Serialize)]
pub struct CanvasInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Context {
    pub actor: Option<String>,
    pub origin: Option<nucleus::component::composition::Origin>,
}

enum ExpectedResponse {
    Registry,
    Scene(scene::Request),
    Element { placement: String },
    Export { name: String },
    Snapshot,
    Mutation { request_id: String, revision: u64 },
    Receipt { request_id: String },
}

impl From<&Request> for ExpectedResponse {
    fn from(request: &Request) -> Self {
        match request {
            Request::Registry => Self::Registry,
            Request::Scene { query, .. } => Self::Scene(query.clone()),
            Request::InspectElement { placement, .. } => Self::Element {
                placement: placement.clone(),
            },
            Request::Export { name, .. } => Self::Export { name: name.clone() },
            Request::Inspect { .. } => Self::Snapshot,
            Request::Mutate {
                request_id,
                expected_revision,
                ..
            } => Self::Mutation {
                request_id: request_id.clone(),
                revision: *expected_revision,
            },
            Request::Receipt { request_id } => Self::Receipt {
                request_id: request_id.clone(),
            },
        }
    }
}

pub struct Call {
    pub request: Request,
    pub context: Context,
    scene_enqueued: Option<(std::time::Instant, usize)>,
    live: Arc<AtomicBool>,
    canvas_live: Arc<AtomicBool>,
    reply: Option<oneshot::Sender<Result<Response, String>>>,
}

impl Call {
    pub fn is_cancelled(&self) -> bool {
        !self.live.load(Ordering::Acquire)
            || !self.canvas_live.load(Ordering::Acquire)
            || self.reply.as_ref().is_none_or(oneshot::Sender::is_closed)
    }

    pub fn complete(mut self, response: Result<Response, String>) -> Result<(), String> {
        if self.is_cancelled() {
            return Err(
                "This canvas request expired or was revoked. Inspect actual state before retrying."
                    .into(),
            );
        }
        self.reply
            .take()
            .ok_or("The canvas request already completed.")?
            .send(response)
            .map_err(|_| {
                "The canvas caller disconnected; inspect actual state before retrying.".into()
            })
    }
}

pub struct Receiver {
    pub info: CanvasInfo,
    wake: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    receiver: mpsc::Receiver<Call>,
    live: Arc<AtomicBool>,
}

impl Receiver {
    pub fn set_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(wake));
    }

    pub fn has_pending(&self) -> bool {
        !self.receiver.is_empty()
    }

    pub fn try_recv(&mut self) -> Result<Call, tokio::sync::mpsc::error::TryRecvError> {
        let call = self.receiver.try_recv()?;
        if let Some((start, bytes)) = call.scene_enqueued {
            tracing::debug!(target: "engine::scene_edits", request_bytes = bytes, queue_us = start.elapsed().as_micros(), remaining_calls = self.receiver.len(), "Scene request admitted and dequeued");
        }
        Ok(call)
    }

    pub async fn recv(&mut self) -> Option<Call> {
        self.receiver.recv().await
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
    }
}

struct Host {
    wake: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    info: CanvasInfo,
    registry: Arc<Vec<Descriptor>>,
    sender: mpsc::Sender<Call>,
    live: Arc<AtomicBool>,
}

type ResultWatch = watch::Sender<Option<Result<Response, String>>>;

struct Receipt {
    hash: [u8; 32],
    result: ResultWatch,
}

#[derive(Default)]
pub(crate) struct Broker {
    hosts: Mutex<BTreeMap<String, Host>>,
    receipts: Mutex<BTreeMap<String, Receipt>>,
    pending: Mutex<Vec<(Context, std::sync::Weak<AtomicBool>)>>,
}

struct Pending(Arc<AtomicBool>);
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Broker {
    fn register(
        &self,
        id: String,
        name: String,
        registry: Arc<Vec<Descriptor>>,
    ) -> Result<Receiver, String> {
        if !nucleus::valid_uid(&id, "canvas") || name.trim().is_empty() || name.len() > 256 {
            return Err("Use a stable canvas UID and a name of at most 256 bytes.".into());
        }
        nucleus::canvas::validate_registry(&registry)?;
        let mut hosts = self
            .hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hosts.retain(|_, host| host.live.load(Ordering::Acquire));
        if hosts.contains_key(&id) {
            return Err("This canvas is already connected.".into());
        }
        if hosts.len() >= 8 {
            return Err("At most eight local canvases can be connected.".into());
        }
        let (sender, receiver) = mpsc::channel(32);
        let live = Arc::new(AtomicBool::new(true));
        let wake = Arc::new(Mutex::new(None));
        let info = CanvasInfo {
            id: id.clone(),
            name,
        };
        hosts.insert(
            id,
            Host {
                wake: wake.clone(),
                info: info.clone(),
                registry,
                sender,
                live: live.clone(),
            },
        );
        Ok(Receiver {
            wake,
            info,
            receiver,
            live,
        })
    }

    fn list(&self) -> Vec<CanvasInfo> {
        self.hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .filter(|host| host.live.load(Ordering::Acquire))
            .map(|host| host.info.clone())
            .collect()
    }

    fn target(
        &self,
        id: Option<&str>,
    ) -> Result<
        (
            String,
            Arc<Vec<Descriptor>>,
            mpsc::Sender<Call>,
            Arc<AtomicBool>,
        ),
        String,
    > {
        let hosts = self
            .hosts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut available = hosts
            .values()
            .filter(|host| host.live.load(Ordering::Acquire));
        let host = if let Some(id) = id {
            hosts
                .get(id)
                .filter(|host| host.live.load(Ordering::Acquire))
                .ok_or("The requested local canvas is not connected.")?
        } else {
            let host = available.next().ok_or("No local canvas is connected. Open a supported native canvas to use layout Actions.")?;
            if available.next().is_some() {
                return Err("Choose an explicit canvas: more than one is connected.".into());
            }
            host
        };
        Ok((
            host.info.id.clone(),
            host.registry.clone(),
            host.sender.clone(),
            host.live.clone(),
        ))
    }

    pub(crate) fn revoke(&self, agent: &str, thread: &str) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.retain(|(context, live)| {
            let Some(live) = live.upgrade() else {
                return false;
            };
            if context
                .origin
                .as_ref()
                .is_some_and(|origin| origin.agent == agent && origin.thread == thread)
            {
                live.store(false, Ordering::Release);
            }
            live.load(Ordering::Acquire)
        });
    }

    async fn dispatch(
        &self,
        id: &str,
        request: Request,
        context: Context,
        sender: mpsc::Sender<Call>,
        canvas_live: Arc<AtomicBool>,
    ) -> Result<Response, String> {
        let key = if let Request::Mutate { request_id, .. } = &request {
            Some(format!(
                "{id}:{}:{}:{request_id}",
                context.actor.as_deref().unwrap_or("local"),
                serde_json::to_string(&context.origin).map_err(|e| e.to_string())?
            ))
        } else {
            None
        };
        let mut result_watch = None;
        if let Some(key) = &key {
            let hash = nucleus::json::sha256(&request).map_err(|e| e.to_string())?;
            let cached = {
                let mut receipts = self
                    .receipts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(receipt) = receipts.get(key) {
                    if receipt.hash != hash {
                        return Err(
                            "This canvas request ID was already used for different parameters."
                                .into(),
                        );
                    }
                    Some(receipt.result.subscribe())
                } else {
                    if receipts.len() >= 256 {
                        let oldest = receipts
                            .iter()
                            .find(|(_, receipt)| receipt.result.borrow().is_some())
                            .map(|(key, _)| key.clone())
                            .ok_or("Too many pending canvas requests.")?;
                        receipts.remove(&oldest);
                    }
                    let (result, _) = watch::channel(None);
                    result_watch = Some(result.clone());
                    receipts.insert(key.clone(), Receipt { hash, result });
                    None
                }
            };
            if let Some(mut cached) = cached {
                loop {
                    if let Some(result) = cached.borrow_and_update().clone() {
                        return result;
                    }
                    tokio::time::timeout(Duration::from_secs(15), cached.changed()).await.map_err(|_| "The earlier canvas request is unresolved. Inspect actual state before retrying.")?.map_err(|_| "The earlier canvas caller disconnected. Inspect actual state before retrying.")?;
                }
            }
        }
        let live = Arc::new(AtomicBool::new(true));
        let _pending = Pending(live.clone());
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.retain(|(_, live)| {
                live.upgrade()
                    .is_some_and(|live| live.load(Ordering::Acquire))
            });
            if pending.len() >= 64 {
                let error = "Too many pending canvas requests.".to_string();
                if let Some(watch) = result_watch {
                    watch.send_replace(Some(Err(error.clone())));
                }
                return Err(error);
            }
            pending.push((context.clone(), Arc::downgrade(&live)));
        }
        let (reply, answer) = oneshot::channel();
        let scene_enqueued = if matches!(request, Request::Scene { .. }) {
            Some((
                std::time::Instant::now(),
                nucleus::json::encoded_len(&request).map_err(|e| e.to_string())?,
            ))
        } else {
            None
        };
        let call = Call {
            scene_enqueued,
            request,
            context,
            live,
            canvas_live,
            reply: Some(reply),
        };
        let sent = sender.try_send(call);
        if sent.is_ok() {
            let wake = self
                .hosts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(id)
                .and_then(|host| {
                    host.wake
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone()
                });
            if let Some(wake) = wake {
                wake();
            }
        }
        let result = match sent {
            Ok(()) => match tokio::time::timeout(Duration::from_secs(15), answer).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err("The canvas disconnected before acknowledging. Inspect actual state before retrying.".into()),
                Err(_) => Err("The canvas did not acknowledge within 15 seconds. An edit may have applied; inspect actual state before retrying.".into()),
            },
            Err(_) => Err("The canvas request queue is unavailable or full.".into()),
        };
        if let Some(watch) = result_watch {
            watch.send_replace(Some(result.clone()));
        }
        result
    }
}

impl Engine {
    pub fn register_canvas(
        &self,
        id: String,
        name: String,
        registry: impl Into<Arc<Vec<Descriptor>>>,
    ) -> Result<Receiver, String> {
        self.canvas.register(id, name, registry.into())
    }
    pub fn connected_canvases(&self) -> Vec<CanvasInfo> {
        self.canvas.list()
    }
    pub fn revoke_canvas_requests(&self, agent: &str, thread: &str) {
        self.canvas.revoke(agent, thread);
    }

    pub(crate) fn resolve_canvas_component<'a>(
        &'a self,
        component: &'a mut Component,
        actor: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), EngineError>> + Send + 'a>>
    {
        Box::pin(async move {
            match component {
                Component::Authored { base, edits } => {
                    self.resolve_canvas_component(base, actor).await?;
                    for part in edits.added.values_mut() {
                        if let Some(component) = &mut part.component {
                            self.resolve_canvas_component(component, actor).await?;
                        }
                    }
                    for behavior in edits.behaviors.values() {
                        crate::actions::Action::deserialize(&behavior.action)
                            .map_err(EngineError::Json)?;
                    }
                }
                Component::Builtin { state } => {
                    self.resolve_component_in_place(state).await?;
                    self.authorize_component(state, actor).await?;
                }
                Component::Native { bindings, .. } => {
                    for binding in bindings.iter_mut() {
                        *binding = self.resolve(binding).await?;
                    }
                    self.refuse_unreadable_karma_inputs(actor, bindings).await?;
                }
                Component::Composition { composition } => {
                    for part in &mut composition.parts {
                        for event in &part.events {
                            crate::actions::Action::deserialize(&event.action)
                                .map_err(EngineError::Json)?;
                        }
                        self.resolve_canvas_component(&mut part.component, actor)
                            .await?;
                    }
                }
            }
            Ok(())
        })
    }

    pub(crate) fn canvas_registry(&self) -> Result<Arc<Vec<Descriptor>>, EngineError> {
        self.canvas
            .target(None)
            .map(|(_, registry, _, _)| registry)
            .map_err(EngineError::Consequence)
    }

    pub(crate) async fn canvas_action(
        &self,
        target: Option<String>,
        mut request: Request,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        if actor.is_some() {
            return Err(EngineError::Forbidden(
                "Canvas layout Actions currently require the local interface session.".into(),
            ));
        }
        let preparation = crate::performance::Span::new("canvas-command-preparation-await");
        request.validate().map_err(EngineError::Consequence)?;
        let (id, registry, sender, live) = self
            .canvas
            .target(target.as_deref())
            .map_err(EngineError::Consequence)?;
        if let Request::Registry = request {
            return Ok(ActionOutcome {
                data: Some(serde_json::json!({"result":"registry","components":registry})),
                ..Default::default()
            });
        }
        let origin = crate::operation_origin::component_origin();
        if let Request::Mutate { mutation, .. } = &mut request {
            let configuring = matches!(mutation, Mutation::Configure { .. });
            match mutation {
                Mutation::Rebind { records, .. } => {
                    self.refuse_unreadable_karma_inputs(
                        actor,
                        &records.values().cloned().collect::<Vec<_>>(),
                    )
                    .await?;
                }
                Mutation::Patch { changes, .. } => {
                    for change in changes {
                        match change {
                            nucleus::canvas::authoring::Change::InsertPart {
                                definition, ..
                            } => {
                                if let Some(component) = &mut definition.component {
                                    self.resolve_canvas_component(component, actor).await?;
                                    component
                                        .validate(&registry, false)
                                        .map_err(EngineError::Consequence)?;
                                }
                            }
                            nucleus::canvas::authoring::Change::ConfigurePart {
                                component: Some(component),
                                ..
                            } => {
                                self.resolve_canvas_component(component, actor).await?;
                                component
                                    .validate(&registry, true)
                                    .map_err(EngineError::Consequence)?;
                            }
                            nucleus::canvas::authoring::Change::SetBehavior {
                                behavior, ..
                            } => {
                                crate::actions::Action::deserialize(&behavior.action)
                                    .map_err(EngineError::Json)?;
                            }
                            _ => {}
                        }
                    }
                }
                Mutation::Add { component, .. } | Mutation::Configure { component, .. } => {
                    component
                        .validate(&registry, configuring)
                        .map_err(EngineError::Consequence)?;
                    self.resolve_canvas_component(component, actor).await?;
                    if let Some(origin) = &origin
                        && !configuring
                    {
                        if let Component::Builtin {
                            state: nucleus::component::ComponentState::Composition { composition },
                        } = component
                        {
                            composition.origin = Some(origin.clone());
                        } else {
                            if !matches!(component, Component::Composition { .. }) {
                                let wrapper = Component::Composition {
                                    composition: nucleus::canvas::Composition {
                                        name: "Fiote interaction".into(),
                                        origin: None,
                                        parts: Vec::with_capacity(1),
                                    },
                                };
                                let content = std::mem::replace(component, wrapper);
                                if let Component::Composition { composition } = component {
                                    composition.parts.push(nucleus::canvas::Part {
                                        id: "content".into(),
                                        geometry: nucleus::canvas::Geometry {
                                            position: [0.0, 0.0],
                                            size: [840.0, 680.0],
                                        },
                                        component: content,
                                        events: vec![],
                                    });
                                }
                            }
                            if let Component::Composition { composition } = component {
                                composition.origin = Some(origin.clone());
                            }
                        }
                        component
                            .validate(&registry, configuring)
                            .map_err(EngineError::Consequence)?;
                    }
                }
                _ => {}
            }
        }
        let expected = ExpectedResponse::from(&request);
        drop(preparation);
        let response = self
            .canvas
            .dispatch(
                &id,
                request,
                Context {
                    actor: actor.map(str::to_string),
                    origin,
                },
                sender,
                live,
            )
            .await
            .map_err(EngineError::Consequence)?;
        nucleus::canvas::bounded(&response).map_err(EngineError::Consequence)?;
        let response = match (expected, response) {
            (ExpectedResponse::Scene(scene::Request::Inspect), Response::Scene { inspection })
                if inspection.document.valid()
                    && inspection.placements.len() <= nucleus::canvas::scene::MAX_SLOTS
                    && inspection
                        .placements
                        .iter()
                        .all(|p| p.handle.root.valid() && p.handle.slot.valid()) =>
            {
                Response::Scene { inspection }
            }
            (
                ExpectedResponse::Scene(scene::Request::Submit { edit }),
                Response::SceneReceipt { receipt },
            ) if receipt.edit == edit
                && receipt.revision != edit.expected
                && receipt.handle.root.valid()
                && receipt.handle.slot.valid()
                && receipt.fiote == crate::operation_origin::component_origin() =>
            {
                Response::SceneReceipt { receipt }
            }
            (
                ExpectedResponse::Scene(scene::Request::Receipt {
                    document,
                    request_id,
                }),
                Response::SceneReceipt { receipt },
            ) if receipt.edit.document == document
                && receipt.edit.request_id == request_id
                && receipt.edit.validate().is_ok() =>
            {
                Response::SceneReceipt { receipt }
            }
            (ExpectedResponse::Element { placement }, Response::Element { element })
                if element.placement.id == placement =>
            {
                element
                    .placement
                    .component
                    .validate_snapshot(&registry)
                    .map_err(EngineError::Consequence)?;
                let records: Vec<_> = element
                    .placement
                    .component
                    .records()
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                self.refuse_unreadable_karma_inputs(actor, &records).await?;
                Response::Element { element }
            }
            (
                ExpectedResponse::Export { name },
                Response::Export {
                    document,
                    hash,
                    revision,
                },
            ) if document.name == name && document.format == nucleus::canvas::Document::FORMAT => {
                document
                    .component
                    .validate_snapshot(&registry)
                    .map_err(EngineError::Consequence)?;
                let records: Vec<_> = document
                    .component
                    .records()
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                self.refuse_unreadable_karma_inputs(actor, &records).await?;
                if nucleus::canvas::authoring::hash(&document).map_err(EngineError::Consequence)?
                    != hash
                {
                    return Err(EngineError::Consequence(
                        "The export hash does not match its canonical definition.".into(),
                    ));
                }
                Response::Export {
                    document,
                    hash,
                    revision,
                }
            }
            (ExpectedResponse::Snapshot, Response::Snapshot { mut snapshot }) => {
                snapshot.validate().map_err(EngineError::Consequence)?;
                let mut readable = Vec::new();
                for placement in snapshot.placements {
                    placement
                        .component
                        .validate_snapshot(&registry)
                        .map_err(EngineError::Consequence)?;
                    let records: Vec<_> = placement
                        .component
                        .records()
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                    if self
                        .refuse_unreadable_karma_inputs(actor, &records)
                        .await
                        .is_ok()
                    {
                        readable.push(placement);
                    }
                }
                snapshot.placements = readable;
                Response::Snapshot { snapshot }
            }
            (
                ExpectedResponse::Mutation {
                    request_id,
                    revision,
                },
                Response::Receipt { receipt },
            ) => {
                if receipt.request_id != request_id || receipt.revision <= revision {
                    return Err(EngineError::Consequence(
                        "The canvas returned an invalid mutation acknowledgement.".into(),
                    ));
                }
                validate_receipt(&receipt)?;
                Response::Receipt { receipt }
            }
            (ExpectedResponse::Receipt { request_id }, Response::Receipt { receipt })
                if receipt.request_id == request_id =>
            {
                validate_receipt(&receipt)?;
                Response::Receipt { receipt }
            }
            _ => {
                return Err(EngineError::Consequence(
                    "The canvas returned a result for the wrong operation.".into(),
                ));
            }
        };
        Ok(ActionOutcome {
            data: Some(serde_json::to_value(response).map_err(EngineError::Json)?),
            ..Default::default()
        })
    }
}

fn validate_receipt(receipt: &nucleus::canvas::Receipt) -> Result<(), EngineError> {
    if receipt.affected_placements.len() > nucleus::canvas::MAX_PLACEMENTS
        || receipt.affected_count > nucleus::canvas::MAX_STATE_PLACEMENTS
        || receipt.affected_count < receipt.affected_placements.len()
        || receipt
            .affected_placements
            .iter()
            .any(|id| nucleus::canvas::placement_id(id).is_err())
        || receipt.workspace == Some(0)
    {
        return Err(EngineError::Consequence(
            "The canvas returned an invalid receipt.".into(),
        ));
    }
    Ok(())
}
