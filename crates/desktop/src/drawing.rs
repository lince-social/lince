mod raster;

use crate::actions::Action;
use bevy::{math::DVec2, prelude::*, text::EditableText, ui::RelativeCursorPosition};
use nucleus::{
    description_asset::Kind,
    drawing::{Drawing, Stroke},
};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct NativeDrawing(pub Drawing);

#[derive(Component)]
pub(crate) struct DrawingSurface;

#[derive(Component)]
struct View {
    surface: Entity,
    status: Entity,
    editable: bool,
    active: bool,
    dirty: bool,
    color: [u8; 4],
    width: f32,
    redo: Vec<Stroke>,
    target: Option<Entity>,
    replace: Option<String>,
    destinations: Option<Entity>,
    saving: bool,
    last_paint: std::time::Instant,
    painted: Option<(Drawing, image::RgbaImage)>,
    pointer: Option<Vec2>,
}

#[derive(Resource, Default)]
struct Copied(Option<Drawing>);

#[derive(Clone, Copy)]
enum Control {
    Undo,
    Redo,
    Clear,
    Color([u8; 4]),
    Width(f32),
    CopyNative,
    PasteNative,
    CopyCanvas,
    CopyImage,
    Export(Kind),
    Insert(Kind),
    Destinations,
    Destination(Entity),
    Cancel,
}

fn status(world: &mut World, owner: Entity, value: &str) {
    if let Some(entity) = world.get::<View>(owner).map(|view| view.status)
        && let Some(mut text) = world.get_mut::<Text>(entity)
    {
        text.0 = value.into();
    }
}

pub(crate) fn inserted(world: &mut World, owner: Entity, result: Result<(), String>) {
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    view.saving = false;
    match result {
        Ok(()) => status(
            world,
            owner,
            "Inserted into the description. Record text contains an asset reference.",
        ),
        Err(error) => status(world, owner, &error),
    }
}

