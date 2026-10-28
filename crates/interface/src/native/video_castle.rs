use crate::native::sand_panel as panel;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub source: String,
    pub file: bool,
    pub paused: bool,
    pub position_ms: u64,
    #[serde(default)]
    pub resource: Option<lince_resources::Resource>,
    #[serde(default)]
    pub muted: bool,
}

impl Settings {
    pub fn valid(&self) -> bool {
        self.source.len() <= 4096
            && !self.source.contains('\0')
            && self
                .resource
                .as_ref()
                .is_none_or(|resource| resource.validate().is_ok())
    }
}

#[derive(Component)]
struct View {
    root: Entity,
    sand: Entity,
    source: Entity,
    url: Entity,
    seek: Entity,
    status: Entity,
    image: Entity,
    viewport: Entity,
    handle: Handle<Image>,
    ratio: f32,
    session: Option<Session>,
    last: Option<std::time::Instant>,
    skipped: u64,
}

struct Frame {
    pixels: Option<crate::native::media_sand::Pixels>,
    status: String,
    position_ms: Option<u64>,
    skipped: u64,
}

type Reply = Arc<Mutex<Option<Frame>>>;

#[derive(Component)]
struct EmbeddedPoster(Entity);

struct Session {
    task: tokio::task::JoinHandle<()>,
    reply: Reply,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.task.abort();
    }
}

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "FFmpeg",
        author: "The FFmpeg developers · https://ffmpeg.org · external media decoder",
        license: include_str!("../../licenses/native/ffmpeg-GPLv3.txt"),
    },
    crate::native::credits::Attribution {
        name: "CPAL",
        author: "The CPAL contributors",
        license: include_str!("../../licenses/native/cpal-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "BLAKE3",
        author: "Jack O'Connor, Samuel Neves and contributors",
        license: include_str!("../../licenses/native/document/blake3-CC0.txt"),
    },
    crate::native::credits::Attribution {
        name: "Reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../../licenses/native/reqwest-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Tokio",
        author: "Tokio contributors",
        license: include_str!("../../licenses/native/tokio-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Futures",
        author: "The Rust Project Developers",
        license: include_str!("../../licenses/native/futures-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Tokio Tungstenite",
        author: "Tokio Tungstenite contributors",
        license: include_str!("../../licenses/native/tokio-tungstenite-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Tungstenite",
        author: "Tungstenite contributors",
        license: include_str!("../../licenses/native/tungstenite-MIT.txt"),
    },
];

pub struct VideoCastlePlugin;

impl Plugin for VideoCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update.after(crate::native::workspace::PrepareWorkspaces),
        )
        .add_systems(PostUpdate, fit.before(bevy::ui::UiSystems::Layout));
    }
}

