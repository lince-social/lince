use bevy::{
    asset::RenderAssetUsages,
    math::DVec2,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::Path,
    sync::{Mutex, mpsc},
};

pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024;
const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and contributors",
        license: crate::native::credits::CHRONO_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Tempfile",
        author: "Steven Allen and contributors",
        license: include_str!("../../licenses/native/document/tempfile-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "BLAKE3",
        author: "Jack O'Connor, Samuel Neves and contributors",
        license: include_str!("../../licenses/native/document/blake3-CC0.txt"),
    },
    crate::native::credits::Attribution {
        name: "Image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Reqwest",
        author: "Sean McArthur and the Reqwest contributors",
        license: include_str!("../../licenses/native/reqwest-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "URL",
        author: "The rust-url developers",
        license: include_str!("../../licenses/native/url-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
];

#[derive(Component, Clone, Serialize, Deserialize)]
pub enum MediaSand {
    Image { path: String },
    ImageSource { resource: lince_resources::Resource },
    Link { url: String },
}

impl MediaSand {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Image { path } => !path.is_empty() && path.len() <= 4096 && !path.contains('\0'),
            Self::ImageSource { resource } => {
                resource.media_type.starts_with("image/") && resource.validate().is_ok()
            }
            Self::Link { url } => web_url(url).is_ok(),
        }
    }
}

pub(crate) fn web_url(source: &str) -> Result<reqwest::Url, String> {
    if source.len() > 4096 || source.chars().any(char::is_control) {
        return Err("The URL is too long or contains control characters.".into());
    }
    let url = reqwest::Url::parse(source).map_err(|_| "Enter a valid HTTP or HTTPS URL.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use HTTP or HTTPS without a username or password in the URL.".into());
    }
    Ok(url)
}

pub(crate) struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub(crate) fn open_file(path: &Path, limit: u64) -> Result<std::fs::File, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let file = options.open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "Choose a regular file of at most {} MiB.",
            limit / 1024 / 1024
        ));
    }
    Ok(file)
}

pub(crate) fn decode(path: &Path) -> Result<Pixels, String> {
    let file = open_file(path, MAX_BYTES)?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Images are limited to 64 MiB.".into());
    }
    decode_bytes(bytes)
}

fn decode_bytes(bytes: Vec<u8>) -> Result<Pixels, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(
        reader.format(),
        Some(
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP
        )
    ) {
        return Err(
            "The Image Sand accepts PNG, JPEG, GIF and WebP. GIF shows its first frame.".into(),
        );
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("Cannot decode image: {error}"))?;
    let image = if image.width() > 2048 || image.height() > 2048 {
        image.thumbnail(2048, 2048)
    } else {
        image
    }
    .into_rgba8();
    Ok(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

pub(crate) fn image(world: &mut World, pixels: Pixels) -> Handle<Image> {
    world.resource_mut::<Assets<Image>>().add(Image::new(
        Extent3d {
            width: pixels.width,
            height: pixels.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ))
}

pub(crate) fn decode_resource(
    library: &lince_resources::Library,
    resource: &lince_resources::Resource,
) -> Result<Pixels, String> {
    let mut reader = lince_resources::RangeReader::open(library.clone(), resource)?;
    if reader.len() > MAX_BYTES {
        return Err("Images are limited to 64 MiB".into());
    }
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if resource
        .hash
        .as_ref()
        .is_some_and(|hash| !hash.eq_ignore_ascii_case(&lince_resources::fingerprint(&bytes)))
    {
        return Err("The image does not match its content hash".into());
    }
    let pixels = decode_bytes(bytes)?;
    if let Some(region) = resource.locator.region {
        let tile = lince_document::crop(
            lince_document::Tile {
                width: pixels.width,
                height: pixels.height,
                rgba: pixels.rgba,
            },
            region,
        )?;
        return Ok(Pixels {
            width: tile.width,
            height: tile.height,
            rgba: tile.rgba,
        });
    }
    Ok(pixels)
}

#[derive(Component)]
struct View {
    image: Entity,
    viewport: Entity,
    status: Entity,
    requested: bool,
    ratio: f32,
}

type Reply = (Entity, Result<Pixels, String>);

#[derive(Resource)]
struct Worker {
    sender: mpsc::SyncSender<(Entity, MediaSand, Option<crate::native::wake::WakeSignal>)>,
    replies: Mutex<mpsc::Receiver<Reply>>,
}

impl Default for Worker {
    fn default() -> Self {
        let (sender, jobs) =
            mpsc::sync_channel::<(Entity, MediaSand, Option<crate::native::wake::WakeSignal>)>(4);
        let (output, replies) = mpsc::sync_channel(2);
        std::thread::spawn(move || {
            while let Ok((owner, sand, wake)) = jobs.recv() {
                let result = match sand {
                    MediaSand::Image { path } => decode(Path::new(&path)),
                    MediaSand::ImageSource { resource } => engine::sources::library()
                        .map_err(|error| error.to_string())
                        .and_then(|library| decode_resource(&library, &resource)),
                    MediaSand::Link { .. } => Err("Choose an image source".into()),
                };
                if output.send((owner, result)).is_err() {
                    break;
                }
                if let Some(wake) = wake {
                    wake.ring();
                }
            }
        });
        Self {
            sender,
            replies: Mutex::new(replies),
        }
    }
}

pub struct MediaSandPlugin;

impl Plugin for MediaSandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Worker>()
            .init_resource::<Assets<Image>>()
            .add_systems(
                Update,
                update.after(crate::native::workspace::PrepareWorkspaces),
            )
            .add_systems(PostUpdate, fit.before(bevy::ui::UiSystems::Layout));
    }
}

