use super::*;
use crate::native::sand_panel as panel;
use lince_ailuros::{
    configuration::{CameraPins, Configuration, Destination, Setup, Workflow},
    project::{Artifact, ChipProbe, Toolchain},
    serial::{Cancellation, DeviceStatus, Identity, Port, Session},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const PORT: usize = 0;
const PROJECT: usize = 1;
const NAME: usize = 2;
const SLUG: usize = 3;
const SERVER: usize = 4;
const SSID: usize = 5;
const PASSWORD: usize = 6;
const CA: usize = 7;
const TIME: usize = 8;
const PINS: usize = 9;
const WIDTH: usize = 10;
const HEIGHT: usize = 11;
const FPS: usize = 12;
const QUALITY: usize = 13;
const KIND: usize = 14;
const VALUE: usize = 15;
const INTERVAL: usize = 16;
const CUSTOM_NAME: usize = 17;
const CUSTOM_SETTINGS: usize = 18;
const CUSTOM_OUTPUT: usize = 19;

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Iroh",
        author: "n0 computer and contributors",
        license: lince_ailuros::IROH_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "ESP iroh provider and DNS adapter",
        author: "Remade With Rust and n0 computer contributors",
        license: lince_ailuros::IROH_PROVIDER_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Rustls",
        author: "Rustls contributors",
        license: lince_ailuros::RUSTLS_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "RustCrypto Rustls provider",
        author: "RustCrypto contributors",
        license: lince_ailuros::RUSTCRYPTO_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Serialport",
        author: "Bryant Mairs, Jesse Braham and serialport-rs contributors",
        license: lince_ailuros::SERIAL_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "ESP-IDF 5.5.2",
        author: "Espressif Systems and contributors",
        license: lince_ailuros::SDK_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "ESP32 camera driver 2.1.0",
        author: "Espressif Systems and contributors",
        license: lince_ailuros::CAMERA_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "ESP WebSocket Client 1.6.1",
        author: "Espressif Systems and contributors",
        license: lince_ailuros::WEBSOCKET_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "ESP JPEG 1.3.1",
        author: "Espressif Systems and contributors",
        license: lince_ailuros::JPEG_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "TJpgDec R0.03",
        author: "ChaN, 2021",
        license: lince_ailuros::TJPGDEC_LICENSE,
    },
];

#[derive(Component)]
struct Form {
    panel: Entity,
    ports: Entity,
    advanced: Entity,
    status: Entity,
    selection: Entity,
    fields: Vec<Entity>,
    values: Vec<String>,
    identity: Option<Identity>,
    probe: Option<ChipProbe>,
    selected: Option<String>,
    artifact: Option<Artifact>,
    streams: Vec<store::signal_streams::Configuration>,
    confirmed_pins: bool,
    pending: Option<(String, Backend)>,
    job: Option<Job>,
}