pub fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    if world.get::<Settings>(sand).is_none() {
        world.entity_mut(sand).insert(Settings::default());
    }
    let settings = world.get::<Settings>(sand).unwrap().clone();
    let body = panel::frame(world, sand, "Video Castle");
    let source = panel::field(world, body, "Signal slug or video file", &settings.source);
    let row = panel::row(world, body);
    panel::button(world, row, body, "Open Signal", Control::Open(false));
    panel::button(
        world,
        row,
        body,
        "Open video file / URL",
        Control::Open(true),
    );
    panel::button(world, row, body, "Mute / unmute", Control::Mute);
    panel::button(world, row, body, "Pause / resume", Control::Pause);
    let seek = panel::field(
        world,
        body,
        "Video position (seconds)",
        &(settings.position_ms / 1000).to_string(),
    );
    panel::button(world, body, body, "Seek video", Control::Seek);
    let setup = panel::column(world, body);
    let url = panel::field(world, setup, "Upstream WebSocket URL (optional)", "");
    let row = panel::row(world, setup);
    panel::button(
        world,
        row,
        body,
        "Use memory frames",
        Control::Configure(Some("jpeg")),
    );
    panel::button(
        world,
        row,
        body,
        "Use memory scalar",
        Control::Configure(Some("scalar")),
    );
    panel::button(
        world,
        row,
        body,
        "Use disk readings",
        Control::Configure(None),
    );
    let status = crate::native::edit_mode::label(world, body, "Choose a Signal or GIF video", 13.0);
    let viewport = world
        .spawn((
            ChildOf(body),
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(80),
                overflow: Overflow::clip(),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .id();
    let image = world
        .spawn((
            ChildOf(viewport),
            ImageNode::default(),
            Node {
                width: px(1),
                height: px(1),
                ..default()
            },
        ))
        .id();
    panel::credits(world, body, body, CREDITS);
    world.entity_mut(sand).insert((
        crate::native::castle::Castle,
        crate::native::sand_store::SandCredits(CREDITS),
    ));
    world.entity_mut(body).insert(View {
        root,
        sand,
        source,
        url,
        seek,
        status,
        image,
        viewport,
        handle: Handle::default(),
        ratio: 1.0,
        session: None,
        last: None,
        skipped: 0,
    });
    body
}

pub(crate) fn open_signal(world: &mut World, sand: Entity, source: String) {
    restore(
        world,
        sand,
        Settings {
            source,
            ..Default::default()
        },
    );
}

pub(crate) fn embedded(
    world: &mut World,
    frame: crate::native::embedded_media::Frame,
    resource: lince_resources::Resource,
) {
    let body = frame.content;
    let controls = frame.controls;
    let source = panel::field(world, controls, "Media source", &resource.uri);
    let seek = panel::field(
        world,
        controls,
        "Position (seconds)",
        &(resource.locator.start_ms.unwrap_or(0) / 1000).to_string(),
    );
    let url = panel::field(world, controls, "Upstream URL", "");
    let status = crate::native::edit_mode::label(world, controls, "Paused", 13.0);
    let row = panel::row(world, controls);
    panel::button(world, row, body, "Play / pause", Control::Pause);
    panel::button(world, row, body, "Mute / unmute", Control::Mute);
    panel::button(world, row, body, "Seek", Control::Seek);
    panel::credits(world, controls, controls, CREDITS);
    let audio = resource.media_type.starts_with("audio/");
    let viewport = world
        .spawn((
            Node {
                width: percent(100),
                height: px(if audio { 48.0 } else { 360.0 }),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                overflow: Overflow::clip(),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    let image = world
        .spawn((
            ImageNode::default(),
            Node {
                width: px(1),
                height: px(1),
                ..default()
            },
            ChildOf(viewport),
        ))
        .id();
    if audio {
        crate::native::edit_mode::label(
            world,
            viewport,
            "Audio · right-click for playback controls",
            14.0,
        );
    } else {
        let poster = panel::column(world, viewport);
        world.get_mut::<Node>(poster).unwrap().position_type = PositionType::Absolute;
        world.get_mut::<Node>(poster).unwrap().width = percent(100);
        world.get_mut::<Node>(poster).unwrap().height = percent(100);
        crate::native::resource_preview::spawn(world, poster, &resource);
        world.entity_mut(body).insert(EmbeddedPoster(poster));
    }
    world.entity_mut(body).insert((
        Settings {
            source: resource.uri.clone(),
            file: true,
            paused: true,
            position_ms: resource.locator.start_ms.unwrap_or(0),
            resource: Some(resource),
            ..default()
        },
        crate::native::sand_store::SandCredits(CREDITS),
        View {
            root: frame.owner,
            sand: body,
            source,
            url,
            seek,
            status,
            image,
            viewport,
            handle: Handle::default(),
            ratio: 16.0 / 9.0,
            session: None,
            last: None,
            skipped: 0,
        },
    ));
}

pub(crate) fn restore(world: &mut World, sand: Entity, settings: Settings) {
    let views: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter(|(_, view)| view.sand == sand)
        .map(|(owner, view)| (owner, view.source, view.seek))
        .collect();
    for (owner, source, seek) in views {
        stop(world, owner);
        if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(source) {
            text.editor.set_text(&settings.source);
        }
        if let Some(mut text) = world.get_mut::<bevy::text::EditableText>(seek) {
            text.editor
                .set_text(&(settings.position_ms / 1000).to_string());
        }
    }
    world.entity_mut(sand).insert(settings);
}

#[derive(Clone)]
enum Control {
    Open(bool),
    Pause,
    Mute,
    Seek,
    Configure(Option<&'static str>),
}

impl crate::native::actions::Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let (sand, source, url, seek, status) =
            (view.sand, view.source, view.url, view.seek, view.status);
        if crate::native::laboratory::blocked(world, owner) {
            return;
        }
        let source = match panel::value(world, source) {
            Ok(value) => value.trim().to_string(),
            Err(error) => {
                set_status(world, status, &error);
                return;
            }
        };
        match self {
            Self::Open(file) => {
                world.entity_mut(sand).insert(Settings {
                    source,
                    file: *file,
                    paused: false,
                    position_ms: 0,
                    ..default()
                });
                stop(world, owner);
            }
            Self::Pause => {
                if let Some(mut settings) = world.get_mut::<Settings>(sand) {
                    settings.paused = !settings.paused;
                }
                stop(world, owner);
            }
            Self::Mute => {
                if let Some(mut settings) = world.get_mut::<Settings>(sand) {
                    settings.muted = !settings.muted;
                }
                stop(world, owner);
            }
            Self::Seek => {
                let seconds = panel::value(world, seek)
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|seconds| *seconds <= 86400);
                if let Some(seconds) = seconds {
                    if let Some(mut settings) = world.get_mut::<Settings>(sand) {
                        settings.position_ms = seconds * 1000;
                    }
                    stop(world, owner);
                } else {
                    set_status(
                        world,
                        status,
                        "Enter a video position from 0 to 86400 seconds",
                    );
                }
            }
            Self::Configure(kind) => {
                let url = panel::value(world, url).unwrap_or_default();
                let Some(runtime) = world
                    .get_resource::<crate::native::app::CellHandle>()
                    .cloned()
                else {
                    return;
                };
                let Ok(handle) = tokio::runtime::Handle::try_current() else {
                    return;
                };
                stop(world, owner);
                world.entity_mut(sand).insert(Settings {
                    source: source.clone(),
                    file: false,
                    paused: true,
                    position_ms: 0,
                    ..default()
                });
                let reply: Reply = Default::default();
                let output = reply.clone();
                let wake = world
                    .get_resource::<crate::native::wake::WakeSignal>()
                    .cloned();
                let kind = kind.map(str::to_string);
                let task = handle.spawn(async move {
                    let result = async {
                        let record = store::records::resolve(
                            &runtime.0.store.pool,
                            source.trim_start_matches('@'),
                        )
                        .await?
                        .ok_or_else(|| engine::signal_streams::invalid("Signal not found"))?;
                        let mut connection = runtime.0.store.pool.acquire().await?;
                        let previous =
                            store::signal_streams::get_on(&mut connection, &record.uid).await?;
                        drop(connection);
                        runtime
                            .0
                            .engine
                            .act(
                                engine::actions::Action::SignalEndpoints {
                                    request: engine::signal_endpoints::Request::ConfigureStream {
                                        signal: record.uid,
                                        value_type: kind,
                                        source_url: url,
                                        expected_revision: previous.map(|config| config.revision),
                                    },
                                },
                                None,
                            )
                            .await?;
                        Ok::<_, engine::EngineError>(())
                    }
                    .await;
                    publish(
                        &output,
                        Frame {
                            pixels: None,
                            status: result.map_or_else(
                                |error| error.to_string(),
                                |_| "Storage configured. Resume to view.".into(),
                            ),
                            position_ms: None,
                            skipped: 0,
                        },
                        &wake,
                    );
                });
                world.get_mut::<View>(owner).unwrap().session = Some(Session {
                    task,
                    reply,
                    cancel: Default::default(),
                });
            }
        }
    }
}

fn set_status(world: &mut World, entity: Entity, status: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity) {
        text.0 = status.into();
    }
}

fn stop(world: &mut World, owner: Entity) {
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    let root = view.root;
    view.session = None;
    if world
        .get::<crate::native::embedded_media::Media>(root)
        .is_none()
    {
        clear_image(world, owner);
    }
}

fn clear_image(world: &mut World, owner: Entity) {
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    view.last = None;
    let image = view.image;
    let handle = std::mem::take(&mut view.handle);
    if let Some(mut node) = world.get_mut::<ImageNode>(image) {
        node.image = Handle::default();
    }
    if handle != Handle::default() {
        world.resource_mut::<Assets<Image>>().remove(handle.id());
    }
}

fn visible(world: &mut World, root: Entity, sand: Entity) -> bool {
    if crate::native::laboratory::blocked(world, sand) {
        return false;
    }
    if world
        .get::<crate::native::embedded_media::Media>(root)
        .is_some()
    {
        return world
            .get::<ComputedNode>(sand)
            .is_some_and(|node| !node.is_empty())
            && world
                .get::<InheritedVisibility>(sand)
                .is_none_or(|value| value.get());
    }
    if world
        .get::<Node>(sand)
        .is_some_and(|node| node.display == Display::None)
    {
        return false;
    }
    if let Some(member) = world.get::<crate::native::workspace::WorkspaceMember>(sand)
        && let Some(views) = world
            .get_resource::<crate::native::output_views::OutputViews>()
            .filter(|views| views.applies_to(root))
    {
        let Some(output) = views
            .outputs
            .iter()
            .find(|output| output.workspace == member.0 && output.usable.is_some())
        else {
            return false;
        };
        let Some(item) = world.get::<crate::native::canvas::CanvasItem>(sand) else {
            return false;
        };
        let canvas = crate::native::output_views::canvas(world, root, member.0);
        return canvas
            .screen_position(
                item,
                Vec2::new(
                    output.bounds.size()[0] as f32,
                    output.bounds.size()[1] as f32,
                ),
            )
            .is_some();
    }
    if world
        .get::<crate::native::workspace::WorkspaceMember>(sand)
        .is_some_and(|member| {
            world
                .get::<crate::native::workspace::Workspaces>(root)
                .is_none_or(|spaces| spaces.active != member.0)
        })
    {
        return false;
    }
    let mut windows =
        world.query_filtered::<(Entity, &Window), With<bevy::window::PrimaryWindow>>();
    let Some((entity, window)) = windows.iter(world).next() else {
        return false;
    };
    let minimized = bevy::winit::WINIT_WINDOWS.with(|windows| {
        windows
            .borrow()
            .get_window(entity)
            .is_some_and(|window| window.is_minimized().unwrap_or(false))
    });
    if !window.visible || minimized {
        return false;
    }
    let Some(canvas) = world.get::<crate::native::canvas::CanvasView>(root) else {
        return false;
    };
    let Some(item) = world.get::<crate::native::canvas::CanvasItem>(sand) else {
        return false;
    };
    let viewport = world
        .get::<ComputedNode>(root)
        .map(|node| node.size() * node.inverse_scale_factor())
        .unwrap_or(Vec2::ZERO);
    canvas.screen_position(item, viewport).is_some()
}

fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        let view = world.get::<View>(owner).unwrap();
        let settings = world
            .get::<Settings>(view.sand)
            .cloned()
            .unwrap_or_default();
        let (root, sand) = (view.root, view.sand);
        let active = visible(world, root, sand) && !settings.paused && !settings.source.is_empty();
        let view = world.get::<View>(owner).unwrap();
        let configuring = settings.paused
            && view
                .session
                .as_ref()
                .is_some_and(|session| !session.task.is_finished());
        if !active && !configuring && view.session.is_some() && !settings.paused {
            stop(world, owner);
        }
        let view = world.get::<View>(owner).unwrap();
        if let Some(frame) = view
            .session
            .as_ref()
            .and_then(|session| session.reply.lock().unwrap().take())
        {
            let (status, image, sand, old) =
                (view.status, view.image, view.sand, view.handle.clone());
            set_status(world, status, &frame.status);
            if let Some(position) = frame.position_ms
                && let Some(mut settings) = world.get_mut::<Settings>(sand)
            {
                settings.position_ms = position;
            }
            if let Some(pixels) = frame.pixels {
                if let Some(poster) = world.entity_mut(owner).take::<EmbeddedPoster>() {
                    world.despawn(poster.0);
                }
                let ratio = pixels.width as f32 / pixels.height as f32;
                let replacement = upload(world, old.clone(), pixels);
                if let Some(mut node) = world.get_mut::<ImageNode>(image) {
                    node.image = replacement.clone();
                }
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.handle = replacement;
                view.ratio = ratio;
                view.last = Some(std::time::Instant::now());
                view.skipped = frame.skipped;
            } else if !settings.file {
                clear_image(world, owner);
            }
        }
        let view = world.get::<View>(owner).unwrap();
        if !active {
            if view.session.is_none() {
                set_status(
                    world,
                    view.status,
                    if settings.paused { "Paused" } else { "Idle" },
                );
            }
            continue;
        }
        if view.session.is_none() {
            let Some(runtime) = world
                .get_resource::<crate::native::app::CellHandle>()
                .cloned()
            else {
                continue;
            };
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                continue;
            };
            let reply: Reply = Default::default();
            let output = reply.clone();
            let wake = world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned();
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stopped = cancel.clone();
            let task = handle.spawn(async move {
                let result = if settings.file {
                    if settings.resource.is_none()
                        && settings.source.to_ascii_lowercase().ends_with(".gif")
                    {
                        file::play(&settings, &output, &wake).await
                    } else {
                        standard::play(settings, output.clone(), wake.clone(), stopped).await
                    }
                } else {
                    watch_signal(runtime.0.engine, settings.source, &output, &wake).await
                };
                if let Err(error) = result {
                    publish(
                        &output,
                        Frame {
                            pixels: None,
                            status: error,
                            position_ms: None,
                            skipped: 0,
                        },
                        &wake,
                    );
                }
            });
            world.get_mut::<View>(owner).unwrap().session = Some(Session {
                task,
                reply,
                cancel,
            });
        } else if view
            .last
            .is_some_and(|last| last.elapsed() > Duration::from_secs(2))
            && !settings.file
        {
            let status = view.status;
            let age = view.last.unwrap().elapsed().as_secs();
            set_status(
                world,
                status,
                &format!("Stale · {age}s since frame · {} skipped", view.skipped),
            );
        }
    }
}

