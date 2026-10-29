use super::*;
use lince_interface::host::{magnification::{Outcome, Snapshot}, Rectangle};
use smithay::backend::renderer::{ExportMem, TextureMapping};

const FACTOR: &str = "Display magnification factor (1–4)";
const OUTPUT: &str = "Magnification-fixture";

pub(super) mod clients;
mod input;
mod drop;
mod caret;
#[cfg(feature = "accessibility-gate")]
mod platform;

pub(super) fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64) -> Fixture {
    create_authorized(runtime, directory, size, scale, Vec::new())
}

fn create_authorized(runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], scale: f64,
    capabilities: Vec<crate::launch::Capability>) -> Fixture
{
    let server = Server::new(size.map(|value| value as i32)).unwrap();
    let mut harness = if capabilities.is_empty() { Harness::connect(server) } else { Harness::connect_authorized(server, Some(capabilities)) };
    harness.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [0, 0], size: size.map(|value| value as i32) }],
        &std::collections::BTreeMap::from([(OutputId(1), scale)])).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let mut fixture = Fixture::with_size(harness, runtime, directory, size);
    fixture.desktop.commands(directory.join("park.json"), directory.join("wayland-magnification-fixture"));
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.control.context.is_some())
        && fixture.desktop.magnification_snapshot().head(OutputId(1)).is_some_and(|head| head.effective == Some(head.requested)));
    fixture
}

fn press(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 10).unwrap();
    fixture.desktop.key(code, false, 11).unwrap();
}

fn settle(fixture: &mut Fixture) { fixture.inspect(|_| ()); }

fn tap(fixture: &mut Fixture, code: u32) { press(fixture, code); settle(fixture); }

fn entity(fixture: &mut Fixture, caption: &str) -> Entity {
    let caption = caption.to_owned();
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world)
            .find(|(entity, node)| node.label() == Some(caption.as_str())
                && world.get::<InheritedVisibility>(*entity).is_some_and(|visible| visible.get())
                && (world.get::<native::actions::ActionButton>(*entity).is_some() || world.get::<bevy::text::EditableText>(*entity).is_some()))
            .map(|(entity, _)| entity).unwrap_or_else(|| panic!("The visible control {caption} is absent"))
    })
}

pub(super) fn focus(fixture: &mut Fixture, caption: &str) -> Entity {
    let entity = entity(fixture, caption);
    focus_entity(fixture, entity, caption)
}

pub(super) fn focus_entity(fixture: &mut Fixture, entity: Entity, caption: &str) -> Entity {
    eprintln!("Tabbing to control: {caption} ({entity:?})");
    let deadline = Instant::now() + Duration::from_secs(120);
    for _ in 0..256 {
        if fixture.inspect(move |canvas| canvas.world_mut().resource::<bevy::input_focus::InputFocus>().get()) == Some(entity) {
            let (output, points) = fixture.inspect(move |canvas| canvas.native_test_points(entity))
                .expect("Keyboard focus must expose its control through the actual presented view");
            let low = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
            let high: [f64; 2] = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
            let bounds = Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis])).expect("The focused control has no projected extent");
            let current = fixture.desktop.current.as_ref().unwrap();
            let mapping = current.frame.input.mappings.get(output).unwrap();
            let visible = mapping.visible_rectangle(bounds).expect("Keyboard focus must reveal the control through the actual lens");
            assert!(visible.size().iter().all(|size| *size >= 1.0));
            let physical = mapping.map_rectangle(bounds).unwrap();
            let size = mapping.physical_size();
            for axis in 0..2 {
                if physical.size()[axis] <= f64::from(size[axis]) * 0.9
                    && (physical.origin()[axis] < -1.0 || physical.origin()[axis] + physical.size()[axis] > f64::from(size[axis]) + 1.0)
                {
                    capture(fixture, "focus-clipped");
                    panic!("{caption} is outside the lens: axis={axis}, physical={physical:?}, output={size:?}, points={points:?}");
                }
            }
            return entity;
        }
        if Instant::now() >= deadline { break; }
        tap(fixture, 15);
    }
    let observed = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let focus = world.resource::<bevy::input_focus::InputFocus>().get();
        let label = focus.and_then(|entity| world.get::<bevy::a11y::AccessibilityNode>(entity)).and_then(|node| node.label()).map(str::to_owned);
        (focus, label, native::shortcuts::keyboard_owner(world))
    });
    capture(fixture, "focus-unreachable");
    panic!("The ordinary Tab sequence cannot reach {caption} ({entity:?}): focus={observed:?}, native_keyboard={}, input_pending={}, native_ready={}",
        fixture.desktop.native_keyboard, fixture.desktop.input_pending(), fixture.desktop.server.native_ready())
}

