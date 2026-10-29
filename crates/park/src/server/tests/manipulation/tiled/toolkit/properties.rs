use super::*;
use bevy::prelude::{
    ChildOf, ComputedNode, ComputedUiTargetCamera, Entity, Name, Rect, Text, UiGlobalTransform,
    Vec2,
};
use bevy::text::EditableText;
use lince_interface::native::{
    self,
    protein_area::{RecordBinding, Source},
};

const BODY: &str = "Original owner description";
const OTHER: &str = "Other owner description";
const TAG: &str = "park-toolkit-property";

#[path = "properties/unavailable.rs"]
mod unavailable;

#[path = "properties/creation.rs"]
mod creation;

#[derive(Debug)]
struct Field {
    entity: u64,
    value: String,
    point: Option<[f64; 2]>,
}

#[derive(Debug)]
struct State {
    area: u64,
    workspace: u64,
    live: bool,
    header: String,
    body: Option<Field>,
    assertions: Option<Field>,
}

fn project(
    world: &bevy::prelude::World,
    entity: Entity,
    geometry: &lince_interface::host::GeometrySnapshot,
) -> Option<[f64; 2]> {
    let camera = world.get::<ComputedUiTargetCamera>(entity)?.get()?;
    let owner = world
        .get::<native::topology::presentation::VisualOwner>(camera)?
        .0;
    let surface = world.get::<native::topology::presentation::Surface>(owner)?;
    let id = world.get::<native::layout::LayoutBox>(owner)?.id;
    let item = geometry
        .items
        .iter()
        .find(|item| item.id == ItemId::Native(id))?;
    if !surface.visible || surface.pixels.min_element() == 0 {
        return None;
    }
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let center = transform.transform_point2(Vec2::ZERO);
    let field = Rect::from_center_size(center, node.size());
    let mut clip = Rect::from_corners(Vec2::ZERO, surface.pixels.as_vec2());
    if let Some(bounds) = world.get::<bevy::ui::CalculatedClip>(entity) {
        clip = clip.intersect(bounds.clip);
    }
    let field = field.intersect(clip);
    if field.size().min_element() < 2.0 {
        return None;
    }
    for x in [0.5, 0.25, 0.75, 0.125, 0.875, 0.0625, 0.9375] {
        for y in [0.5, 0.25, 0.75, 0.125, 0.875, 0.0625, 0.9375] {
            let pixel = field.min + field.size() * Vec2::new(x, y);
            let source = std::array::from_fn(|axis| {
                item.mapping.surface.origin()[axis]
                    + f64::from(pixel[axis]) / f64::from(surface.pixels[axis])
                        * item.mapping.surface.size()[axis]
            });
            let Some(point) = item.mapping.canvas_point(source) else {
                continue;
            };
            if item.surface_point(point).is_some()
                && geometry
                    .outputs
                    .iter()
                    .any(|output| output.id == item.output && output.bounds.contains(point))
            {
                return Some(point);
            }
        }
    }
    None
}