pub(crate) fn saved_reference(world: &mut World, owner: Entity, reference: String) {
    if let Some(mut view) = world.get_mut::<View>(owner) {
        view.replace = Some(reference);
    }
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world.get::<crate::workspace::Workspaces>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

impl Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Cancel) {
            world.despawn(owner);
            return;
        }
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        if view.saving {
            status(world, owner, "Wait for the asset to finish saving.");
            return;
        }
        let drawing = world.get::<NativeDrawing>(owner).unwrap().0.clone();
        match *self {
            Self::Color(color) if view.editable => {
                world.get_mut::<View>(owner).unwrap().color = color
            }
            Self::Width(width) if view.editable => {
                world.get_mut::<View>(owner).unwrap().width = width
            }
            Self::Undo if view.editable => {
                if let Some(stroke) = world
                    .get_mut::<NativeDrawing>(owner)
                    .unwrap()
                    .0
                    .strokes
                    .pop()
                {
                    world.get_mut::<View>(owner).unwrap().redo.push(stroke);
                    world.get_mut::<View>(owner).unwrap().dirty = true;
                }
            }
            Self::Redo if view.editable => {
                if let Some(stroke) = world.get_mut::<View>(owner).unwrap().redo.pop() {
                    world
                        .get_mut::<NativeDrawing>(owner)
                        .unwrap()
                        .0
                        .strokes
                        .push(stroke);
                    world.get_mut::<View>(owner).unwrap().dirty = true;
                }
            }
            Self::Clear if view.editable => {
                world
                    .get_mut::<NativeDrawing>(owner)
                    .unwrap()
                    .0
                    .strokes
                    .clear();
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.redo.clear();
                view.dirty = true;
            }
            Self::CopyNative => {
                world.init_resource::<Copied>();
                world.resource_mut::<Copied>().0 = Some(drawing.clone());
                let serialized =
                    serde_json::to_string(&drawing).map(|json| format!("lince-drawing:{json}"));
                if let Ok(text) = serialized
                    && let Some(mut clipboard) =
                        world.get_resource_mut::<bevy::clipboard::Clipboard>()
                {
                    let _ = clipboard.set_text(text);
                }
                status(
                    world,
                    owner,
                    "Native strokes copied. Use Paste native in another Drawing Sand.",
                );
            }
            Self::PasteNative if view.editable => {
                let copied = world
                    .get_resource::<Copied>()
                    .and_then(|copied| copied.0.clone());
                if let Some(copied) = copied {
                    world.get_mut::<NativeDrawing>(owner).unwrap().0 = copied;
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.redo.clear();
                    view.dirty = true;
                } else {
                    status(world, owner, "Copy native from a drawing first.");
                }
            }
            Self::CopyCanvas => {
                if let Some(root) = root(world, owner) {
                    let workspace = world
                        .get::<crate::workspace::Workspaces>(root)
                        .unwrap()
                        .active;
                    let position = world
                        .get::<crate::canvas::CanvasView>(root)
                        .map_or(DVec2::ZERO, |view| view.center);
                    spawn(world, root, workspace, position, drawing);
                    status(world, owner, "Copied editable strokes to the canvas.");
                }
            }
            Self::CopyImage => {
                let result = raster::pixels(&drawing).and_then(|pixels| {
                    let image = bevy_image(pixels);
                    world
                        .get_resource_mut::<bevy::clipboard::Clipboard>()
                        .ok_or("Clipboard unavailable.".to_string())?
                        .set_image(&image)
                        .map_err(|error| error.to_string())
                });
                status(
                    world,
                    owner,
                    result
                        .as_ref()
                        .err()
                        .map_or("Image copied.", String::as_str),
                );
            }
            Self::Export(kind) => export(world, owner, drawing, kind),
            Self::Insert(kind) => {
                let Some(input) = view.target else {
                    destinations(world, owner);
                    return;
                };
                let replace = view.replace.clone();
                encode_for_insertion(world, owner, input, drawing, kind, replace);
            }
            Self::Destinations => destinations(world, owner),
            Self::Destination(input) => {
                world.get_mut::<View>(owner).unwrap().target = Some(input);
                if let Some(panel) = world.get_mut::<View>(owner).unwrap().destinations.take() {
                    world.despawn(panel);
                }
                status(
                    world,
                    owner,
                    "Description selected. Choose Insert native, PNG or WebP.",
                );
            }
            Self::Cancel => {
                world.despawn(owner);
            }
            _ => {}
        }
    }
}

fn destinations(world: &mut World, owner: Entity) {
    if let Some(panel) = world.get_mut::<View>(owner).unwrap().destinations.take() {
        world.despawn(panel);
    }
    let inputs = world
        .query::<(Entity, &crate::record_binding::TextBinding, &EditableText)>()
        .iter(world)
        .filter(|(entity, binding, _)| {
            binding.property == "body"
                && crate::record_binding::can_edit(world, *entity)
                && root(world, *entity) == root(world, owner)
        })
        .map(|(entity, binding, _)| (entity, binding.record.uid.clone()))
        .collect::<Vec<_>>();
    let panel = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.get_mut::<View>(owner).unwrap().destinations = Some(panel);
    if inputs.is_empty() {
        crate::edit_mode::label(
            world,
            panel,
            "Open an editable Record description first.",
            14.0,
        );
    }
    for (input, uid) in inputs {
        crate::description::button(
            world,
            panel,
            owner,
            &format!("Use {uid}"),
            Control::Destination(input),
        );
    }
}