fn publish(output: &Reply, frame: Frame, wake: &Option<crate::native::wake::WakeSignal>) {
    *output.lock().unwrap() = Some(frame);
    if let Some(wake) = wake {
        wake.ring();
    }
}

async fn watch_signal(
    engine: Arc<engine::Engine>,
    source: String,
    output: &Reply,
    wake: &Option<crate::native::wake::WakeSignal>,
) -> Result<(), String> {
    let record = store::records::resolve(&engine.store.pool, source.trim_start_matches('@'))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Signal not found")?;
    let channel = engine
        .stream_channel(&record.uid, None)
        .await
        .map_err(|error| error.to_string())?;
    let mut subscription = channel.subscribe().map_err(|error| error.to_string())?;
    let mut sequence = 0_u64;
    let mut epoch = String::new();
    let mut skipped = 0_u64;
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    let mut authority = engine.watch_query_changes();
    loop {
        let snapshot = subscription.updates.borrow_and_update().clone();
        if snapshot.status == "Configuration changed" {
            return Err("Signal configuration changed. Reopen it.".into());
        }
        if snapshot.epoch != epoch {
            sequence = 0;
            epoch = snapshot.epoch;
        }
        if let Some(value) = snapshot.value {
            if value.metadata.sequence != sequence {
                skipped = skipped.saturating_add(
                    value
                        .metadata
                        .sequence
                        .saturating_sub(sequence.saturating_add(1)),
                );
                sequence = value.metadata.sequence;
                let (pixels, status) = if value.metadata.content_type == "image/jpeg" {
                    let bytes = value.bytes.clone();
                    let (width, height, rgba) = tokio::task::spawn_blocking(move || {
                        engine::signal_streams::decode_jpeg(&bytes)
                    })
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                    (
                        Some(crate::native::media_sand::Pixels {
                            width,
                            height,
                            rgba,
                        }),
                        format!("Live · frame {sequence} · {skipped} skipped"),
                    )
                } else {
                    (
                        None,
                        format!("Value: {}", String::from_utf8_lossy(&value.bytes)),
                    )
                };
                publish(
                    output,
                    Frame {
                        pixels,
                        status,
                        position_ms: None,
                        skipped,
                    },
                    wake,
                );
            }
        } else {
            publish(
                output,
                Frame {
                    pixels: None,
                    status: snapshot.status,
                    position_ms: None,
                    skipped,
                },
                wake,
            );
        }
        tokio::select! {
            result = subscription.updates.changed() => { result.map_err(|_| "Stream closed")?; }
            _ = authority.changed() => {
                let current = engine.stream_channel(&record.uid, None).await.map_err(|error| error.to_string())?;
                if !Arc::ptr_eq(&current, &channel) { return Err("Signal changed".into()); }
            }
            _ = timer.tick() => {
                let current = engine.stream_channel(&record.uid, None).await.map_err(|error| error.to_string())?;
                if !Arc::ptr_eq(&current, &channel) { return Err("Signal changed".into()); }
                if let Some(wake) = wake { wake.ring(); }
            }
        }
    }
}

fn fit(views: Query<&View>, viewports: Query<&ComputedNode>, mut images: Query<&mut Node>) {
    for view in &views {
        let Ok(viewport) = viewports.get(view.viewport) else {
            continue;
        };
        let size = viewport.size() * viewport.inverse_scale_factor();
        if size.min_element() <= 0.0 {
            continue;
        }
        let width = size.x.min(size.y * view.ratio);
        if let Ok(mut node) = images.get_mut(view.image) {
            node.width = px(width);
            node.height = px(width / view.ratio);
        }
    }
}

fn upload(
    world: &mut World,
    handle: Handle<Image>,
    pixels: crate::native::media_sand::Pixels,
) -> Handle<Image> {
    let image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: pixels.width,
            height: pixels.height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        pixels.rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    let mut images = world.resource_mut::<Assets<Image>>();
    if handle != Handle::default()
        && let Some(mut current) = images.get_mut(&handle)
    {
        *current = image;
        handle
    } else {
        images.add(image)
    }
}

mod file;
mod standard;
#[cfg(test)]
mod tests;
