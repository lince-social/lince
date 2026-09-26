use bevy::prelude::*;
use lince_media::{
    native::{
        capture::AudioInput,
        preview::{Command, Devices, Preview},
    },
    video::VideoFrame,
};

use crate::{actions::Action, sand_panel as panel};
pub mod calls;

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "str0m and RustCrypto media adapter",
        author: include_str!("communication/vendor/str0m-crypto-NOTICE.txt"),
        license: include_str!("communication/vendor/str0m-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "Opus Rust",
        author: "restsend and the Opus contributors",
        license: include_str!("communication/vendor/opus-rs-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "Sonora audio processing",
        author: "Friedel Ziegelmayer and the WebRTC contributors",
        license: include_str!("communication/vendor/sonora-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "rav1e AV1 encoder",
        author: "The rav1e contributors",
        license: include_str!("communication/vendor/rav1e-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "rav1d AV1 decoder",
        author: "The rav1d and dav1d contributors",
        license: include_str!("communication/vendor/rav1d-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "scrcap screen capture",
        author: "kingwingfly and contributors",
        license: include_str!("communication/vendor/scrcap-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "TURN and STUN protocol",
        author: "Matthew Waters and contributors",
        license: include_str!("communication/vendor/turn-client-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "Rustls TLS",
        author: "The Rustls contributors",
        license: include_str!("communication/vendor/rustls-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "AWS-LC Rust bindings",
        author: "AWS Cryptography and contributors",
        license: include_str!("communication/vendor/aws-lc-rs-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "AWS-LC cryptography",
        author: "AWS Cryptography, BoringSSL, OpenSSL, and contributors",
        license: include_str!("communication/vendor/aws-lc-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "X11RB screen capture",
        author: "The X11RB contributors",
        license: include_str!("communication/vendor/x11rb-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "if-addrs interface discovery",
        author: "MaidSafe.net limited, messense, and contributors",
        license: include_str!("communication/vendor/if-addrs-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "Windows API bindings",
        author: "Microsoft and the windows-rs contributors",
        license: include_str!("communication/vendor/windows-sys-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "WebPKI root certificates",
        author: "The webpki-roots contributors and Mozilla",
        license: include_str!("communication/vendor/webpki-roots-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "flexaudio",
        author: "tubome / Studio Sadola",
        license: include_str!("communication/vendor/flexaudio-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "CPAL",
        author: "The CPAL contributors",
        license: include_str!("communication/vendor/cpal-LICENSE.txt"),
    },
    crate::credits::Attribution {
        name: "nokhwa",
        author: "l1npengtul and contributors",
        license: include_str!("communication/vendor/nokhwa-LICENSE.txt"),
    },
];

#[derive(Component)]
pub struct MediaPreviewSand {
    preview: Preview,
    status: Entity,
    meters: Entity,
    choices: Entity,
    camera: Entity,
    screen: Entity,
    camera_image: Handle<Image>,
    screen_image: Handle<Image>,
    devices: Devices,
    synthetic: bool,
}

pub struct MediaPreviewPlugin;

impl Plugin for MediaPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<Image>>()
            .add_systems(Update, update);
    }
}

pub fn populate(world: &mut World, sand: Entity) -> Result<(), String> {
    let preview = Preview::spawn().map_err(|error| error.to_string())?;
    let body = panel::frame(world, sand, "Local media test");
    crate::edit_mode::label(
        world,
        body,
        "Choose a source to preview. Closing this panel stops capture.",
        13.0,
    );
    let controls = panel::row(world, body);
    for (label, command) in [
        ("Find devices", Command::Devices),
        ("Test video texture", Command::Pattern),
        ("Default microphone", Command::Microphone(None)),
        ("Default camera", Command::Camera("0".into())),
        ("Choose screen", Command::Screen(None)),
        (
            "System audio",
            Command::SharedAudio(AudioInput::System(None)),
        ),
        ("Test speakers", Command::Speaker(None)),
    ] {
        panel::button(world, controls, sand, label, MediaAction(command));
    }
    let stops = panel::row(world, body);
    for (label, command) in [
        ("Stop microphone", Command::StopMicrophone),
        ("Stop camera", Command::StopCamera),
        ("Stop screen", Command::StopScreen),
        ("Stop shared audio", Command::StopSharedAudio),
        ("Volume 0%", Command::Volume(0.0)),
        ("Volume 50%", Command::Volume(0.5)),
        ("Volume 100%", Command::Volume(1.0)),
    ] {
        panel::button(world, stops, sand, label, MediaAction(command));
    }
    let status = crate::edit_mode::label(world, body, "Ready", 13.0);
    let meters = crate::edit_mode::label(world, body, "", 13.0);
    let choices = panel::column(world, body);
    world.entity_mut(choices).insert((
        Node {
            max_height: px(170),
            overflow: Overflow::scroll_y(),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        ScrollPosition::default(),
    ));
    let videos = panel::row(world, body);
    let camera = screen(world, videos, "Camera preview");
    let screen = screen(world, videos, "Screen preview");
    panel::credits(world, controls, body, CREDITS);
    world.entity_mut(sand).insert((
        MediaPreviewSand {
            preview,
            status,
            meters,
            choices,
            camera,
            screen,
            camera_image: Handle::default(),
            screen_image: Handle::default(),
            devices: Devices::default(),
            synthetic: false,
        },
        crate::sand_store::SandCredits(CREDITS),
    ));
    Ok(())
}

fn screen(world: &mut World, parent: Entity, name: &str) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(47),
                aspect_ratio: Some(16.0 / 9.0),
                min_height: px(100),
                ..default()
            },
            BackgroundColor(Color::BLACK),
            crate::icons::Tooltip(name.into()),
        ))
        .id()
}

#[derive(Clone)]
struct MediaAction(Command);

impl Action for MediaAction {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::active(world) {
            return;
        }
        let Some(mut sand) = world.get_mut::<MediaPreviewSand>(owner) else {
            return;
        };
        let status = sand.status;
        let command = self.0.clone();
        match &command {
            Command::Pattern => sand.synthetic = true,
            Command::StopCamera | Command::Camera(_) => sand.synthetic = false,
            _ => {}
        }
        if let Err(error) = sand.preview.send(command) {
            panel::status(world, status, error.to_string());
        }
    }
}

fn update(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<MediaPreviewSand>>()
        .iter(world)
        .collect();
    for owner in entities {
        let Some(mut sand) = world.entity_mut(owner).take::<MediaPreviewSand>() else {
            continue;
        };
        let state = sand.preview.status();
        panel::status(world, sand.status, state.message);
        panel::status(
            world,
            sand.meters,
            format!(
                "Microphone: {} · {:.0}%    Shared audio: {} · {:.0}%    Camera: {}    Screen: {}",
                if state.microphone { "on" } else { "off" },
                state.microphone_peak * 100.0,
                if state.shared_audio { "on" } else { "off" },
                state.shared_audio_peak * 100.0,
                if state.camera { "on" } else { "off" },
                if state.screen { "on" } else { "off" }
            ),
        );
        if state.devices != sand.devices {
            panel::clear(world, sand.choices);
            for choice in &state.devices.microphones {
                panel::button(
                    world,
                    sand.choices,
                    owner,
                    &format!("Microphone: {}", choice.label),
                    MediaAction(Command::Microphone(Some(choice.id.clone()))),
                );
            }
            for choice in &state.devices.speakers {
                panel::button(
                    world,
                    sand.choices,
                    owner,
                    &format!("Speaker: {}", choice.label),
                    MediaAction(Command::Speaker(Some(choice.id.clone()))),
                );
            }
            for choice in &state.devices.cameras {
                panel::button(
                    world,
                    sand.choices,
                    owner,
                    &format!("Camera: {}", choice.label),
                    MediaAction(Command::Camera(choice.id.clone())),
                );
            }
            for choice in &state.devices.screens {
                if let Ok(id) = choice.id.parse() {
                    panel::button(
                        world,
                        sand.choices,
                        owner,
                        &format!("Screen: {}", choice.label),
                        MediaAction(Command::Screen(Some(id))),
                    );
                }
            }
            for choice in &state.devices.applications {
                if let Ok(pid) = choice.id.parse() {
                    panel::button(
                        world,
                        sand.choices,
                        owner,
                        &format!("Application audio: {}", choice.label),
                        MediaAction(Command::SharedAudio(AudioInput::Application(pid))),
                    );
                }
            }
            sand.devices = state.devices;
        }
        if let Some(frame) = sand.preview.camera.take()
            && (state.camera || sand.synthetic)
        {
            texture(world, sand.camera, &mut sand.camera_image, frame);
        }
        if let Some(frame) = sand.preview.screen.take()
            && state.screen
        {
            texture(world, sand.screen, &mut sand.screen_image, frame);
        }
        if !state.camera && !sand.synthetic {
            world.entity_mut(sand.camera).remove::<ImageNode>();
        }
        if !state.screen {
            world.entity_mut(sand.screen).remove::<ImageNode>();
        }
        world.entity_mut(owner).insert(sand);
    }
}

pub fn texture(world: &mut World, entity: Entity, handle: &mut Handle<Image>, frame: VideoFrame) {
    let image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: frame.width(),
            height: frame.height(),
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        frame.into_rgba(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    let mut images = world.resource_mut::<Assets<Image>>();
    if let Some(mut existing) = images.get_mut(&*handle) {
        *existing = image;
    } else {
        *handle = images.add(image);
    }
    world
        .entity_mut(entity)
        .insert(ImageNode::new(handle.clone()));
}