struct Job {
    cancellation: Cancellation,
    reply: Arc<Mutex<Option<Result<Reply, String>>>>,
    port: String,
    project: String,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

enum Reply {
    Ports(Vec<Port>),
    Probe(ChipProbe),
    Profile(lince_ailuros::profile::BoardProfile),
    Folder(PathBuf),
    Identity(Identity),
    Exported(lince_ailuros::profile::BoardProfile),
    Compiled(Artifact),
    Flashed,
    Provisioned {
        password: String,
        endpoint: String,
        revision: i64,
    },
    Status(DeviceStatus),
}

#[derive(Clone, Copy)]
enum Backend {
    Enroll,
    Configure,
    Enable,
}

#[derive(Clone)]
pub(super) enum Control {
    Ports,
    Probe,
    Profile,
    ChooseProject,
    ChoosePort(String),
    Inspect,
    SetupWindow,
    Export(String),
    Compile,
    Flash,
    Enroll,
    Prepare,
    Provision,
    Status,
    OpenVideo,
    OpenSource,
    OpenResources,
    Advanced,
    ConfirmPins,
    Cancel,
    Select(String),
}

fn values() -> Vec<String> {
    vec![
        "",
        "",
        "ESP camera",
        "esp-camera",
        "",
        "",
        "",
        "",
        "pool.ntp.org",
        "",
        "320",
        "240",
        "5",
        "12",
        "camera",
        "12.5",
        "60",
        "scalar-demo",
        "{\"value\":\"12.5\",\"interval_seconds\":60}",
        "scalar",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let root = panel::column(world, parent);
    world.get_mut::<Node>(root).unwrap().display = Display::None;
    crate::native::edit_mode::label(world, root, "Install and connect firmware", 16.0);
    crate::native::edit_mode::label(
        world,
        root,
        "USB setup, Wi-Fi video, then wall USB power. The selected Cell receives the Signal. Firmware keys stay on the ESP.",
        13.0,
    );
    let row = panel::row(world, root);
    panel::button(world, row, owner, "Find USB devices", Control::Ports);
    panel::button(world, row, owner, "Probe selected ESP", Control::Probe);
    panel::button(
        world,
        row,
        owner,
        "Inspect installed firmware",
        Control::Inspect,
    );
    panel::button(world, row, owner, "Open USB setup", Control::SetupWindow);
    let ports_panel = panel::column(world, root);
    let defaults = values();
    let mut fields = vec![Entity::PLACEHOLDER; defaults.len()];
    for (index, label) in [
        (PORT, "Selected serial port"),
        (PROJECT, "Editable firmware project folder"),
        (NAME, "Device name"),
        (SLUG, "Signal slug"),
        (SERVER, "Destination Cell HTTPS origin"),
        (SSID, "Wi-Fi SSID"),
    ] {
        fields[index] = panel::field(world, root, label, &defaults[index]);
    }
    fields[PASSWORD] = crate::native::owner_backup::secret_field(world, root, "Wi-Fi password");
    let row = panel::row(world, root);
    for (id, _) in lince_ailuros::profile::catalog() {
        match lince_ailuros::profile::builtin(id) {
            Ok(profile) => {
                panel::button(
                    world,
                    row,
                    owner,
                    &format!("Create {} project", profile.name),
                    Control::Export(id.into()),
                );
            }
            Err(error) => {
                crate::native::edit_mode::label(world, row, &error, 13.0);
            }
        }
    }
    let row = panel::row(world, root);
    for (caption, control) in [
        ("Choose project folder", Control::ChooseProject),
        ("Load board profile", Control::Profile),
        ("Edit firmware", Control::OpenSource),
        ("Firmware resources", Control::OpenResources),
        ("Compile firmware", Control::Compile),
        ("Install compiled firmware", Control::Flash),
    ] {
        panel::button(world, row, owner, caption, control);
    }
    let row = panel::row(world, root);
    for (caption, control) in [
        ("Enroll device and prepare Signal", Control::Enroll),
        ("Prepare selected Signal", Control::Prepare),
        ("Provision and enable", Control::Provision),
        ("Read device status", Control::Status),
        ("Open Video Castle", Control::OpenVideo),
    ] {
        panel::button(world, row, owner, caption, control);
    }
    let selection = crate::native::edit_mode::label(
        world,
        root,
        "Inspect the installed firmware, then enroll or select its device above.",
        13.0,
    );
    let row = panel::row(world, root);
    panel::button(
        world,
        row,
        owner,
        "Camera, trust and workflow settings",
        Control::Advanced,
    );
    panel::button(
        world,
        row,
        owner,
        "Confirm camera wiring",
        Control::ConfirmPins,
    );
    panel::button(world, row, owner, "Cancel local operation", Control::Cancel);
    let advanced = panel::column(world, root);
    world.get_mut::<Node>(advanced).unwrap().display = Display::None;
    crate::native::edit_mode::label(
        world,
        advanced,
        "Choose hardware through the project board.json. Confirm its wiring before provisioning. Supply CameraPins JSON with data[8], xclk, pclk, vsync, href, sda, scl, pwdn and reset. Use -1 only for absent power/reset pins. Memory and clock settings follow the board profile when compiling.",
        13.0,
    );
    for (index, label) in [
        (CA, "Custom CA PEM (empty uses firmware CA bundle)"),
        (TIME, "Time server"),
        (PINS, "Verified camera pin map JSON"),
        (WIDTH, "Frame width"),
        (HEIGHT, "Frame height"),
        (FPS, "Frames per second"),
        (QUALITY, "JPEG quality (10–63)"),
        (KIND, "Workflow: camera, scalar or custom"),
        (VALUE, "Scalar sample value"),
        (INTERVAL, "Scalar interval seconds"),
        (CUSTOM_NAME, "Custom workflow name"),
        (CUSTOM_SETTINGS, "Custom workflow settings JSON"),
        (CUSTOM_OUTPUT, "Custom Signal output: scalar or jpeg"),
    ] {
        fields[index] = panel::field(world, advanced, label, &defaults[index]);
    }
    let status = crate::native::edit_mode::label(
        world,
        root,
        "Choose a serial device and an empty project folder. Initial unconfigured firmware permits provisioning; later hold BOOT for two seconds while it is running, then release.",
        13.0,
    );
    crate::native::accessibility::status(world, status);
    panel::credits(world, root, root, CREDITS);
    world.entity_mut(owner).insert(Form {
        panel: root,
        ports: ports_panel,
        advanced,
        status,
        selection,
        fields,
        values: defaults,
        identity: None,
        probe: None,
        selected: None,
        artifact: None,
        streams: Vec::new(),
        confirmed_pins: false,
        pending: None,
        job: None,
    });
}

pub(super) fn toggle(world: &mut World, owner: Entity, open: bool) {
    if let Some(form) = world.get::<Form>(owner) {
        let root = form.panel;
        world.get_mut::<Node>(root).unwrap().display =
            if open { Display::Flex } else { Display::None };
    }
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(form) = world.get::<Form>(owner) {
        let target = form.status;
        if let Some(mut text) = world.get_mut::<Text>(target) {
            text.0 = message.into();
        }
    }
}

fn capture(world: &mut World, owner: Entity) {
    let Some(form) = world.get::<Form>(owner) else {
        return;
    };
    let values: Vec<_> = form
        .fields
        .iter()
        .map(|entity| {
            world
                .get::<EditableText>(*entity)
                .map_or_else(String::new, |text| text.value().to_string())
        })
        .collect();
    let mut form = world.get_mut::<Form>(owner).unwrap();
    if values[PORT] != form.values[PORT] {
        form.identity = None;
        form.probe = None;
        form.selected = None;
        form.confirmed_pins = false;
    }
    if values[PROJECT] != form.values[PROJECT] {
        form.artifact = None;
        form.confirmed_pins = false;
    }
    if values[PINS] != form.values[PINS] {
        form.confirmed_pins = false;
    }
    form.values = values;
}

fn selected(world: &World, owner: Entity) -> Result<Endpoint, String> {
    let form = world
        .get::<Form>(owner)
        .ok_or("Firmware setup unavailable")?;
    let row = world
        .get::<Devices>(owner)
        .and_then(|state| {
            state
                .rows
                .iter()
                .find(|row| Some(&row.uid) == form.selected.as_ref())
        })
        .ok_or("Enroll or select a device first")?;
    let identity = form
        .identity
        .as_ref()
        .ok_or("Inspect the connected firmware first")?;
    if identity.public_key != row.public_key {
        return Err("The connected firmware key differs from this enrolled device".into());
    }
    Ok(row.clone())
}

fn configuration(world: &World, owner: Entity) -> Result<Configuration, String> {
    let row = selected(world, owner)?;
    let form = world.get::<Form>(owner).unwrap();
    let values = &form.values;
    let number = |index: usize| {
        values[index]
            .parse::<u32>()
            .map_err(|_| "Enter whole numbers for workflow settings".to_owned())
    };
    let workflow = match values[KIND].as_str() {
        "camera" => {
            if !form.confirmed_pins {
                return Err("Review the actual board wiring, enter its pin map and choose Confirm camera wiring".into());
            }
            let pins: CameraPins = serde_json::from_str(&values[PINS])
                .map_err(|_| "Enter a valid CameraPins JSON object")?;
            Workflow::Camera {
                pins,
                width: number(WIDTH)?,
                height: number(HEIGHT)?,
                fps: number(FPS)?,
                quality: number(QUALITY)?,
            }
        }
        "scalar" => Workflow::Scalar {
            value: values[VALUE].clone(),
            interval_seconds: number(INTERVAL)?,
        },
        "custom" => Workflow::Custom {
            name: values[CUSTOM_NAME].clone(),
            output: values[CUSTOM_OUTPUT].clone(),
            settings: serde_json::from_str(&values[CUSTOM_SETTINGS])
                .map_err(|_| "Enter a custom settings JSON object")?,
        },
        _ => {
            return Err(
                "Choose camera, scalar or custom; edit workflow source in the exported project"
                    .into(),
            );
        }
    };
    let configuration = Configuration {
        version: 1,
        ssid: values[SSID].clone(),
        password: values[PASSWORD].clone(),
        destination: Destination::Https {
            origin: values[SERVER].clone(),
            ca_pem: values[CA].clone(),
        },
        time_server: values[TIME].clone(),
        setup: Setup {
            audience: world.get::<Devices>(owner).unwrap().audience.clone(),
            device: row.uid,
            signal: row.signal_uid,
            key_epoch: row.credential_epoch,
        },
        workflow,
    };
    configuration.validate()?;
    Ok(configuration)
}

fn backend(
    world: &mut World,
    owner: Entity,
    request: Request,
    kind: Backend,
) -> Result<(), String> {
    let id = nucleus::new_uid("ailuros-setup");
    super::send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::SignalEndpoints { request },
        },
    )?;
    world.get_mut::<Form>(owner).unwrap().pending = Some((id, kind));
    status(world, owner, "Waiting for the destination Cell to confirm…");
    Ok(())
}