fn bevy_image(pixels: image::RgbaImage) -> Image {
    Image::new(
        bevy::render::render_resource::Extent3d {
            width: pixels.width(),
            height: pixels.height(),
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        pixels.into_raw(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    )
}

fn content(
    world: &mut World,
    owner: Entity,
    drawing: Drawing,
    editable: bool,
    target: Option<Entity>,
) {
    world.entity_mut(owner).insert((
        NativeDrawing(drawing.clone()),
        crate::description::Rendered,
        crate::sand_store::SandCredits(crate::description::CREDITS),
    ));
    let controls = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    if editable {
        for (title, action) in [
            ("Undo", Control::Undo),
            ("Redo", Control::Redo),
            ("Clear", Control::Clear),
            ("Black", Control::Color([24, 24, 24, 255])),
            ("Red", Control::Color([200, 35, 55, 255])),
            ("Blue", Control::Color([30, 100, 210, 255])),
            ("Thin", Control::Width(2.0)),
            ("Medium", Control::Width(5.0)),
            ("Thick", Control::Width(12.0)),
            ("Paste native", Control::PasteNative),
        ] {
            crate::description::button(world, controls, owner, title, action);
        }
    }
    let surface = world
        .spawn((
            Node {
                width: percent(100),
                aspect_ratio: Some(drawing.width as f32 / drawing.height as f32),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::WHITE),
            RelativeCursorPosition::default(),
            DrawingSurface,
            ChildOf(owner),
        ))
        .id();
    let outputs = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    for (title, action) in [
        ("Copy native", Control::CopyNative),
        ("Copy to canvas", Control::CopyCanvas),
        ("Copy image", Control::CopyImage),
        ("Export PNG", Control::Export(Kind::Png)),
        ("Export WebP", Control::Export(Kind::Webp)),
    ] {
        crate::description::button(world, outputs, owner, title, action);
    }
    if editable {
        if target.is_none() {
            crate::description::button(
                world,
                outputs,
                owner,
                "Choose description…",
                Control::Destinations,
            );
        }
        for (title, kind) in [
            ("Insert native", Kind::Drawing),
            ("Insert PNG", Kind::Png),
            ("Insert WebP", Kind::Webp),
        ] {
            crate::description::button(world, outputs, owner, title, Control::Insert(kind));
        }
        if target.is_some() {
            crate::description::button(world, outputs, owner, "Close drawing", Control::Cancel);
        }
    }
    let status = crate::edit_mode::label(
        world,
        owner,
        if editable {
            "Draw on the white surface. Native copies keep editable strokes."
        } else {
            "Read-only drawing. Copy it to the canvas to edit a separate drawing."
        },
        13.0,
    );
    world.entity_mut(owner).insert(View {
        surface,
        status,
        editable,
        active: false,
        dirty: true,
        color: [24, 24, 24, 255],
        width: 5.0,
        redo: Vec::new(),
        target,
        replace: None,
        destinations: None,
        saving: false,
        painted: None,
        pointer: None,
        last_paint: std::time::Instant::now() - std::time::Duration::from_secs(1),
    });
    world.entity_mut(surface).observe(
        move |mut event: On<Pointer<Press>>,
              mut views: Query<&mut View>,
              geometry: Query<(&ComputedNode, &UiGlobalTransform)>| {
            event.propagate(false);
            if event.button == PointerButton::Primary
                && let Ok(mut view) = views.get_mut(owner)
                && view.editable
                && !view.saving
            {
                view.active = true;
                if let Ok((node, transform)) = geometry.get(event.entity) {
                    view.pointer = node.normalize_point(
                        *transform,
                        event.pointer_location.position / node.inverse_scale_factor(),
                    );
                }
            }
        },
    );
    world.entity_mut(surface).observe(
        move |mut event: On<Pointer<Move>>,
              mut views: Query<&mut View>,
              geometry: Query<(&ComputedNode, &UiGlobalTransform)>| {
            event.propagate(false);
            if let Ok(mut view) = views.get_mut(owner)
                && let Ok((node, transform)) = geometry.get(event.entity)
            {
                view.pointer = node.normalize_point(
                    *transform,
                    event.pointer_location.position / node.inverse_scale_factor(),
                );
            }
        },
    );
    world.entity_mut(surface).observe(
        move |mut event: On<Pointer<Out>>, mut views: Query<&mut View>| {
            event.propagate(false);
            if let Ok(mut view) = views.get_mut(owner) {
                view.pointer = None;
            }
        },
    );
    world.entity_mut(surface).observe(
        move |mut event: On<Pointer<Release>>, mut views: Query<&mut View>| {
            event.propagate(false);
            if let Ok(mut view) = views.get_mut(owner) {
                view.active = false;
            }
        },
    );
    world
        .entity_mut(surface)
        .observe(|mut event: On<Pointer<Drag>>| {
            event.propagate(false);
        });
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    drawing: Drawing,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            crate::workspace::WorkspaceMember(workspace),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(680.0, 610.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    crate::edit_mode::label(world, owner, "Drawing Sand", 22.0);
    content(world, owner, drawing, true, None);
    owner
}

pub(crate) fn embedded(world: &mut World, parent: Entity, drawing: Drawing) {
    let owner = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    content(world, owner, drawing, false, None);
}

#[derive(Clone)]
pub(crate) struct EditDescriptionDrawing {
    pub input: Entity,
    pub reference: String,
}

impl Action for EditDescriptionDrawing {
    fn apply(&self, world: &mut World, embedded: Entity) {
        if !crate::record_binding::can_edit(world, self.input) {
            return;
        }
        let Some(drawing) = world
            .get::<NativeDrawing>(embedded)
            .map(|drawing| drawing.0.clone())
        else {
            return;
        };
        let Some(parent) = world.get::<ChildOf>(self.input).map(ChildOf::parent) else {
            return;
        };
        let owner = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        content(world, owner, drawing, true, Some(self.input));
        world.get_mut::<View>(owner).unwrap().replace = Some(self.reference.clone());
        status(
            world,
            owner,
            "Saving replaces this drawing's image references in this description. Close drawing to cancel.",
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DrawDescription(pub Entity);

impl Action for DrawDescription {
    fn apply(&self, world: &mut World, parent: Entity) {
        if !crate::record_binding::can_edit(world, self.0) {
            return;
        }
        let owner = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        content(world, owner, Drawing::default(), true, Some(self.0));
    }
}

#[derive(Clone, Copy)]
struct Add;

impl Action for Add {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(workspace) = world
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active)
        else {
            return;
        };
        let position = world
            .get::<crate::canvas::CanvasView>(root)
            .map_or(DVec2::ZERO, |view| view.center);
        spawn(world, root, workspace, position, Drawing::default());
        crate::edit_mode::EditAction::Close.apply(world, root);
    }
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    let entry = crate::description::button(
        world,
        parent,
        root,
        "Drawing Sand · Draw, copy and export",
        Add,
    );
    world
        .entity_mut(entry)
        .insert(crate::sand_store::StoreComponent {
            title: "Drawing Sand".into(),
            description: "Draw, copy and export".into(),
            size: Vec2::new(680.0, 610.0),
        });
}

fn update(world: &mut World) {
    let owners = world
        .query::<(Entity, &View)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    let pressed = world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|input| input.pressed(MouseButton::Left));
    for owner in owners {
        let view = world.get::<View>(owner).unwrap();
        let surface = view.surface;
        let pointer = view.pointer.or_else(|| {
            world
                .get::<RelativeCursorPosition>(surface)
                .filter(|cursor| cursor.cursor_over())
                .and_then(|cursor| cursor.normalized)
        });
        if view.active
            && pressed
            && let Some(pointer) = pointer
        {
            let (color, width) = (view.color, view.width);
            let point = pointer + Vec2::splat(0.5);
            let mut drawing = world.get::<NativeDrawing>(owner).unwrap().0.clone();
            let point = [
                point.x.clamp(0.0, 1.0) * drawing.width as f32,
                point.y.clamp(0.0, 1.0) * drawing.height as f32,
            ];
            let new_stroke = world.get::<ActiveStroke>(owner).is_none();
            if new_stroke {
                drawing.strokes.push(Stroke {
                    points: vec![point],
                    color,
                    width,
                });
            } else if let Some(stroke) = drawing.strokes.last_mut() {
                if stroke
                    .points
                    .last()
                    .is_none_or(|last| (last[0] - point[0]).hypot(last[1] - point[1]) >= 0.75)
                {
                    stroke.points.push(point);
                }
            }
            if drawing != world.get::<NativeDrawing>(owner).unwrap().0 && drawing.validate().is_ok()
            {
                world.get_mut::<NativeDrawing>(owner).unwrap().0 = drawing;
                world.entity_mut(owner).insert(ActiveStroke);
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.dirty = true;
                view.redo.clear();
            } else if drawing.validate().is_err() {
                world.get_mut::<View>(owner).unwrap().active = false;
                status(
                    world,
                    owner,
                    "Drawing limit reached. Undo a stroke or start another drawing.",
                );
            }
        }
        if !pressed || !world.get::<View>(owner).unwrap().active {
            world.entity_mut(owner).remove::<ActiveStroke>();
            world.get_mut::<View>(owner).unwrap().active = false;
        }
        let view = world.get::<View>(owner).unwrap();
        if view.dirty
            && view.last_paint.elapsed().as_millis() < 40
            && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>()
        {
            wake.after(
                std::time::Duration::from_millis(40)
                    - view
                        .last_paint
                        .elapsed()
                        .min(std::time::Duration::from_millis(40)),
            );
        }
        if view.dirty && view.last_paint.elapsed().as_millis() >= 40 {
            let drawing = world.get::<NativeDrawing>(owner).unwrap().0.clone();
            let painted = world.get_mut::<View>(owner).unwrap().painted.take();
            let pixels = match painted {
                Some((previous, mut pixels)) => {
                    raster::update(&previous, &drawing, &mut pixels).map(|()| pixels)
                }
                None => raster::pixels(&drawing),
            };
            if let Ok(pixels) = pixels {
                world.init_resource::<Assets<Image>>();
                let handle = world
                    .get::<ImageNode>(surface)
                    .map(|node| node.image.clone());
                let image = bevy_image(pixels.clone());
                let handle = if let Some(handle) = handle
                    && let Some(mut target) = world.resource_mut::<Assets<Image>>().get_mut(&handle)
                {
                    *target = image;
                    handle
                } else {
                    world.resource_mut::<Assets<Image>>().add(image)
                };
                world.entity_mut(surface).insert(ImageNode::new(handle));
                world.get_mut::<Node>(surface).unwrap().aspect_ratio =
                    Some(drawing.width as f32 / drawing.height as f32);
                world.get_mut::<View>(owner).unwrap().painted = Some((drawing, pixels));
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.dirty = false;
            view.last_paint = std::time::Instant::now();
        }
    }
}

#[derive(Component)]
struct ActiveStroke;

type ExportResult = Result<bool, String>;

#[derive(Component)]
struct Exporting(std::sync::Mutex<std::sync::mpsc::Receiver<ExportResult>>);

fn export(world: &mut World, owner: Entity, drawing: Drawing, kind: Kind) {
    if world.get::<Exporting>(owner).is_some() {
        status(world, owner, "An export is already open.");
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    world
        .entity_mut(owner)
        .insert(Exporting(std::sync::Mutex::new(receiver)));
    status(world, owner, "Choose where to export the image…");
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    std::thread::spawn(move || {
        let result = (|| {
            let extension = kind.name();
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Drawing image", &[extension])
                .set_file_name(format!("drawing.{extension}"))
                .save_file()
            else {
                return Ok(false);
            };
            let bytes = raster::encode(&drawing, kind)?;
            std::fs::write(path, bytes).map_err(|error| error.to_string())?;
            Ok(true)
        })();
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
}

fn exports(world: &mut World) {
    let results = world
        .query::<(Entity, &Exporting)>()
        .iter(world)
        .filter_map(|(entity, export)| {
            export
                .0
                .lock()
                .ok()?
                .try_recv()
                .ok()
                .map(|result| (entity, result))
        })
        .collect::<Vec<_>>();
    for (entity, result) in results {
        world.entity_mut(entity).remove::<Exporting>();
        status(
            world,
            entity,
            match &result {
                Ok(true) => "Image exported.",
                Ok(false) => "Export cancelled.",
                Err(error) => error,
            },
        );
    }
}

#[derive(Component)]
struct Encoding {
    input: Entity,
    kind: Kind,
    replace: Option<String>,
    result: std::sync::Mutex<std::sync::mpsc::Receiver<Result<Vec<u8>, String>>>,
}

fn encode_for_insertion(
    world: &mut World,
    owner: Entity,
    input: Entity,
    drawing: Drawing,
    kind: Kind,
    replace: Option<String>,
) {
    if !crate::record_binding::can_edit(world, input) {
        status(world, owner, "Description is read only or disconnected.");
        return;
    }
    if world.query::<&Encoding>().iter(world).count() >= 4 {
        status(
            world,
            owner,
            "Four drawings are being saved. Try again shortly.",
        );
        return;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    world.entity_mut(owner).insert(Encoding {
        input,
        kind,
        replace,
        result: std::sync::Mutex::new(receiver),
    });
    world.get_mut::<View>(owner).unwrap().saving = true;
    status(world, owner, "Preparing drawing asset…");
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    std::thread::spawn(move || {
        let _ = sender.send(raster::encode(&drawing, kind));
        if let Some(wake) = wake {
            wake.ring();
        }
    });
}

fn encoded(world: &mut World) {
    let results = world
        .query::<(Entity, &Encoding)>()
        .iter(world)
        .filter_map(|(entity, encoding)| {
            encoding.result.lock().ok()?.try_recv().ok().map(|result| {
                (
                    entity,
                    encoding.input,
                    encoding.kind,
                    encoding.replace.clone(),
                    result,
                )
            })
        })
        .collect::<Vec<_>>();
    for (owner, input, kind, replace, bytes) in results {
        world.entity_mut(owner).remove::<Encoding>();
        let result = bytes.and_then(|bytes| {
            crate::description::insert_asset(world, owner, input, kind, bytes, replace)
        });
        if let Err(error) = result {
            inserted(world, owner, Err(error));
        } else {
            status(world, owner, "Saving drawing asset…");
        }
    }
}

pub struct DrawingPlugin;
impl Plugin for DrawingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Copied>()
            .add_systems(Update, (update, exports, encoded));
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedDrawing {
    pub workspace: u64,
    drawing: Drawing,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedDrawing {
    pub(crate) fn valid(&self) -> bool {
        self.drawing.validate().is_ok()
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
            self.drawing,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::canvas::CanvasItem>(owner)
            .unwrap()
            .size = size;
        self.placement.restore(world, owner);
        world
            .entity_mut(owner)
            .insert((self.tokens, crate::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedDrawing> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
            &crate::canvas::CanvasItem,
            &NativeDrawing,
        )>()
        .iter(world)
        .filter(|(_, parent, ..)| parent.parent() == root)
        .map(|(entity, _, workspace, item, drawing)| SavedDrawing {
            workspace: workspace.0,
            drawing: drawing.0.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        let root = world
            .spawn((
                Node::default(),
                crate::workspace::Workspaces::default(),
                crate::canvas::CanvasView::default(),
            ))
            .id();
        (world, root)
    }

    #[test]
    fn native_copies_undo_redo_and_persistence_preserve_editable_strokes() {
        let (mut world, root) = world();
        let drawing = Drawing {
            strokes: vec![Stroke {
                points: vec![[4.0, 5.0], [45.0, 30.0]],
                color: [80, 130, 240, 255],
                width: 5.0,
            }],
            ..Drawing::default()
        };
        let owner = spawn(&mut world, root, 1, DVec2::new(40.0, 90.0), drawing.clone());
        Control::Undo.apply(&mut world, owner);
        assert!(
            world
                .get::<NativeDrawing>(owner)
                .unwrap()
                .0
                .strokes
                .is_empty()
        );
        Control::Redo.apply(&mut world, owner);
        assert_eq!(world.get::<NativeDrawing>(owner).unwrap().0, drawing);
        Control::CopyNative.apply(&mut world, owner);
        let copy = spawn(&mut world, root, 1, DVec2::ZERO, Drawing::default());
        Control::PasteNative.apply(&mut world, copy);
        assert_eq!(world.get::<NativeDrawing>(copy).unwrap().0, drawing);
        let saved = snapshot(&mut world, root);
        assert_eq!(saved.len(), 2);
        let bytes = serde_json::to_vec(&saved).unwrap();
        let restored: Vec<SavedDrawing> = serde_json::from_slice(&bytes).unwrap();
        assert!(restored.iter().all(SavedDrawing::valid));
        world.despawn(owner);
        world.despawn(copy);
        for saved in restored {
            saved.restore(&mut world, root);
        }
        assert!(
            world
                .query::<&NativeDrawing>()
                .iter(&world)
                .all(|native| native.0 == drawing)
        );
        assert_eq!(
            world
                .query::<&crate::record_view::RecordEditor>()
                .iter(&world)
                .count(),
            0
        );
    }

    #[test]
    fn embedded_drawing_does_not_mutate_when_undo_or_clear_is_requested() {
        let (mut world, root) = world();
        let drawing = Drawing {
            strokes: vec![Stroke {
                points: vec![[4.0, 5.0]],
                color: [0, 0, 0, 255],
                width: 5.0,
            }],
            ..Drawing::default()
        };
        embedded(&mut world, root, drawing.clone());
        let owner = world
            .query_filtered::<Entity, With<NativeDrawing>>()
            .single(&world)
            .unwrap();
        Control::Undo.apply(&mut world, owner);
        Control::Clear.apply(&mut world, owner);
        assert_eq!(world.get::<NativeDrawing>(owner).unwrap().0, drawing);
        assert!(snapshot(&mut world, root).is_empty());
    }

    #[test]
    fn pointer_coordinates_create_native_strokes_and_release_starts_a_separate_stroke() {
        let (mut world, root) = world();
        world.init_resource::<ButtonInput<MouseButton>>();
        let owner = spawn(&mut world, root, 1, DVec2::ZERO, Drawing::default());
        let surface = world.get::<View>(owner).unwrap().surface;
        world.get_mut::<ComputedNode>(surface).unwrap().size = Vec2::splat(100.0);
        world.entity_mut(surface).insert(UiGlobalTransform::from(
            bevy::math::Affine2::from_translation(Vec2::splat(50.0)),
        ));
        world.flush();
        let location = bevy::picking::pointer::Location {
            target: bevy::camera::NormalizedRenderTarget::Image(bevy::camera::ImageRenderTarget {
                handle: Handle::default(),
                scale_factor: 1.0,
            }),
            position: Vec2::new(10.0, 20.0),
        };
        world.trigger(Pointer::new(
            crate::topology::input::CONTENT_POINTER,
            location.clone(),
            Press {
                button: PointerButton::Primary,
                hit: bevy::picking::backend::HitData::new(root, 0.0, None, None),
                count: 1,
            },
            surface,
        ));
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        update(&mut world);
        world.trigger(Pointer::new(
            crate::topology::input::CONTENT_POINTER,
            bevy::picking::pointer::Location {
                position: Vec2::splat(50.0),
                ..location
            },
            Move {
                hit: bevy::picking::backend::HitData::new(root, 0.0, None, None),
                delta: Vec2::new(40.0, 30.0),
            },
            surface,
        ));
        update(&mut world);
        assert!(world.get::<ImageNode>(surface).is_some());
        assert_eq!(
            world.get::<NativeDrawing>(owner).unwrap().0.strokes.len(),
            1
        );
        assert_eq!(
            world.get::<NativeDrawing>(owner).unwrap().0.strokes[0]
                .points
                .len(),
            2
        );
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        update(&mut world);
        world.get_mut::<View>(owner).unwrap().active = true;
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        update(&mut world);
        let drawing = &world.get::<NativeDrawing>(owner).unwrap().0;
        assert_eq!(drawing.strokes.len(), 2);
        assert!((drawing.strokes[0].points[0][0] - 64.0).abs() < 0.01);
        assert!((drawing.strokes[0].points[0][1] - 80.0).abs() < 0.01);
    }
}