fn activate(fixture: &mut Fixture, caption: &'static str) { focus(fixture, caption); tap(fixture, 28); }

pub(super) fn open(fixture: &mut Fixture) {
    fixture.desktop.key(56, true, 1).unwrap(); press(fixture, 18); fixture.desktop.key(56, false, 2).unwrap();
    settle(fixture);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.modal));
    activate(fixture, "Park");
    activate(fixture, "Edit display magnification");
}

fn write(fixture: &mut Fixture, digit: u32) {
    focus(fixture, FACTOR);
    fixture.desktop.key(29, true, 20).unwrap(); press(fixture, 30); fixture.desktop.key(29, false, 21).unwrap();
    tap(fixture, digit);
}

pub(super) fn value(fixture: &mut Fixture) -> String {
    let entity = entity(fixture, FACTOR);
    fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(entity).unwrap().value().to_string())
}

fn snapshot(fixture: &mut Fixture) -> Snapshot { fixture.inspect(|canvas| canvas.world_mut().resource::<Snapshot>().clone()) }

fn view(fixture: &mut Fixture) -> (bevy::math::DVec2, f64) {
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let view = world.get::<native::canvas::CanvasView>(root).unwrap();
        (view.center, view.zoom)
    })
}

fn capture(fixture: &mut Fixture, name: &str) {
    let size = (fixture.size[0] as i32, fixture.size[1] as i32);
    let mapping = {
        let target = fixture.renderer.bind(&mut fixture.texture).unwrap();
        fixture.renderer.copy_framebuffer(&target, smithay::utils::Rectangle::from_size(size.into()), Fourcc::Abgr8888).unwrap()
    };
    let pixels = fixture.renderer.map_texture(&mapping).unwrap();
    assert_eq!(pixels.len(), fixture.size[0] as usize * fixture.size[1] as usize * 4);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[..3] != [0; 3]));
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory); std::fs::create_dir_all(&directory).unwrap();
        let mut file = std::fs::File::create(directory.join(format!("magnification-{name}-{}.ppm", fixture.size[0]))).unwrap();
        use std::io::Write;
        write!(file, "P6\n{} {}\n255\n", fixture.size[0], fixture.size[1]).unwrap();
        let stride = fixture.size[0] as usize * 4;
        for row in 0..fixture.size[1] as usize {
            let row = if mapping.flipped() { fixture.size[1] as usize - row - 1 } else { row };
            for pixel in pixels[row * stride..(row + 1) * stride].chunks_exact(4) { file.write_all(&pixel[..3]).unwrap(); }
        }
    }
}

