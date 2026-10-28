use super::*;
use crate::presentation::accessibility::platform::{self as bus, Provider};
use bevy::text::EditableText;

const TEST: &str = "server::tests::transfers_tests::dispatch::magnification::platform::real_private_reader_protected_fields_and_queued_caret_requests_retire_on_composed_session_lock_and_fresh_input_recovers";
const FIELD: &str = "Protected-lifetime Unicode editor";
const SECRET: &str = "Protected-lifetime password editor";
const BODY: &str = "aé界 e\u{301} z";
const PRIVATE: &str = "fixture-only-protected-value";

mod mixed;

fn field(world: &mut World, root: Entity, workspace: u64, label: &'static str, body: &'static str, protected: bool) -> Entity {
    let owner = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText, label, bevy::math::DVec2::ZERO);
    let field = world.get::<native::sand_store::StoredSand>(owner).unwrap().content.unwrap();
    world.get_mut::<EditableText>(field).unwrap().editor.set_text(body);
    world.get_mut::<bevy::a11y::AccessibilityNode>(field).unwrap().set_label(label);
    if !protected {
        world.entity_mut(field).remove::<native::token_style::TextToken>().remove::<native::token_style::CursorToken>()
            .insert((TextColor(Color::srgb(0.0, 1.0, 0.0)), bevy::text::TextCursorStyle { color: Color::srgb(1.0, 0.0, 1.0), ..default() }));
    }
    if protected { native::accessibility::secret(world, field); }
    field
}

#[track_caller]
fn observe<T>(fixture: &mut Fixture, mut read: impl FnMut(&Fixture) -> bus::Result<Option<T>>) -> bus::Result<T> {
    let result = bus::wait(|| { fixture.tick(); read(fixture) });
    if result.is_err() && fixture.desktop.worker.is_running() {
        let nodes = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
                .map(|(entity, node)| (entity, node.role(), node.label().map(str::to_owned), node.children().len())).collect::<Vec<_>>()
        });
        eprintln!("Native nodes at reader observation failure: {nodes:?}");
    }
    result
}

fn selection(fixture: &mut Fixture, entity: Entity) -> usize {
    fixture.inspect(move |canvas| canvas.world_mut().get::<EditableText>(entity).unwrap().editor.raw_selection().focus().index())
}