fn prepare(world: &mut World, owner: Entity) -> Result<(), String> {
    let row = selected(world, owner)?;
    let form = world.get::<Form>(owner).unwrap();
    let kind = match form.values[KIND].as_str() {
        "camera" => Some("jpeg".into()),
        "scalar" => None,
        "custom" if form.values[CUSTOM_OUTPUT] == "jpeg" => Some("jpeg".into()),
        "custom" if form.values[CUSTOM_OUTPUT] == "scalar" => None,
        _ => {
            return Err(
                "Choose a supported workflow and output type before changing the Signal".into(),
            );
        }
    };
    let revision = form
        .streams
        .iter()
        .find(|stream| stream.signal_uid == row.signal_uid)
        .map(|stream| stream.revision);
    backend(
        world,
        owner,
        Request::ConfigureStream {
            signal: row.signal_uid,
            value_type: kind,
            source_url: String::new(),
            expected_revision: revision,
        },
        Backend::Configure,
    )
}

fn start(
    world: &mut World,
    owner: Entity,
    work: impl FnOnce(Cancellation) -> Result<Reply, String> + Send + 'static,
) {
    let cancellation = Cancellation::default();
    let reply = Arc::new(Mutex::new(None));
    let output = reply.clone();
    let token = cancellation.clone();
    let form = world.get::<Form>(owner).unwrap();
    let job = Job {
        cancellation,
        reply,
        port: form.values[PORT].clone(),
        project: form.values[PROJECT].clone(),
    };
    std::thread::spawn(move || {
        let result = work(token);
        if let Ok(mut output) = output.lock() {
            *output = Some(result);
        }
    });
    world.get_mut::<Form>(owner).unwrap().job = Some(job);
    status(
        world,
        owner,
        "Local operation running; SDK preparation may take time on first use…",
    );
}

