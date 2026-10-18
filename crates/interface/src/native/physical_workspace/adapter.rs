use super::*;
use base64::{Engine, prelude::BASE64_STANDARD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_PACKET: usize = 1_048_576;
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    Clock,
    Observe {
        evidence: model::Evidence,
    },
    Interact {
        interaction: model::Interaction,
    },
    Frame {
        evidence: model::Evidence,
        jpeg: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub version: u16,
    pub token: String,
    pub sequence: u64,
    pub command: Command,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Reply {
    pub sequence: u64,
    pub now_ms: u64,
    pub error: Option<String>,
    pub outcome: Option<InteractionOutcome>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub version: u16,
    pub address: String,
    pub token: String,
    pub device: String,
    pub session: String,
    pub now_ms: u64,
    pub surface: String,
    pub calibration_revision: u64,
}

enum Input {
    Clock,
    Observe(model::Evidence),
    Interact(model::Interaction),
    Frame(model::Evidence, PreparedFrame),
}

struct Pending {
    sequence: u64,
    input: Input,
    reply: tokio::sync::oneshot::Sender<Reply>,
}

struct Endpoint {
    root: Entity,
    workspace: u64,
    device: String,
    session: String,
    input: tokio::sync::mpsc::Receiver<Pending>,
    task: tokio::task::JoinHandle<()>,
    descriptor: Descriptor,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Resource, Default)]
pub(super) struct Endpoints(Vec<Endpoint>);

fn token_matches(expected: &str, supplied: &str) -> bool {
    if expected.len() != supplied.len() {
        return false;
    }
    expected
        .bytes()
        .zip(supplied.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

async fn read_packet(stream: &mut tokio::net::TcpStream) -> Result<Packet, String> {
    let size = stream.read_u32().await.map_err(|e| e.to_string())? as usize;
    if size == 0 || size > MAX_PACKET {
        return Err("Adapter packet exceeds 1 MiB.".into());
    }
    let mut bytes = vec![0; size];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid adapter packet.".into())
}

async fn write_reply(stream: &mut tokio::net::TcpStream, reply: Reply) -> Result<(), String> {
    let bytes = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    stream
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(&bytes).await.map_err(|e| e.to_string())
}

fn prepare(command: Command, device: &str, session: &str) -> Result<Input, String> {
    let check = |evidence: &model::Evidence| {
        if evidence.device != device || evidence.session != session {
            Err("Input belongs to another device or session.".to_string())
        } else {
            Ok(())
        }
    };
    match command {
        Command::Clock => Ok(Input::Clock),
        Command::Observe { evidence } => {
            check(&evidence)?;
            Ok(Input::Observe(evidence))
        }
        Command::Interact { interaction } => {
            check(&interaction.evidence)?;
            Ok(Input::Interact(interaction))
        }
        Command::Frame { evidence, jpeg } => {
            check(&evidence)?;
            let bytes = BASE64_STANDARD
                .decode(jpeg)
                .map_err(|_| "Invalid frame encoding.")?;
            if bytes.len() > 524_288 {
                return Err("Adapter JPEG exceeds 512 KiB.".into());
            }
            let dimensions = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes),
                image::ImageFormat::Jpeg,
            )
            .into_dimensions()
            .map_err(|_| "Invalid JPEG frame.")?;
            if dimensions.0 == 0
                || dimensions.1 == 0
                || dimensions.0 > 1920
                || dimensions.1 > 1920
                || u64::from(dimensions.0) * u64::from(dimensions.1) > 2_073_600
            {
                return Err("Adapter frame exceeds the pixel budget.".into());
            }
            let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
                .map_err(|_| "Could not decode adapter JPEG.")?
                .into_rgb8();
            Ok(Input::Frame(
                evidence,
                prepare_frame(Frame {
                    size: [image.width(), image.height()],
                    rgb: image.into_raw(),
                })?,
            ))
        }
    }
}

async fn serve(
    listener: tokio::net::TcpListener,
    token: String,
    device: String,
    session: String,
    sender: tokio::sync::mpsc::Sender<Pending>,
    wake: Option<crate::native::wake::WakeSignal>,
) {
    loop {
        let Ok((mut stream, address)) = listener.accept().await else {
            return;
        };
        if !address.ip().is_loopback() {
            continue;
        }
        let mut sequence = 0;
        loop {
            let Ok(Ok(packet)) =
                tokio::time::timeout(std::time::Duration::from_secs(10), read_packet(&mut stream))
                    .await
            else {
                break;
            };
            if packet.version != PROTOCOL_VERSION
                || !token_matches(&token, &packet.token)
                || packet.sequence <= sequence
            {
                break;
            }
            sequence = packet.sequence;
            let interval = if matches!(&packet.command, Command::Frame { .. }) {
                100
            } else {
                16
            };
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;
            let selected_device = device.clone();
            let selected_session = session.clone();
            let input = tokio::task::spawn_blocking(move || {
                prepare(packet.command, &selected_device, &selected_session)
            })
            .await;
            let input = match input {
                Ok(Ok(input)) => input,
                Ok(Err(error)) => {
                    let _ = write_reply(
                        &mut stream,
                        Reply {
                            sequence,
                            now_ms: 0,
                            error: Some(error),
                            outcome: None,
                        },
                    )
                    .await;
                    continue;
                }
                Err(_) => break,
            };
            let (reply, result) = tokio::sync::oneshot::channel();
            if sender
                .try_send(Pending {
                    sequence,
                    input,
                    reply,
                })
                .is_err()
            {
                break;
            }
            if let Some(wake) = &wake {
                wake.ring();
            }
            let Ok(Ok(reply)) =
                tokio::time::timeout(std::time::Duration::from_secs(5), result).await
            else {
                break;
            };
            if write_reply(&mut stream, reply).await.is_err() {
                break;
            }
        }
    }
}

pub fn start(
    world: &mut World,
    root: Entity,
    workspace: u64,
    surface: &str,
) -> Result<Descriptor, String> {
    permitted(world, root, workspace)?;
    let handle = tokio::runtime::Handle::try_current()
        .map_err(|_| "Adapter requires the application runtime.")?;
    if !world.contains_resource::<Endpoints>() {
        return Err("Adapter service is unavailable.".into());
    }
    let selected = world
        .get::<Installation>(root)
        .and_then(|i| i.0.get(&workspace))
        .ok_or("Configure a surface first.")?
        .runtime
        .surface(surface, workspace)?
        .clone();
    let device = selected
        .camera
        .clone()
        .ok_or("Assign a camera device first.")?;
    if let Some(endpoint) = world
        .resource::<Endpoints>()
        .0
        .iter()
        .find(|e| e.root == root && e.workspace == workspace && e.device == device)
    {
        let mut descriptor = endpoint.descriptor.clone();
        descriptor.now_ms = world.get::<Installation>(root).unwrap().0[&workspace].now_ms();
        descriptor.surface = selected.id;
        descriptor.calibration_revision = selected.calibration_revision;
        return Ok(descriptor);
    }
    if world.resource::<Endpoints>().0.len() >= 2 {
        return Err("At most two local hardware adapter endpoints can be enabled.".into());
    }
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let address = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .to_string();
    let _guard = handle.enter();
    let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
    let session = nucleus::new_uid("adapter");
    let token = format!("{}{}", nucleus::new_uid("key"), nucleus::new_uid("key"));
    connect(world, root, workspace, &device, &session)?;
    capture(world, root, workspace, &device, true)?;
    let (sender, input) = tokio::sync::mpsc::channel(4);
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let task = handle.spawn(serve(
        listener,
        token.clone(),
        device.clone(),
        session.clone(),
        sender,
        wake,
    ));
    let descriptor = Descriptor {
        version: PROTOCOL_VERSION,
        address,
        token,
        device: device.clone(),
        session: session.clone(),
        now_ms: world.get::<Installation>(root).unwrap().0[&workspace].now_ms(),
        surface: selected.id,
        calibration_revision: selected.calibration_revision,
    };
    world.resource_mut::<Endpoints>().0.push(Endpoint {
        root,
        workspace,
        device,
        session,
        input,
        task,
        descriptor: descriptor.clone(),
    });
    Ok(descriptor)
}

pub fn stop(world: &mut World, root: Entity, workspace: u64) {
    let revoked: Vec<_> = world
        .get_resource::<Endpoints>()
        .map(|e| {
            e.0.iter()
                .filter(|e| e.root == root && e.workspace == workspace)
                .map(|e| (e.device.clone(), e.session.clone()))
                .collect()
        })
        .unwrap_or_default();
    if let Some(mut endpoints) = world.get_resource_mut::<Endpoints>() {
        endpoints
            .0
            .retain(|e| e.root != root || e.workspace != workspace);
    }
    if let Some(mut installation) = world.get_mut::<Installation>(root)
        && let Some(session) = installation.0.get_mut(&workspace)
    {
        for (device, identity) in revoked {
            if session
                .runtime
                .connections
                .get(&device)
                .is_some_and(|c| c.session == identity)
            {
                session.runtime.disconnect(&device);
            }
        }
    }
}

pub(super) fn tick(world: &mut World) {
    let Some(mut endpoints) = world.remove_resource::<Endpoints>() else {
        return;
    };
    endpoints.0.retain_mut(|endpoint| {
        let valid = permitted(world, endpoint.root, endpoint.workspace).is_ok()
            && world
                .get::<Installation>(endpoint.root)
                .and_then(|i| i.0.get(&endpoint.workspace))
                .and_then(|s| s.runtime.connections.get(&endpoint.device))
                .is_some_and(|c| c.capture_enabled && c.session == endpoint.session);
        if !valid {
            if let Some(mut installation) = world.get_mut::<Installation>(endpoint.root)
                && let Some(session) = installation.0.get_mut(&endpoint.workspace)
                && session
                    .runtime
                    .connections
                    .get(&endpoint.device)
                    .is_some_and(|c| c.session == endpoint.session)
            {
                session.runtime.disconnect(&endpoint.device);
            }
            return false;
        }
        for _ in 0..8 {
            let Ok(pending) = endpoint.input.try_recv() else {
                break;
            };
            let result = match pending.input {
                Input::Clock => Ok(None),
                Input::Observe(evidence) => {
                    observe(world, endpoint.root, endpoint.workspace, evidence).map(|_| None)
                }
                Input::Interact(interaction) => {
                    interact(world, endpoint.root, endpoint.workspace, interaction)
                }
                Input::Frame(evidence, frame) => {
                    submit_prepared_frame(world, endpoint.root, endpoint.workspace, evidence, frame)
                        .map(|_| None)
                }
            };
            let now_ms =
                world.get::<Installation>(endpoint.root).unwrap().0[&endpoint.workspace].now_ms();
            let (outcome, error) = match result {
                Ok(outcome) => (outcome, None),
                Err(error) => (None, Some(error)),
            };
            let _ = pending.reply.send(Reply {
                sequence: pending.sequence,
                now_ms,
                error,
                outcome,
            });
        }
        true
    });
    world.insert_resource(endpoints);
}