fn text_pixels(fixture: &mut Fixture, provider: &Provider, object: &bus::Object, id: OutputId) -> bus::Result<()> {
    let reader = bus::proxy(&provider.bus, object, "org.a11y.atspi.Text")?;
    let (output, pixels) = fixture.output_pixels(id);
    let size = output.source_size().map(|value| value as usize);
    let scale = output.raster.scale[0];
    let origin = output.bounds.origin();
    let physical = |axis: usize, value: i32| (f64::from(value) - origin[axis]) * scale;
    for offset in [0_i32, 1, 2, 4, 7] {
        let (x, y, width, height): (i32, i32, i32, i32) = reader.call("GetCharacterExtents", &(offset, atspi_common::CoordType::Screen))?;
        assert!(width > 0 && height > 0, "A displayed Unicode character has no platform extent");
        let low = [(physical(0, x) - 1.0).max(0.0).floor() as usize, (physical(1, y) - 1.0).max(0.0).floor() as usize];
        let high = [((physical(0, x + width + 2) + 1.0).max(0.0).ceil() as usize).min(size[0]),
            ((physical(1, y + height + 2) + 1.0).max(0.0).ceil() as usize).min(size[1])];
        let ink = (low[1]..high[1]).flat_map(|y| (low[0]..high[0]).map(move |x| (x, y)))
            .filter(|(x, y)| {
                let pixel = (y * size[0] + x) * 4;
                pixels[pixel] < 80 && pixels[pixel + 1] > 180 && pixels[pixel + 2] < 80
            }).count();
        assert!(ink > 1, "The platform character rectangle does not contain independently rendered Unicode ink");
    }
    let (x, y, width, height): (i32, i32, i32, i32) = reader.call("GetCharacterExtents", &(2_i32, atspi_common::CoordType::Screen))?;
    let low = [physical(0, x - 4).max(0.0).floor() as usize, physical(1, y - 2).max(0.0).floor() as usize];
    let high = [(physical(0, x + width + 2).max(0.0).ceil() as usize).min(size[0]), (physical(1, y + height + 2).max(0.0).ceil() as usize).min(size[1])];
    let mut caret = Vec::new();
    for y in low[1]..high[1] {
        for x in low[0]..high[0] {
            let pixel = (y * size[0] + x) * 4;
            if pixels[pixel] > 180 && pixels[pixel + 1] < 80 && pixels[pixel + 2] > 180 { caret.push([x as f64 + 0.5, y as f64 + 0.5]); }
        }
    }
    assert!(caret.len() > 2, "The real focused caret was not rendered with its independent fixture color");
    let left = caret.iter().map(|point| point[0] - 0.5).fold(f64::INFINITY, f64::min);
    let mut rows = std::collections::BTreeMap::new();
    for point in &caret {
        let range = rows.entry(point[1] as usize).or_insert([point[0]; 2]);
        range[0] = range[0].min(point[0]);
        range[1] = range[1].max(point[0]);
    }
    let leading = rows.values().map(|range| (range[0] + range[1]) * 0.5).fold(f64::INFINITY, f64::min);
    let first = caret.iter().map(|point| point[1]).fold(f64::INFINITY, f64::min);
    let last = caret.iter().map(|point| point[1]).fold(f64::NEG_INFINITY, f64::max);
    let aligned = leading >= physical(0, x) - 1.0 && leading <= physical(0, x + 1) + 1.0;
    let contained = first >= physical(1, y) - 1.0 && last <= physical(1, y + height + 2) + 1.0;
    let complete = last - first >= (f64::from(height - 2) * scale - 2.0).max(1.0);
    if (!aligned || !contained || !complete)
        && let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS")
    {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        let name = format!("native-caret-{}-{}-{}", std::process::id(), id.0, fixture.desktop.current.as_ref().unwrap().sequence);
        std::fs::write(directory.join(&name).with_extension("rgba"), &pixels)?;
        std::fs::write(directory.join(&name).with_extension("json"), serde_json::to_vec(&serde_json::json!({
            "size": size, "output": id.0, "rotation": format!("{:?}", output.rotation), "scale": scale,
            "origin": origin, "character": [x, y, width, height], "sample_low": low, "sample_high": high,
            "caret_range": [left, first, last], "caret_leading_center": leading, "sequence": fixture.desktop.current.as_ref().unwrap().sequence,
            "expected": [physical(0, x), physical(0, x + 1), physical(1, y), physical(1, y + height + 2)],
            "editor": format!("{:?}", fixture.desktop.current.as_ref().unwrap().frame.input.editor),
        }))?)?;
        eprintln!("Native caret pixel evidence: {}", directory.join(&name).display());
    }
    assert!(aligned, "The displayed caret lies outside the platform coordinate's quantization interval");
    assert!(contained);
    assert!(complete, "Only a fragment of the expected caret is visible");
    Ok(())
}