impl crate::native::actions::Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) || world.get::<Form>(owner).is_none()
        {
            return;
        }
        if matches!(self, Self::Cancel) {
            if let Some(job) = world.get::<Form>(owner).unwrap().job.as_ref() {
                job.cancellation.cancel();
            }
            let message = if world.get::<Form>(owner).unwrap().pending.is_some() {
                "A Cell request is already submitted; wait for its confirmation."
            } else {
                "Cancellation requested. A hardware write may need cable recovery."
            };
            status(world, owner, message);
            return;
        }
        if world
            .get::<Form>(owner)
            .is_some_and(|form| form.job.is_some() || form.pending.is_some())
            || world
                .get::<Devices>(owner)
                .is_some_and(|state| state.pending.is_some())
        {
            return;
        }
        capture(world, owner);
        if let Err(error) = apply(world, owner, self) {
            status(world, owner, error);
        }
    }
}

fn apply(world: &mut World, owner: Entity, control: &Control) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, owner)
        && matches!(
            control,
            Control::Ports
                | Control::Probe
                | Control::Profile
                | Control::ChooseProject
                | Control::Inspect
                | Control::SetupWindow
                | Control::Export(_)
                | Control::Compile
                | Control::Flash
                | Control::Provision
                | Control::Status
                | Control::OpenSource
                | Control::OpenResources
        )
    {
        return Err(
            "Local hardware and firmware tools are unavailable in an isolated Laboratory".into(),
        );
    }
    let values = world.get::<Form>(owner).unwrap().values.clone();
    let port = values[PORT].clone();
    let project = PathBuf::from(&values[PROJECT]);
    match control {
        Control::Ports => start(world, owner, |_| {
            lince_ailuros::serial::ports().map(Reply::Ports)
        }),
        Control::Probe => start(world, owner, move |cancel| {
            lince_ailuros::project::probe(&port, Toolchain::Nix, &cancel).map(Reply::Probe)
        }),
        Control::Profile => start(world, owner, move |_| {
            lince_ailuros::project::board(&project).map(Reply::Profile)
        }),
        Control::ChooseProject => start(world, owner, |cancel| {
            cancel.check()?;
            let path = rfd::FileDialog::new()
                .set_title("Choose an empty or existing Lince firmware project folder")
                .pick_folder()
                .ok_or("Folder selection cancelled")?;
            cancel.check()?;
            Ok(Reply::Folder(path))
        }),
        Control::ChoosePort(path) => {
            let field = world.get::<Form>(owner).unwrap().fields[PORT];
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(path);
            capture(world, owner);
        }
        Control::Inspect => start(world, owner, move |cancel| {
            Session::open(&port)?.identity(&cancel).map(Reply::Identity)
        }),
        Control::SetupWindow => {
            let identity = world
                .get::<Form>(owner)
                .unwrap()
                .identity
                .as_ref()
                .ok_or("Inspect installed firmware before opening USB setup")?;
            let expected = identity.public_key.clone();
            start(world, owner, move |cancel| {
                Session::open(&port)?
                    .begin_setup(&expected, &cancel)
                    .map(Reply::Identity)
            });
        }
        Control::Export(board_id) => {
            if values[PROJECT].trim().is_empty() {
                return Err("Choose an empty project folder".into());
            }
            let board_id = board_id.clone();
            start(world, owner, move |_| {
                lince_ailuros::project::export_for_board(&project, &board_id)?;
                lince_ailuros::project::board(&project).map(Reply::Exported)
            });
        }
        Control::Compile => start(world, owner, move |cancel| {
            lince_ailuros::project::compile(&project, Toolchain::Nix, &cancel).map(Reply::Compiled)
        }),
        Control::Flash => {
            let artifact = world
                .get::<Form>(owner)
                .unwrap()
                .artifact
                .clone()
                .ok_or("Compile the selected project first")?;
            if artifact.mode != lince_ailuros::project::FirmwareMode::Workflow {
                return Err("Choose compiled workflow firmware for camera setup; device tests and transport qualification have separate installation commands".into());
            }
            let fingerprint = artifact.fingerprint()?;
            let mac = world
                .get::<Form>(owner)
                .unwrap()
                .probe
                .as_ref()
                .ok_or(
                    "Probe the selected ESP and check its hardware identity before installation",
                )?
                .mac
                .clone();
            start(world, owner, move |cancel| {
                lince_ailuros::project::flash(
                    &project,
                    &artifact,
                    &fingerprint,
                    &port,
                    &mac,
                    Toolchain::Nix,
                    &cancel,
                )
                .map(|()| Reply::Flashed)
            });
        }
        Control::Enroll => {
            let identity = world
                .get::<Form>(owner)
                .unwrap()
                .identity
                .clone()
                .ok_or("Inspect installed Lince firmware first")?;
            backend(
                world,
                owner,
                Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: values[NAME].clone(),
                    public_key: identity.public_key,
                    signal_slug: values[SLUG].clone(),
                    enabled: false,
                    cooldown_seconds: values[INTERVAL]
                        .parse()
                        .map_err(|_| "Enter a numeric reading interval")?,
                    minimum: "-1000000".into(),
                    maximum: "1000000".into(),
                    expires_at: (chrono::Utc::now() + chrono::Duration::days(90)).to_rfc3339(),
                },
                Backend::Enroll,
            )?;
        }
        Control::Select(uid) => {
            world.get_mut::<Form>(owner).unwrap().selected = Some(uid.clone());
            let selection = world.get::<Form>(owner).unwrap().selection;
            world.get_mut::<Text>(selection).unwrap().0 =
                format!("Selected device {uid}. Inspect its firmware before provisioning.");
        }
        Control::Prepare => prepare(world, owner)?,
        Control::Provision => {
            let configuration = configuration(world, owner)?;
            let row = selected(world, owner)?;
            let form = world.get::<Form>(owner).unwrap();
            if (matches!(&configuration.workflow, Workflow::Camera { .. })
                || matches!(&configuration.workflow, Workflow::Custom { output,.. } if output == "jpeg"))
                && !form.streams.iter().any(|stream| {
                    stream.signal_uid == row.signal_uid
                        && stream.value_type == "jpeg"
                        && stream.source_url.is_empty()
                })
            {
                return Err("Prepare the selected video Signal before provisioning".into());
            }
            start(world, owner, move |cancel| {
                Session::open(&port)?
                    .configure(&configuration, &row.public_key, &cancel)
                    .map(|()| Reply::Provisioned {
                        password: configuration.password,
                        endpoint: row.uid,
                        revision: row.revision,
                    })
            });
        }
        Control::Status => {
            let row = selected(world, owner)?;
            start(world, owner, move |cancel| {
                Session::open(&port)?
                    .status(&row.public_key, &cancel)
                    .map(Reply::Status)
            });
        }
        Control::Advanced => {
            let advanced = world.get::<Form>(owner).unwrap().advanced;
            let mut node = world.get_mut::<Node>(advanced).unwrap();
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
        Control::ConfirmPins => {
            let pins: CameraPins = serde_json::from_str(&values[PINS])
                .map_err(|_| "Enter the verified camera pin map in workflow settings first")?;
            pins.validate()?;
            world.get_mut::<Form>(owner).unwrap().confirmed_pins = true;
            status(
                world,
                owner,
                "Camera pin mapping confirmed for this setup. Changing it requires confirmation again.",
            );
        }
        Control::OpenVideo | Control::OpenSource | Control::OpenResources => {
            let (root, workspace, position) = placement(world, owner)?;
            if matches!(control, Control::OpenVideo) {
                let row = selected(world, owner)?;
                let sand = crate::native::sand_store::spawn_sand(
                    world,
                    root,
                    workspace,
                    crate::native::sand_store::SandKind::Video,
                    "",
                    position,
                );
                crate::native::video_castle::open_signal(world, sand, row.signal_uid);
                if let Some(source) = crate::native::practice_cells::source(world, owner) {
                    world
                        .entity_mut(sand)
                        .insert(crate::native::practice_cells::PracticeSource(source));
                }
            } else {
                let root_path = project
                    .canonicalize()
                    .map_err(|_| "Create or select an existing firmware project")?;
                let path = root_path.join(if matches!(control, Control::OpenResources) {
                    "resources.json"
                } else {
                    "main/main.c"
                });
                crate::native::ide::spawn(
                    world,
                    root,
                    workspace,
                    position,
                    crate::native::ide::Ide {
                        explorer: crate::native::file_explorer::FileExplorer {
                            roots: vec![root_path],
                            ..Default::default()
                        },
                        paths: vec![path.clone()],
                        active: Some(path),
                        ..Default::default()
                    },
                );
            }
        }
        Control::Cancel => {}
    }
    Ok(())
}