fn update(world: &mut World) {
    let replies: Vec<_> = world
        .resource::<Worker>()
        .replies
        .lock()
        .unwrap()
        .try_iter()
        .collect();
    for (owner, result) in replies {
        match result {
            Ok(pixels) => show(world, owner, pixels),
            Err(error) => {
                if let Some(status) = world.get::<View>(owner).map(|view| view.status)
                    && let Some(mut text) = world.get_mut::<Text>(status)
                {
                    text.0 = error;
                }
            }
        }
    }
    let pending: Vec<_> = world
        .query::<(Entity, &MediaSand, &View)>()
        .iter(world)
        .filter_map(|(entity, sand, view)| match sand {
            MediaSand::Image { .. } | MediaSand::ImageSource { .. }
                if !view.requested && !crate::native::laboratory::suspended(world, entity) =>
            {
                Some((entity, sand.clone()))
            }
            _ => None,
        })
        .collect();
    for (entity, sand) in pending {
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
        if world
            .resource::<Worker>()
            .sender
            .try_send((entity, sand, wake.clone()))
            .is_ok()
        {
            world.get_mut::<View>(entity).unwrap().requested = true;
        } else if let Some(wake) = wake {
            wake.ring();
        }
    }
}

pub(crate) fn show(world: &mut World, owner: Entity, pixels: Pixels) {
    let Some(view) = world.get::<View>(owner) else {
        return;
    };
    let (target, status) = (view.image, view.status);
    let dimensions = format!("{} × {} · Display only", pixels.width, pixels.height);
    world.get_mut::<View>(owner).unwrap().ratio = pixels.width as f32 / pixels.height as f32;
    let handle = image(world, pixels);
    world.entity_mut(target).insert(ImageNode::new(handle));
    world.get_mut::<Text>(status).unwrap().0 = dimensions;
    world.get_mut::<View>(owner).unwrap().requested = true;
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    sand: MediaSand,
) -> Entity {
    let is_image = matches!(
        sand,
        MediaSand::Image { .. } | MediaSand::ImageSource { .. }
    );
    let title = if is_image { "Image Sand" } else { "Link Sand" };
    let size = if is_image {
        Vec2::new(520.0, 420.0)
    } else {
        Vec2::new(460.0, 150.0)
    };
    let owner = world
        .spawn((
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(CREDITS),
            crate::native::canvas::CanvasItem { position, size },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            sand.clone(),
        ))
        .id();
    crate::native::edit_mode::label(world, owner, title, 22.0);
    match sand {
        image @ (MediaSand::Image { .. } | MediaSand::ImageSource { .. }) => {
            let name = match &image {
                MediaSand::Image { path } => Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                MediaSand::ImageSource { resource } => resource.title.clone(),
                MediaSand::Link { .. } => unreachable!(),
            };
            crate::native::edit_mode::label(world, owner, &name, 13.0);
            let status = crate::native::edit_mode::label(world, owner, "Opening image…", 12.0);
            let viewport = world
                .spawn((
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        min_height: px(0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    ChildOf(owner),
                ))
                .id();
            let target = world
                .spawn((
                    crate::native::sand::ImageSand,
                    Node {
                        width: px(1),
                        height: px(1),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode {
                        image_mode: bevy::ui::widget::NodeImageMode::Auto,
                        ..default()
                    },
                    ChildOf(owner),
                ))
                .id();
            world.entity_mut(target).insert(ChildOf(viewport));
            world.entity_mut(owner).insert(View {
                image: target,
                viewport,
                status,
                requested: false,
                ratio: 1.0,
            });
        }
        MediaSand::Link { url } => {
            crate::native::edit_mode::label(world, owner, &url, 14.0);
            crate::native::castle_feed::button(world, owner, owner, "Open in browser", OpenLink);
        }
    }
    owner
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
        let height = width / view.ratio;
        if let Ok(mut node) = images.get_mut(view.image)
            && (node.width != px(width) || node.height != px(height))
        {
            node.width = px(width);
            node.height = px(height);
        }
    }
}

#[derive(Clone)]
struct OpenLink;

impl crate::native::actions::Action for OpenLink {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        let Some(MediaSand::Link { url }) = world.get::<MediaSand>(owner) else {
            return;
        };
        let Ok(url) = web_url(url) else {
            return;
        };
        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("rundll32")
            .arg("url.dll,FileProtocolHandler")
            .arg(url.as_str())
            .spawn();
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(url.as_str()).spawn();
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let result = std::process::Command::new("xdg-open")
            .arg(url.as_str())
            .spawn();
        if let Err(error) = result {
            crate::native::notifications::report(world, "Link Sand", &error.to_string());
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedMedia {
    pub workspace: u64,
    sand: MediaSand,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::native::sand_placement::Placement,
    tokens: crate::native::tokens::TokenOverrides,
}

crate::native::sand_placement::scratch::saved!(SavedMedia);

impl SavedMedia {
    pub(crate) fn valid(&self) -> bool {
        self.sand.valid()
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }

    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let owner = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.sand,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::native::canvas::CanvasItem>(owner)
            .unwrap()
            .size = size;
        self.placement.restore(world, owner);
        world
            .entity_mut(owner)
            .insert((self.tokens, crate::native::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedMedia> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
            &crate::native::canvas::CanvasItem,
            &MediaSand,
        )>()
        .iter(world)
        .filter(|(entity, parent, ..)| {
            parent.parent() == root
                && world
                    .get::<crate::native::canvas_item::Ephemeral>(*entity)
                    .is_none()
        })
        .map(|(entity, _, member, item, sand)| SavedMedia {
            workspace: member.0,
            sand: sand.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::native::sand_placement::Placement::capture(world, entity),
            tokens: crate::native::token_style::overrides(world, entity),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referenced_images_verify_content_crop_and_reuse_retained_files() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("figure.png");
        image::RgbaImage::from_pixel(20, 10, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let library = lince_resources::Library::new(temporary.path().join("library"));
        let resource = lince_resources::Resource {
            uri: lince_resources::address(path.to_str().unwrap())
                .unwrap()
                .to_string(),
            media_type: "image/png".into(),
            hash: Some(lince_resources::fingerprint(&std::fs::read(&path).unwrap())),
            locator: lince_resources::Locator {
                region: Some([0.25, 0.2, 0.5, 0.5]),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            decode_resource(
                &library,
                &lince_resources::Resource {
                    hash: Some("0".repeat(64)),
                    ..resource.clone()
                }
            )
            .is_err()
        );
        let first = decode_resource(&library, &resource).unwrap();
        assert_eq!((first.width, first.height), (10, 5));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            decode_resource(&library, &resource).unwrap().rgba,
            first.rgba
        );
        let sand = MediaSand::ImageSource { resource };
        let restored: MediaSand =
            serde_json::from_value(serde_json::to_value(sand).unwrap()).unwrap();
        assert!(restored.valid());
    }
}