fn child(root: std::path::PathBuf) -> bus::Result<()> {
    let provider = Provider::new(&root)?;
    provider.status.set_property("ScreenReaderEnabled", true)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?;
    let _entered = runtime.enter();
    let path = root.join("profile"); std::fs::create_dir(&path)?;
    let mut fixture = create(&runtime, &path, [1000, 720], 1.25);
    let ordinary = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspace = world.get::<native::workspace::Workspaces>(root).unwrap().active;
        native::workspace_config::set_physics(world, root, workspace, false);
        let ordinary = field(world, root, workspace, FIELD, BODY, false);
        field(world, root, workspace, SECRET, PRIVATE, true);
        ordinary
    });
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Tiled))).unwrap();
    settle(&mut fixture);
    let plain = observe(&mut fixture, |_| Ok(bus::named_role(&provider.bus, FIELD, Some(atspi_common::Role::Entry))?))?;
    let secret = observe(&mut fixture, |_| Ok(bus::named_role(&provider.bus, SECRET, Some(atspi_common::Role::PasswordText))?))?;
    let reader = bus::proxy(&provider.bus, &plain, "org.a11y.atspi.Text")?;
    assert_eq!(reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, BODY);
    assert_eq!(reader.get_property::<i32>("CharacterCount")?, BODY.chars().count() as i32);
    let children: Vec<bus::Object> = bus::proxy(&provider.bus, &secret, "org.a11y.atspi.Accessible")?.call("GetChildren", &())?;
    assert!(children.is_empty());
    if let Ok(value) = bus::proxy(&provider.bus, &secret, "org.a11y.atspi.Text")?.call::<_, _, String>("GetText", &(0_i32, -1_i32)) {
        assert!(!value.contains(PRIVATE), "A protected native field exposed its raw text through the private platform adapter");
    }
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(2_i32,))?);
    observe(&mut fixture, |_| Ok((reader.get_property::<i32>("CaretOffset")? == 2).then_some(())))?;
    let before = selection(&mut fixture, ordinary);
    assert!(before > 0);
    text_pixels(&mut fixture, &provider, &plain, OutputId(1))?;
    let (_, release) = fixture.pause();
    let mut queued = 0;
    loop {
        match fixture.desktop.worker.input(Input::Pointer([0.0; 2])) {
            Ok(_) => { queued += 1; assert!(queued <= 128); }
            Err(std::sync::mpsc::TrySendError::Full(_)) => break,
            Err(_) => panic!("The protected-input fixture worker disconnected before real queue pressure"),
        }
    }
    assert_eq!(queued, 128);
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(0_i32,))?);
    let qh = fixture.queue.handle();
    let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ());
    fixture.exchange();
    assert!(fixture.desktop.server.locked());
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ());
    fixture.exchange();
    let buffer = super::super::privacy::buffer(&fixture, [800, 576], [20, 20, 20, 255]);
    surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 800, 576); surface.commit();
    fixture.exchange(); fixture.desktop.dispatch().unwrap();
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
    observe(&mut fixture, |_| Ok((bus::named_role(&provider.bus, FIELD, None)?.is_none()
        && bus::named_role(&provider.bus, SECRET, None)?.is_none()).then_some(())))?;
    assert_eq!(selection(&mut fixture, ordinary), before, "A queued platform caret action crossed the real lock boundary");
    if let Ok(value) = reader.call::<_, _, String>("GetText", &(0_i32, -1_i32)) {
        assert!(!value.contains(BODY), "A retired platform text object retained readable locked content");
    }
    lock.unlock_and_destroy(); fixture.exchange();
    fixture.until(|fixture| !fixture.desktop.server.locked());
    role.destroy(); surface.destroy(); buffer.destroy(); fixture.exchange();
    let returned = observe(&mut fixture, |_| Ok(bus::named_role(&provider.bus, FIELD, Some(atspi_common::Role::Entry))?))?;
    let fresh = bus::proxy(&provider.bus, &returned, "org.a11y.atspi.Text")?;
    assert_eq!(fresh.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, BODY);
    assert!(fresh.call::<_, _, bool>("SetCaretOffset", &(0_i32,))?);
    observe(&mut fixture, |_| Ok((fresh.get_property::<i32>("CaretOffset")? == 0).then_some(())))?;
    assert_eq!(selection(&mut fixture, ordinary), 0);
    let _ = reader.call::<_, _, bool>("SetCaretOffset", &(2_i32,));
    settle(&mut fixture);
    assert_eq!(selection(&mut fixture, ordinary), 0, "A pre-lock platform object acquired the returned field");
    press(&mut fixture, 30);
    let expected = format!("a{BODY}");
    observe(&mut fixture, |_| Ok((fresh.call::<_, _, String>("GetText", &(0_i32, -1_i32))? == expected).then_some(())))?;
    assert!(fixture.desktop.running && fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
    fixture.desktop.worker.stop()?;
    bus::wait(|| Ok(bus::named_role(&provider.bus, "Lince Park", None)?.is_none().then_some(())))?;
    Ok(())
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn real_private_reader_protected_fields_and_queued_caret_requests_retire_on_composed_session_lock_and_fresh_input_recovers() -> bus::Result<()> {
    bus::run(TEST, child)
}