fn property(world: &bevy::prelude::World, mut entity: Entity, uid: &str, label: &str) -> bool {
    let mut bound = false;
    let mut labelled = false;
    for _ in 0..64 {
        if let Some(binding) = world.get::<RecordBinding>(entity) {
            if binding.uid != uid {
                return false;
            }
            bound = true;
        }
        labelled |= world
            .get::<native::icons::Tooltip>(entity)
            .is_some_and(|tooltip| tooltip.0 == label);
        if bound && labelled {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn belongs(world: &bevy::prelude::World, mut entity: Entity, uid: &str) -> bool {
    for _ in 0..64 {
        if let Some(window) = world.get::<native::full_record::window::WindowRecord>(entity) {
            return window.uid == uid;
        }
        if let Some(binding) = world.get::<RecordBinding>(entity) {
            return binding.uid == uid;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn button(
    canvas: &mut crate::presentation::Presentation,
    uid: &str,
    caption: &str,
    geometry: &lince_interface::host::GeometrySnapshot,
) -> Option<(Entity, [f64; 2])> {
    let world = canvas.world_mut();
    let entity = world
        .query::<(
            Entity,
            &native::actions::ActionButton,
            Option<&bevy::prelude::Children>,
        )>()
        .iter(world)
        .find(|(entity, _, children)| {
            belongs(world, *entity, uid)
                && children.is_some_and(|children| {
                    children.iter().any(|child| {
                        world
                            .get::<Text>(*child)
                            .is_some_and(|text| text.0 == caption)
                    })
                })
        })
        .map(|(entity, _, _)| entity)?;
    Some((entity, project(world, entity, geometry)?))
}

fn snapshot(
    canvas: &mut crate::presentation::Presentation,
    uid: &str,
    geometry: &lince_interface::host::GeometrySnapshot,
) -> Option<State> {
    let world = canvas.world_mut();
    let (area, metadata) = world
        .query::<(Entity, &native::full_record::window::Metadata)>()
        .iter(world)
        .find(|(_, metadata)| metadata.uid == uid)
        .map(|(entity, metadata)| (entity, metadata.clone()))?;
    let workspace = world.get::<native::workspace::WorkspaceMember>(area)?.0;
    let header = world
        .query::<(&Name, &Text)>()
        .iter(world)
        .find(|(name, _)| name.as_str() == "window.properties.metadata")
        .map(|(_, text)| text.0.clone())
        .unwrap_or_default();
    let fields: Vec<_> = world
        .query::<(Entity, &EditableText)>()
        .iter(world)
        .map(|(entity, text)| (entity, text.value().to_string()))
        .collect();
    let find = |label| {
        fields
            .iter()
            .find(|(entity, _)| property(world, *entity, uid, label))
            .map(|(entity, value)| Field {
                entity: entity.to_bits(),
                value: value.clone(),
                point: project(world, *entity, geometry),
            })
    };
    Some(State {
        area: area.to_bits(),
        workspace,
        live: metadata.live,
        header,
        body: find("Description"),
        assertions: find("Assertions"),
    })
}

impl Toolkit {
    fn text(&mut self, text: &str) {
        let mut events = Vec::new();
        for character in text.chars() {
            let code = match character {
                'a' => 30,
                'b' => 48,
                'c' => 46,
                'd' => 32,
                'e' => 18,
                'f' => 33,
                'g' => 34,
                'h' => 35,
                'i' => 23,
                'j' => 36,
                'k' => 37,
                'l' => 38,
                'm' => 50,
                'n' => 49,
                'o' => 24,
                'p' => 25,
                'q' => 16,
                'r' => 19,
                's' => 31,
                't' => 20,
                'u' => 22,
                'v' => 47,
                'w' => 17,
                'x' => 45,
                'y' => 21,
                'z' => 44,
                '/' => 53,
                '-' => 12,
                _ => panic!("Unsupported fixture character: {character}"),
            };
            events.extend([(code, true), (code, false)]);
        }
        self.chord(&events);
    }

    fn properties_state(&mut self, uid: &str) -> Option<State> {
        let uid = uid.to_owned();
        self.inspect(move |canvas| {
            let geometry = canvas.published_geometry()?.clone();
            snapshot(canvas, &uid, &geometry)
        })
    }

    fn properties_ready(&mut self, uid: &str) -> State {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let state = self.properties_state(uid);
            if let Some(state) = state
                && state.body.is_some()
                && !state.header.is_empty()
            {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "The actual Properties inspector did not load: {uid}"
            );
            self.tick("properties-loading");
        }
    }

    fn click_property(&mut self, field: &Field) {
        let point = field.point.expect(
            "The Properties field must be visible without manually changing focus or placement",
        );
        assert!(
            self.fixture.viewport().bounds.contains(point),
            "The Properties field is outside the visible output: {field:?}"
        );
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("properties-hover");
        assert!(
            matches!(
                self.fixture.desktop.target(),
                crate::desktop::input::Target::Native
            ),
            "The Properties editor is hidden by an application at {point:?}: {field:?}"
        );
        let expected = field.entity;
        let (owner, hit, surfaces) = self.inspect(move |canvas| {
            let world = canvas.world_mut();
            let entity = world.query_filtered::<Entity, bevy::prelude::With<EditableText>>()
                .iter(world).find(|entity| entity.to_bits() == expected).unwrap();
            let camera = world.get::<ComputedUiTargetCamera>(entity).unwrap().get().unwrap();
            let owner = world.get::<native::topology::presentation::VisualOwner>(camera).unwrap().0;
            let hit = world.resource::<native::topology::input::PointerState>().hit.map(|(entity, _)| entity);
            let surfaces = [Some(owner), hit].into_iter().flatten().map(|entity| {
                let surface = world.get::<native::topology::presentation::Surface>(entity).unwrap();
                format!("{entity:?}: face={:?}, transform={:?}", surface.face, world.get::<bevy::prelude::GlobalTransform>(surface.face))
            }).collect::<Vec<_>>();
            (owner, hit, surfaces)
        });
        assert_eq!(hit, Some(owner), "The visible Properties field must receive the native content pointer before clicking at {point:?}: {surfaces:?}");
        self.button(272, true);
        self.button(272, false);
        let deadline = Instant::now() + Duration::from_secs(40);
        while !self
            .fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .editor
            .as_ref()
            .is_some_and(|editor| editor.owner == field.entity)
        {
            self.tick("properties-editor-focus");
            if Instant::now() >= deadline {
                let expected = field.entity;
                let details = self.inspect(move |canvas| {
                    use bevy::picking::{hover::HoverMap, pointer::{PointerId, PointerLocation}};
                    let world = canvas.world_mut();
                    let expected = world.query_filtered::<Entity, bevy::prelude::With<EditableText>>()
                        .iter(world).find(|entity| entity.to_bits() == expected).unwrap();
                    let pointers = world.query::<(&PointerId, &PointerLocation)>().iter(world)
                        .map(|(id, location)| format!("{id:?} {location:?}")).collect::<Vec<_>>();
                    format!("input={:?}; hit={:?}; hover={:?}; pointers={pointers:?}; expected={expected:?}, node={:?}, transform={:?}, camera={:?}",
                        world.get_resource::<bevy::input_focus::InputFocus>().and_then(bevy::input_focus::InputFocus::get),
                        world.get_resource::<native::topology::input::PointerState>().and_then(|state| state.hit),
                        world.get_resource::<HoverMap>(), world.get::<ComputedNode>(expected),
                        world.get::<UiGlobalTransform>(expected), world.get::<ComputedUiTargetCamera>(expected))
                });
                panic!(
                    "The clicked Properties editor never acquired its published identity: {field:?}; {details}"
                );
            }
        }
        assert_eq!(
            self.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .as_ref()
                .map(|editor| editor.owner),
            Some(field.entity)
        );
    }

    fn property_button(&mut self, uid: &str, caption: &str) {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let activated = Arc::new(AtomicUsize::new(0));
        let deadline = Instant::now() + Duration::from_secs(15);
        let (entity, point) = loop {
            let expected_uid = uid.to_owned();
            let expected_caption = caption.to_owned();
            let observed = activated.clone();
            if let Some(target) = self.inspect(move |canvas| {
                let geometry = canvas.published_geometry()?.clone();
                let (entity, point) =
                    button(canvas, &expected_uid, &expected_caption, &geometry)?;
                canvas.world_mut().entity_mut(entity).observe(
                    move |_: bevy::prelude::On<bevy::ui_widgets::Activate>| {
                        observed.fetch_add(1, Ordering::SeqCst);
                    },
                );
                Some((entity, point))
            }) {
                break target;
            }
            if Instant::now() >= deadline {
                let expected_uid = uid.to_owned();
                let details = self.inspect(move |canvas| {
                    let world = canvas.world_mut();
                    world
                        .query::<(Entity, Option<&Name>, Option<&Text>)>()
                        .iter(world)
                        .filter(|(entity, _, _)| belongs(world, *entity, &expected_uid))
                        .map(|(entity, name, text)| format!(
                            "{entity:?} name={name:?} text={text:?} node={:?} computed={:?} transform={:?} scroll={:?}",
                            world.get::<bevy::prelude::Node>(entity),
                            world.get::<ComputedNode>(entity),
                            world.get::<UiGlobalTransform>(entity),
                            world.get::<bevy::ui::ScrollPosition>(entity),
                        ))
                        .collect::<Vec<_>>()
                        .join("\n")
                });
                panic!("The normal Record control is not visible: {caption}; {details}");
            }
            self.tick("properties-control-presented");
        };
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("properties-control-hover");
        assert!(matches!(
            self.fixture.desktop.target(),
            crate::desktop::input::Target::Native
        ));
        self.button(272, true);
        self.button(272, false);
        assert_eq!(
            activated.load(Ordering::SeqCst),
            1,
            "The intended Record control did not activate exactly once: {caption}; {entity:?}"
        );
    }

    fn revealed_property(&mut self, field: &Field) {
        let editor = field.entity;
        let id = self.inspect(move |canvas| {
            let world = canvas.world_mut();
            let mut entity = Entity::from_bits(editor);
            for _ in 0..native::layout::MAX_DEPTH {
                if let Some(layout) = world.get::<native::layout::LayoutBox>(entity)
                    && world.get::<native::canvas::CanvasItem>(entity).is_some()
                {
                    return layout.id;
                }
                entity = world.get::<ChildOf>(entity).unwrap().parent();
            }
            panic!("The Properties editor has no canvas owner");
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let geometry = self.fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap();
            let item = geometry.items.iter().find(|item| item.id == ItemId::Native(id));
            if let Some(item) = item {
                let usable = geometry.outputs.iter().find(|output| output.id == item.output).unwrap().usable.unwrap();
                let rectangle = item.mapping.canvas;
                assert!((0..2).all(|axis| rectangle.size()[axis] <= usable.size()[axis] + 1.0),
                    "The reveal fixture must fit in its usable output: {rectangle:?}; {usable:?}");
                if (0..2).all(|axis| rectangle.origin()[axis] >= usable.origin()[axis] - 1.0
                    && rectangle.origin()[axis] + rectangle.size()[axis] <= usable.origin()[axis] + usable.size()[axis] + 1.0)
                {
                    return;
                }
            }
            assert!(Instant::now() < deadline, "The focused Properties card was not revealed: {item:?}");
            self.tick("properties-selected-reveal");
        }
    }

    fn editable_properties(&mut self, uid: &str) -> State {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let state = self.properties_ready(uid);
            if state
                .body
                .as_ref()
                .is_some_and(|field| field.point.is_some())
            {
                return state;
            }
            if Instant::now() >= deadline {
                let expected = state.body.as_ref().map(|field| field.entity);
                let details = self.inspect(move |canvas| {
                    let geometry = canvas.published_geometry().cloned();
                    let world = canvas.world_mut();
                    let mut entity = expected.map(Entity::from_bits);
                    let mut ancestors = Vec::new();
                    for _ in 0..native::layout::MAX_DEPTH {
                        let Some(current) = entity else { break };
                        ancestors.push(format!(
                            "{current:?} name={:?} node={:?} computed={:?} transform={:?} clip={:?} scroll={:?}",
                            world.get::<Name>(current),
                            world.get::<bevy::prelude::Node>(current),
                            world.get::<ComputedNode>(current),
                            world.get::<UiGlobalTransform>(current),
                            world.get::<bevy::ui::CalculatedClip>(current),
                            world.get::<bevy::ui::ScrollPosition>(current),
                        ));
                        entity = world.get::<ChildOf>(current).map(ChildOf::parent);
                    }
                    format!("geometry={geometry:?}; ancestors={}", ancestors.join("\n"))
                });
                panic!("The normal Raw description editor did not become visible: {state:?}; {details}");
            }
            self.tick("properties-editor-ready");
        }
    }

    fn application_record(&mut self, id: SurfaceId) -> Option<String> {
        self.inspect(move |canvas| {
            let world = canvas.world_mut();
            world
                .query::<(&native::layout::desktop::ApplicationWindow, &RecordBinding)>()
                .iter(world)
                .find(|(window, _)| window.surface == id)
                .map(|(_, binding)| binding.uid.clone())
        })
    }
}

fn records(
    runtime: &tokio::runtime::Runtime,
    provider: &native::app::CellHandle,
    first: &str,
    second: &str,
) -> Vec<Value> {
    let mut config = native::full_record::config(first, Source::Local);
    config.draft.query["where"] = serde_json::json!([{"any":[{"uid_eq":first},{"uid_eq":second}]}]);
    config.draft.query["limit"] = serde_json::json!(2);
    let replies = runtime.block_on(provider.0.local_session().handle(
        cell::ClientMessage::Subscribe {
            id: "verify-toolkit-properties".into(),
            protein: config.query().unwrap(),
        },
    ));
    let rows = replies
        .into_iter()
        .find_map(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            _ => None,
        })
        .unwrap();
    assert_eq!(rows.len(), 2);
    rows
}

fn exercise(spatial: bool, free: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut app = Toolkit::new(&runtime, directory.path(), spatial);
    app.wait("properties-map", |app| {
        app.window("Park toolkit parent").is_some() && !app.state().is_null()
    });
    let main = app.window("Park toolkit parent").unwrap().id;
    app.fixture
        .desktop
        .worker
        .input(Input::Activate(main))
        .unwrap();
    app.tick("properties-parent");
    app.configured(main);
    if free {
        app.chord(&[(125, true), (20, true), (20, false), (125, false)]);
        app.chord(&[(125, true), (14, true), (14, false), (125, false)]);
        app.configured(main);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let uid = loop {
        if let Some(uid) = app.application_record(main) {
            break uid;
        }
        assert!(Instant::now() < deadline);
    };
    let other = app.application_record(app.fixture.clients[0].1).unwrap();
    for (uid, body) in [(&uid, BODY), (&other, OTHER)] {
        runtime
            .block_on(app.fixture.cell.0.engine.act(
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some(body.into()),
                },
                None,
            ))
            .unwrap();
    }
    runtime
        .block_on(app.fixture.cell.0.engine.act(
            engine::actions::Action::CreateConcept {
                lingua: "g_local".into(),
                name: TAG.into(),
                parents: Vec::new(),
            },
            None,
        ))
        .unwrap();
    app.chord(&[(125, true), (25, true), (25, false), (125, false)]);
    app.wait("properties-operation", |app| {
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .editor
            .is_some()
    });
    app.text("/window/properties");
    app.type_key(28);
    let deadline = Instant::now() + Duration::from_secs(15);
    let state = loop {
        let state = app.properties_ready(&uid);
        if state
            .assertions
            .as_ref()
            .is_some_and(|field| field.point.is_some())
        {
            break state;
        }
        assert!(
            Instant::now() < deadline,
            "The Properties assertion field was never presented: {state:?}"
        );
        app.tick("properties-presented");
    };
    assert!(state.live);
    assert_eq!(state.workspace, 1);
    assert!(state.header.contains("Park toolkit parent"));
    assert!(
        state.assertions.is_some(),
        "The normal assertion editor must remain available"
    );
    assert_eq!(state.body.as_ref().unwrap().value, BODY);
    app.tick("properties-visible");
    app.click_property(state.assertions.as_ref().unwrap());
    if spatial && !free {
        app.revealed_property(state.assertions.as_ref().unwrap());
    }
    app.text(TAG);
    app.type_key(28);
    let completion = app.properties_ready(&uid).assertions.unwrap().value;
    if !completion.is_empty() {
        assert!(
            completion.contains(TAG) && completion.contains("^c_"),
            "Enter must either save or select the visible Concept completion: {completion}"
        );
        app.type_key(28);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let rows = records(&runtime, &app.fixture.cell, &uid, &other);
        let original = rows.iter().find(|row| row["uid"] == uid).unwrap();
        if original["assertions"].as_array().is_some_and(|assertions| {
            assertions
                .iter()
                .any(|assertion| assertion["predicate"] == TAG)
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The visible assertion editor did not save to the captured Record: {rows:?}"
        );
        app.tick("properties-assertion-save");
    }
    app.property_button(&uid, "Raw");
    let editable = app.editable_properties(&uid);
    let field = editable.body.as_ref().unwrap();
    app.click_property(field);
    app.type_key(107);
    app.type_key(45);
    let edited = app.properties_ready(&uid);
    assert_eq!(edited.area, state.area);
    assert_eq!(edited.body.as_ref().unwrap().entity, field.entity);
    assert_eq!(edited.body.as_ref().unwrap().value, format!("{BODY}x"));
    app.chord(&[
        (56, true),
        (15, true),
        (15, false),
        (15, true),
        (15, false),
        (56, false),
    ]);
    app.wait("properties-other-focus", |app| matches!(app.fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().focused,
        Some(ItemId::Application(id)) if id != main));
    let focused = app
        .fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .geometry
        .as_ref()
        .unwrap()
        .focused;
    assert!(
        matches!(focused, Some(ItemId::Application(id)) if id != main),
        "Navigation must focus a different application's Record: {focused:?}"
    );
    let retained = app.properties_ready(&uid);
    assert_eq!(retained.area, state.area);
    assert_eq!(retained.body.as_ref().unwrap().entity, field.entity);
    app.chord(&[(125, true), (3, true), (3, false), (125, false)]);
    let origin = state.workspace;
    app.wait("properties-other-workspace", |app| {
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .outputs
            .iter()
            .any(|output| output.workspace != origin)
    });
    assert_eq!(app.properties_ready(&uid).workspace, state.workspace);
    app.chord(&[(125, true), (2, true), (2, false), (125, false)]);
    app.wait("properties-return-workspace", |app| {
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .outputs
            .iter()
            .any(|output| output.workspace == origin)
    });
    app.chord(&[(56, true), (15, true), (15, false), (56, false)]);
    app.wait("properties-return-focus", |app| {
        matches!(
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .focused,
            Some(ItemId::Native(_))
        )
    });
    let restored = app.editable_properties(&uid);
    assert_eq!(restored.body.as_ref().unwrap().entity, field.entity);
    app.click_property(restored.body.as_ref().unwrap());
    app.type_key(107);
    app.allow_exit = true;
    for pressed in [true, false] {
        app.advance_time();
        app.fixture.desktop.key(44, pressed, app.time).unwrap();
    }
    app.fixture.desktop.server.close(main);
    app.tick("properties-source-close");
    app.wait("properties-unmapped", |app| {
        app.window("Park toolkit parent").is_none()
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let current = app.properties_ready(&uid);
        assert_eq!(current.area, state.area);
        assert_eq!(current.body.as_ref().unwrap().entity, field.entity);
        let rows = records(&runtime, &app.fixture.cell, &uid, &other);
        assert_eq!(
            rows.iter().find(|row| row["uid"] == other).unwrap()["body"],
            OTHER
        );
        assert!(
            !rows.iter().find(|row| row["uid"] == other).unwrap()["assertions"]
                .as_array()
                .is_some_and(|assertions| assertions
                    .iter()
                    .any(|assertion| assertion["predicate"] == TAG))
        );
        if !current.live
            && current.header.contains("closed or unavailable")
            && rows.iter().find(|row| row["uid"] == uid).unwrap()["body"] == format!("{BODY}xz")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The closed window's editor/save did not retain its Record: {current:?}"
        );
        app.tick("properties-closed-save");
    }
    app.tick("properties-closed-visible");
    app.fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn flat_qt_properties_keep_the_original_record_through_focus_and_close() {
    exercise(false, false);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn spatial_qt_properties_keep_the_original_record_through_focus_and_close() {
    exercise(true, false);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn flat_free_qt_properties_keep_the_original_record_through_focus_and_close() {
    exercise(false, true);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn spatial_free_qt_properties_keep_the_original_record_through_focus_and_close() {
    exercise(true, true);
}