fn placement(world: &World, owner: Entity) -> Result<(Entity, u64, bevy::math::DVec2), String> {
    let root = world
        .get::<crate::native::sand::InBox>(owner)
        .ok_or("Workspace unavailable")?
        .0;
    let workspace = world
        .get::<crate::native::workspace::WorkspaceMember>(owner)
        .map_or(1, |member| member.0);
    let position = world
        .get::<crate::native::canvas::CanvasItem>(owner)
        .map_or(bevy::math::DVec2::ZERO, |item| {
            item.position + bevy::math::DVec2::new(40.0, 40.0)
        });
    Ok((root, workspace, position))
}

pub(super) fn observe(world: &mut World, owner: Entity, data: &Value) {
    if let Ok(streams) = serde_json::from_value(data["streams"].clone()) {
        if let Some(mut form) = world.get_mut::<Form>(owner) {
            form.streams = streams;
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let found = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .find_map(|(owner, form)| {
            form.pending
                .as_ref()
                .filter(|(pending, _)| pending == id)
                .map(|(_, kind)| (owner, *kind))
        });
    let Some((owner, kind)) = found else {
        return false;
    };
    world.get_mut::<Form>(owner).unwrap().pending = None;
    match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => {
            if let Ok(rows) = serde_json::from_value(data["endpoints"].clone()) {
                let mut state = world.get_mut::<Devices>(owner).unwrap();
                state.rows = rows;
                state.audience = data["audience"].as_str().unwrap_or_default().into();
                super::render(world, owner);
            }
            observe(world, owner, data);
            let result = match kind {
                Backend::Enroll => {
                    if let Some(saved) = data["saved"].as_str() {
                        world.get_mut::<Form>(owner).unwrap().selected = Some(saved.into());
                        prepare(world, owner)
                    } else {
                        Err("Cell did not confirm enrollment".into())
                    }
                }
                Backend::Configure => {
                    selected(world, owner).and_then(|row| {
                        let stream: Option<store::signal_streams::Configuration> = serde_json::from_value(data["stream"].clone())
                            .map_err(|_| "Cell returned an invalid Signal configuration")?;
                        let form = world.get::<Form>(owner).unwrap();
                        let jpeg = form.values[KIND] == "camera" || (form.values[KIND] == "custom" && form.values[CUSTOM_OUTPUT] == "jpeg");
                        if !data.as_object().is_some_and(|data| data.contains_key("stream")) ||
                            (jpeg && !stream.as_ref().is_some_and(|stream| stream.signal_uid == row.signal_uid && stream.value_type == "jpeg" && stream.source_url.is_empty())) ||
                            (!jpeg && stream.is_some()) {
                            return Err("Cell did not confirm the selected workflow's Signal configuration; refresh before provisioning.".into());
                        }
                        let mut form = world.get_mut::<Form>(owner).unwrap();
                        form.streams
                            .retain(|stream| stream.signal_uid != row.signal_uid);
                        if let Some(stream) = stream {
                            form.streams.push(stream);
                        }
                        status(
                            world,
                            owner,
                            "Signal configuration confirmed. Enter Wi-Fi settings and provision the device.",
                        );
                        Ok(())
                    })
                }
                Backend::Enable => {
                    selected(world, owner).and_then(|row| {
                        if !row.enabled || !data["endpoints"].as_array().is_some_and(|rows| rows.iter().any(|saved| saved["uid"] == row.uid && saved["enabled"] == true && saved["public_key"] == row.public_key)) {
                            return Err("Cell did not confirm enabled device access; refresh and review access before continuing.".into());
                        }
                        status(
                            world,
                            owner,
                            "Device configuration and access confirmed. Open Video Castle; the ESP can now use wall USB power.",
                        );
                        Ok(())
                    })
                }
            };
            if let Err(error) = result {
                status(world, owner, error);
            }
        }
        ServerMessage::Error { message, .. } => status(world, owner, message),
        _ => status(
            world,
            owner,
            "Cell response was incomplete; refresh access before continuing",
        ),
    }
    true
}

fn load_profile(world: &mut World, owner: Entity, profile: &lince_ailuros::profile::BoardProfile) {
    let pins = profile
        .camera
        .as_ref()
        .map(|camera| serde_json::to_string(&camera.pins).unwrap_or_default())
        .unwrap_or_default();
    let field = world.get::<Form>(owner).unwrap().fields[PINS];
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(&pins);
    capture(world, owner);
    let mut form = world.get_mut::<Form>(owner).unwrap();
    form.confirmed_pins = false;
    form.artifact = None;
}

pub(super) fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .filter(|(_, form)| {
            form.job
                .as_ref()
                .is_some_and(|job| job.reply.lock().is_ok_and(|output| output.is_some()))
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        capture(world, owner);
        let job = world.get_mut::<Form>(owner).unwrap().job.take().unwrap();
        let result = job.reply.lock().ok().and_then(|mut result| result.take());
        let Some(result) = result else {
            continue;
        };
        if result.is_ok() && job.cancellation.check().is_err() {
            status(
                world,
                owner,
                "Local operation cancelled. The ESP may already have stored configuration; inspect it and review access before enabling.",
            );
            continue;
        }
        let selection_changed = world.get::<Form>(owner).is_some_and(|form| {
            form.values[PORT] != job.port || form.values[PROJECT] != job.project
        });
        if selection_changed {
            status(
                world,
                owner,
                "Device or project selection changed during the operation; inspect and compile again before proceeding.",
            );
            continue;
        }
        match result {
            Err(error) => status(world, owner, error),
            Ok(Reply::Ports(ports)) => {
                let parent = world.get::<Form>(owner).unwrap().ports;
                panel::clear(world, parent);
                for port in ports.iter().take(128) {
                    panel::button(
                        world,
                        parent,
                        owner,
                        &format!("Select {}", port.path),
                        Control::ChoosePort(port.path.clone()),
                    );
                }
                status(
                    world,
                    owner,
                    format!(
                        "Found {} serial devices. Select the connected ESP; identical adapters may share a USB identifier.",
                        ports.len()
                    ),
                );
            }
            Ok(Reply::Probe(chip)) => {
                let label = format!(
                    "Selected {} · MAC {} · flash {} bytes. Install writes firmware to this selected device.",
                    chip.chip, chip.mac, chip.flash_bytes
                );
                world.get_mut::<Form>(owner).unwrap().probe = Some(chip);
                status(world, owner, label);
            }
            Ok(Reply::Folder(path)) => {
                let field = world.get::<Form>(owner).unwrap().fields[PROJECT];
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(&path.to_string_lossy());
                capture(world, owner);
                status(
                    world,
                    owner,
                    "Project folder selected. Create a starter project in an empty folder, or compile an existing one.",
                );
            }
            Ok(Reply::Profile(profile)) => {
                load_profile(world, owner, &profile);
                status(
                    world,
                    owner,
                    format!(
                        "Board profile: {}. Confirm camera wiring only after checking the selected hardware. {}",
                        profile.name, profile.reference
                    ),
                );
            }
            Ok(Reply::Identity(identity)) => {
                let label = format!(
                    "Firmware {} · flash {} bytes · PSRAM {} bytes · public key {} · USB setup {}",
                    identity.firmware,
                    identity.flash_bytes,
                    identity.psram_bytes,
                    identity.public_key,
                    if identity.provisioning {
                        "open"
                    } else {
                        "closed"
                    }
                );
                world.get_mut::<Form>(owner).unwrap().identity = Some(identity);
                status(world, owner, label);
            }
            Ok(Reply::Exported(profile)) => {
                load_profile(world, owner, &profile);
                let kind = if profile.camera.is_some() {
                    "camera"
                } else {
                    "scalar"
                };
                let field = world.get::<Form>(owner).unwrap().fields[KIND];
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(kind);
                capture(world, owner);
                status(
                    world,
                    owner,
                    format!(
                        "Editable {} project created. Review board.json and firmware source, then compile.",
                        profile.name
                    ),
                );
            }
            Ok(Reply::Compiled(artifact)) => {
                let fingerprint = artifact.fingerprint().unwrap_or_default();
                world.get_mut::<Form>(owner).unwrap().artifact = Some(artifact);
                status(
                    world,
                    owner,
                    format!(
                        "Firmware compiled. Artifact {fingerprint}. Check the selected serial device before Install."
                    ),
                );
            }
            Ok(Reply::Flashed) => {
                world.get_mut::<Form>(owner).unwrap().identity = None;
                status(
                    world,
                    owner,
                    "Firmware write confirmed. Inspect the running firmware, then enroll its key.",
                );
            }
            Ok(Reply::Provisioned {
                password,
                endpoint,
                revision,
            }) => {
                clear_confirmed_password(world, owner, &password);
                let result = selected(world, owner).and_then(|row| {
                    if row.uid != endpoint || row.revision != revision { return Err("Configuration stored on the ESP, but access changed during provisioning. Review access before enabling it.".into()); }
                    backend(
                        world,
                        owner,
                        Request::SetEnabled {
                            endpoint: row.uid,
                            expected_revision: row.revision,
                            enabled: true,
                        },
                        Backend::Enable,
                    )
                });
                if let Err(error) = result {
                    status(world, owner, error);
                }
            }
            Ok(Reply::Status(state)) => status(
                world,
                owner,
                format!(
                    "Wi-Fi {} · {}",
                    if state.wifi_connected {
                        "connected"
                    } else {
                        "disconnected"
                    },
                    state.phase
                ),
            ),
        }
    }
}

fn clear_confirmed_password(world: &mut World, owner: Entity, submitted: &str) {
    let form = world.get::<Form>(owner).unwrap();
    if form.values[PASSWORD] != submitted {
        return;
    }
    let field = form.fields[PASSWORD];
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("");
    world.get_mut::<Form>(owner).unwrap().values[PASSWORD].clear();
}

#[cfg(test)]
mod tests;