fn saved(fixture: &Fixture, factor: f64) -> bool {
    fixture.desktop.magnification_snapshot().head(OutputId(1)).is_some_and(|head| head.requested == factor && head.effective == Some(factor))
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_keyboard_magnification_saves_through_the_dispatcher_retries_pressure_and_loads_at_fresh_startup() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string()); std::fs::create_dir(&profile).unwrap();
        let path = profile.join("park.json");
        std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"outputs":{(OUTPUT):{"scale":scale,"magnification":1.0},"Dormant":{"magnification":1.5}},"keyboard":{"layout":"us"}})).unwrap()).unwrap();
        let initial = std::fs::read(&path).unwrap();
        let mut fixture = create(&runtime, &profile, size, scale);
        open(&mut fixture);
        assert_eq!(value(&mut fixture), "1");
        write(&mut fixture, 11);
        let save = entity(&mut fixture, "Save magnification");
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::ui::InteractionDisabled>(save).is_some()));
        activate(&mut fixture, "Cancel draft");
        assert_eq!(value(&mut fixture), "1");
        assert_eq!(std::fs::read(&path).unwrap(), initial);
        let original_view = view(&mut fixture);
        write(&mut fixture, 3);
        focus(&mut fixture, "Save magnification");
        press(&mut fixture, 28);
        let serial = fixture.desktop.worker.input(Input::Test(Box::new(|_| {}))).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(fixture.desktop.worker.failure().is_none());
            if let Some(frame) = fixture.desktop.worker.frame() && frame.input_serial >= serial { break; }
            assert!(Instant::now() < deadline, "The real UI Save did not complete its worker update");
            std::thread::sleep(Duration::from_millis(2));
        }
        let (_, release) = fixture.pause();
        let mut queued = 0;
        while fixture.desktop.worker.input(Input::Pointer([0.0; 2])).is_ok() { queued += 1; assert!(queued <= 128); }
        assert_eq!(queued, 128);
        fixture.desktop.dispatch().unwrap();
        let head = fixture.desktop.magnification_snapshot().head(OutputId(1)).unwrap();
        assert_eq!(head.requested, 2.0);
        assert_eq!(head.effective, Some(1.0));
        assert!(matches!(fixture.desktop.magnification_snapshot().response.as_ref().unwrap().outcome, Outcome::Saved { .. }));
        release.send(()).unwrap();
        fixture.until(|fixture| saved(fixture, 2.0));
        assert_eq!(snapshot(&mut fixture).head(OutputId(1)).unwrap().effective, Some(2.0));
        assert_eq!(view(&mut fixture), original_view);
        let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(disk["outputs"][OUTPUT]["magnification"], 2.0);
        assert_eq!(disk["outputs"]["Dormant"]["magnification"], 1.5);
        assert_eq!(disk["keyboard"]["layout"], "us");
        focus(&mut fixture, "Cancel draft"); capture(&mut fixture, "saved");
        write(&mut fixture, 5);
        activate(&mut fixture, "Cancel draft");
        assert_eq!(value(&mut fixture), "2");
        let mut changed = disk;
        changed["outputs"][OUTPUT]["magnification"] = serde_json::json!(3.0);
        std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        write(&mut fixture, 5);
        let previous = fixture.desktop.magnification_snapshot().last_request;
        activate(&mut fixture, "Save magnification");
        fixture.until(|fixture| fixture.desktop.magnification_snapshot().response.as_ref().is_some_and(|response| response.request > previous && matches!(response.outcome, Outcome::Failed(_))));
        assert_eq!(value(&mut fixture), "4");
        assert!(saved(&fixture, 2.0));
        activate(&mut fixture, "Discard draft and reload saved magnification");
        fixture.until(|fixture| saved(fixture, 3.0));
        assert_eq!(value(&mut fixture), "3");
        focus(&mut fixture, "Reset draft to 1×"); tap(&mut fixture, 28);
        assert_eq!(value(&mut fixture), "1");
        activate(&mut fixture, "Cancel draft");
        assert_eq!(value(&mut fixture), "3");
        capture(&mut fixture, "reloaded");
        fixture.desktop.worker.stop().unwrap(); drop(fixture);
        let mut reopened = create(&runtime, &profile, size, scale);
        assert!(saved(&reopened, 3.0));
        open(&mut reopened);
        assert_eq!(value(&mut reopened), "3");
        focus(&mut reopened, "Reset draft to 1×"); capture(&mut reopened, "restarted");
        reopened.desktop.worker.stop().unwrap();
    }
}
