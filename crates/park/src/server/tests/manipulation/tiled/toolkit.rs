use super::*;
use serde_json::Value;
use std::{
    fs::File,
    process::{Child, Stdio},
};

#[path = "toolkit/handoff.rs"]
mod handoff;
#[path = "toolkit/properties.rs"]
mod properties;

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

struct Toolkit {
    fixture: Fixture,
    process: Process,
    log: PathBuf,
    time: u32,
    started: Instant,
    allow_exit: bool,
}

impl Toolkit {
    fn new(runtime: &tokio::runtime::Runtime, directory: &Path, spatial: bool) -> Self {
        Self::prepared(runtime, directory, spatial, None)
    }

    fn prepared(runtime: &tokio::runtime::Runtime, directory: &Path, spatial: bool, before_launch: Option<fn(&Path)>) -> Self {
        let program = std::env::var_os("LINCE_PARK_TEST_QML")
            .expect("Set LINCE_PARK_TEST_QML to a Qt 6.8+ qml executable");
        let mut fixture = if spatial {
            projected::fixture(runtime, directory, [1000, 700], true, false)
        } else {
            Fixture::prepared(
                runtime,
                directory,
                [1000, 700],
                Some(projected::prepare_flat),
            )
        };
        if let Some(prepare) = before_launch {
            let _ = fixture.identities();
            prepare(directory);
        }
        let source = directory.join("toolkit.qml");
        std::fs::write(&source, include_str!("../../../../../fixtures/toolkit.qml")).unwrap();
        let log_directory = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.into());
        std::fs::create_dir_all(&log_directory).unwrap();
        let log = log_directory.join(format!("qt-toolkit-{spatial}.log"));
        let output = File::create(&log).unwrap();
        let socket = fixture.desktop.server.listen().unwrap();
        let arguments: Vec<String> = std::env::var("LINCE_PARK_TEST_QML_ARGUMENTS")
            .map(|value| serde_json::from_str(&value).unwrap())
            .unwrap_or_default();
        assert!(arguments.len() <= 128);
        let mut application = crate::launch::Application::new(program).unwrap();
        for argument in arguments {
            assert!(argument.len() <= 8192 && !argument.contains('\0'));
            application = application.arg(argument);
        }
        let mut command = application.arg(source).command(&socket).unwrap();
        command
            .env("QT_QUICK_CONTROLS_STYLE", "Basic")
            .env("QT_FORCE_STDERR_LOGGING", "1")
            .env("QT_LOGGING_RULES", "qml.debug=true;qml.info=true")
            .env("XDG_CACHE_HOME", directory.join("cache"))
            .env("XDG_CONFIG_HOME", directory.join("config"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/no-bus", directory.display()),
            )
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        if std::env::var_os("LINCE_PARK_TEST_WAYLAND_TRACE").is_some() {
            command.env("WAYLAND_DEBUG", "1");
        }
        let process = Process(command.spawn().unwrap());
        Self {
            fixture,
            process,
            log,
            time: 1,
            started: Instant::now(),
            allow_exit: false,
        }
    }

    fn state(&self) -> Value {
        std::fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str(line.split_once("PARK_TOOLKIT ")?.1).ok())
            .unwrap_or(Value::Null)
    }

    fn advance_time(&mut self) {
        let elapsed = self
            .started
            .elapsed()
            .as_millis()
            .min(u128::from(u32::MAX - 1)) as u32;
        self.time = self.time.max(elapsed).saturating_add(1);
    }

    fn events(&self, event: &str) -> usize {
        std::fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .filter_map(|line| {
                serde_json::from_str::<Value>(line.split_once("PARK_TOOLKIT ")?.1).ok()
            })
            .filter(|state| state["event"] == event)
            .count()
    }

    fn diagnostics(&self) -> String {
        std::fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn tick(&mut self, stage: &str) -> Vec<u8> {
        let status = self.process.0.try_wait().unwrap();
        assert!(
            status.is_none() || self.allow_exit && status.is_some_and(|status| status.success()),
            "Qt exited: {}",
            self.diagnostics()
        );
        let scene = self
            .fixture
            .directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let name = format!("toolkit-{scene}-{stage}");
        let serial = barrier(&mut self.fixture.desktop);
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            self.fixture.sync();
            self.fixture.desktop.dispatch().unwrap();
            if self
                .fixture
                .desktop
                .accept(&mut self.fixture.composition, &mut self.fixture.renderer)
                .unwrap()
            {
                let frame = self.fixture.desktop.pending.clone().unwrap();
                let ready = frame.input_serial >= serial;
                let pixels = self.fixture.paint(&frame, None);
                if ready {
                    self.fixture.capture(&name, &pixels);
                    self.fixture.verify_clients(&frame, &name, &pixels);
                    return pixels;
                }
            }
            assert!(self.fixture.desktop.worker.failure().is_none());
            assert!(
                Instant::now() < deadline,
                "The Qt compositor frame did not complete"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait(&mut self, stage: &str, mut ready: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(40);
        while !ready(self) {
            self.tick(stage);
            assert!(
                Instant::now() < deadline,
                "Qt {stage} timed out; state={}, windows={:?}; keys={:?}, modifiers={:?}, consumed={:?}, pending={:?}, deferred={}, native={}, modal={:?}, focus={:?}, recent-bindings={}; log={}",
                self.state(),
                self.fixture
                    .desktop
                    .server
                    .windows()
                    .iter()
                    .map(|window| (&window.title, window.mapped, window.parent))
                    .collect::<Vec<_>>(),
                self.fixture.desktop.keys.keys().collect::<Vec<_>>(),
                self.fixture.desktop.server.state.seat.get_keyboard().unwrap().modifier_state(),
                self.fixture.desktop.consumed_actions,
                self.fixture.desktop.pending_output,
                self.fixture.desktop.deferred.len(),
                self.fixture.desktop.native_keyboard,
                self.fixture.desktop.current.as_ref().map(|frame| (frame.frame.input.keyboard_modal, frame.frame.input.keyboard_capture)),
                self.fixture.desktop.current.as_ref().and_then(|frame| frame.frame.geometry.as_ref()).and_then(|geometry| geometry.focused),
                self.fixture.desktop.current.as_ref().map_or(0, |frame| frame.frame.input.bindings.iter().filter(|binding| binding.action.is_recent()).count()),
                self.diagnostics()
            );
        }
    }

    fn window(&self, title: &str) -> Option<WindowSnapshot> {
        self.fixture
            .desktop
            .server
            .windows()
            .into_iter()
            .find(|window| window.title == title && window.mapped)
    }

    fn geometry(&self, id: SurfaceId) -> ItemGeometry {
        self.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(id))
            .unwrap()
            .clone()
    }

    fn point(&self, id: SurfaceId, group: &str, control: &str) -> [f64; 2] {
        let state = self.state();
        let rect = state[group][control].as_array().unwrap();
        let point = [
            rect[0].as_f64().unwrap() + rect[2].as_f64().unwrap() * 0.5,
            rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap() * 0.5,
        ];
        let item = self.geometry(id);
        let size = self
            .fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .windows
            .iter()
            .find(|window| window.id == id)
            .unwrap()
            .bounds()
            .unwrap()
            .size;
        let size = [size.w, size.h];
        let point = item
            .mapping
            .canvas_point(std::array::from_fn(|axis| {
                item.mapping.surface.origin()[axis]
                    + point[axis] * item.mapping.surface.size()[axis] / size[axis]
            }))
            .unwrap();
        assert!(
            item.surface_point(point).is_some(),
            "Qt {control} is outside the visible item at {point:?}: {item:?}"
        );
        assert!(
            self.fixture.viewport().bounds.contains(point),
            "Qt {control} is outside its output at {point:?}"
        );
        point
    }

    fn button(&mut self, code: u32, pressed: bool) {
        self.advance_time();
        self.fixture
            .desktop
            .button(code, pressed, self.time)
            .unwrap();
        self.tick("button");
    }

    fn click(&mut self, id: SurfaceId, group: &str, control: &str, button: u32) {
        let point = self.point(id, group, control);
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("hover");
        assert!(
            matches!(self.fixture.desktop.target(), crate::desktop::input::Target::Application(target, _, _) if target == id),
            "The Qt {control} control is not the presented application pointer target"
        );
        self.button(button, true);
        self.button(button, false);
    }

    fn type_key(&mut self, code: u32) {
        for pressed in [true, false] {
            self.advance_time();
            self.fixture.desktop.key(code, pressed, self.time).unwrap();
        }
        self.tick("key");
    }

    fn chord(&mut self, events: &[(u32, bool)]) {
        for (code, pressed) in events {
            self.advance_time();
            self.fixture
                .desktop
                .key(*code, *pressed, self.time)
                .unwrap();
        }
        self.tick("chord");
    }

    fn normal_click(&mut self, main: SurfaceId, count: u64) {
        self.click(main, "controls", "count", 272);
        self.wait("normal-click", |app| app.state()["clicks"] == count);
        assert!(
            self.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert!(self.fixture.desktop.server.state.buttons.is_empty());
    }

    fn configured(&mut self, main: SurfaceId) {
        self.configured_size(main, "size");
    }

    fn configured_size(&mut self, main: SurfaceId, key: &str) {
        self.wait("configured", |app| {
            let current = app.fixture.desktop.current.as_ref().unwrap();
            current
                .frame
                .windows
                .iter()
                .find(|window| window.surface == main)
                .is_some_and(|window| {
                    app.state()[key] == serde_json::json!(window.size)
                        && current
                            .windows
                            .iter()
                            .find(|snapshot| snapshot.id == main)
                            .and_then(WindowSnapshot::bounds)
                            .is_some_and(|bounds| {
                                bounds.size.w == f64::from(window.size[0])
                                    && bounds.size.h == f64::from(window.size[1])
                            })
                })
        });
        self.tick("configured-visible");
    }

    fn size(&self, main: SurfaceId) -> [u32; 2] {
        self.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .windows
            .iter()
            .find(|window| window.surface == main)
            .unwrap()
            .size
    }

    fn drag(&mut self, main: SurfaceId, resize: bool, modifier: bool, commit: bool) {
        self.configured(main);
        let before = self.geometry(main).mapping;
        let size = self.size(main);
        let point = self.point(main, "controls", if resize { "resize" } else { "move" });
        self.advance_time();
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.tick("drag-hover");
        assert!(
            matches!(self.fixture.desktop.target(), crate::desktop::input::Target::Application(id, _, _) if id == main),
            "The Qt drag handle is not the presented pointer target"
        );
        if modifier {
            self.chord(&[(125, true)]);
        }
        let button = if resize && modifier { 273 } else { 272 };
        self.button(button, true);
        self.wait("drag-start", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_some_and(|active| active.target.item == ItemId::Application(main))
        });
        if modifier {
            self.chord(&[(125, false)]);
        }
        let local = before.surface_point_unbounded(point).unwrap();
        let next = before
            .canvas_point(std::array::from_fn(|axis| {
                local[axis]
                    + [28.0, 16.0][axis] * before.surface.size()[axis] / f64::from(size[axis])
            }))
            .unwrap();
        assert!(self.fixture.viewport().bounds.contains(next));
        self.advance_time();
        self.fixture.desktop.move_pointer(next, self.time).unwrap();
        self.tick("drag-preview");
        if resize {
            self.wait("resize-preview", |app| app.size(main) != size);
        }
        if !commit {
            self.type_key(1);
        }
        self.button(button, false);
        self.wait("drag-finished", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        });
        self.configured(main);
        if resize {
            assert_eq!(
                self.size(main),
                if commit {
                    [size[0] + 28, size[1] + 16]
                } else {
                    size
                }
            );
        } else if commit {
            assert_ne!(
                self.geometry(main).mapping.canvas.origin(),
                before.canvas.origin()
            );
        } else {
            assert_eq!(self.geometry(main).mapping, before);
        }
        assert!(self.fixture.desktop.server.state.buttons.is_empty());
    }

    fn nested_menu(&mut self, main: SurfaceId) {
        let confirmations = self.events("nested-confirm");
        self.click(main, "controls", "menu", 272);
        self.wait("nested-parent", |app| app.popup(main));
        self.type_key(108);
        self.wait("nested-first", |app| app.state()["menuIndex"] == 0);
        self.type_key(108);
        self.wait("nested-more", |app| app.state()["menuIndex"] == 1);
        self.type_key(106);
        self.wait("nested-open", |app| {
            app.fixture
                .desktop
                .popups
                .iter()
                .filter(|popup| {
                    popup.owner == crate::desktop::renderer::PopupOwner::Application(main)
                })
                .count()
                == 2
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let image = self.tick("nested-visible");
            let (quadrants, _) = self.menu_pixels(main, &image, true);
            if quadrants.iter().all(|count| *count >= 8) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Missing nested menu quadrants: {quadrants:?}"
            );
        }
        self.type_key(108);
        self.wait("nested-selection", |app| app.state()["nestedIndex"] == 0);
        self.type_key(28);
        self.wait("nested-confirm", |app| {
            app.events("nested-confirm") == confirmations + 1
                && !app.popup(main)
                && !app.live_popup(main)
        });
    }

    fn native_handoff(&mut self, main: SurfaceId) {
        self.click(main, "controls", "field", 272);
        let text = self.state()["text"].as_str().unwrap().to_owned();
        self.chord(&[(125, true), (25, true), (25, false), (125, false)]);
        self.wait("native-editor", |app| {
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
        self.type_key(44);
        self.wait("native-text", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .as_ref()
                .is_some_and(|editor| editor.surrounding == "z")
        });
        assert_eq!(self.state()["text"], text);
        self.type_key(1);
        self.wait("native-close", |app| {
            app.fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .is_none()
        });
        self.click(main, "controls", "field", 272);
        assert_eq!(self.state()["text"], text);
        self.type_key(107);
        self.type_key(32);
        self.wait("native-return", |app| {
            app.state()["text"] == format!("{text}d")
        });
    }

    fn popup(&self, main: SurfaceId) -> bool {
        self.fixture
            .desktop
            .popups
            .iter()
            .any(|popup| popup.owner == crate::desktop::renderer::PopupOwner::Application(main))
    }

    fn live_popup(&self, main: SurfaceId) -> bool {
        self.fixture.desktop.server.windows().iter().any(|window| {
            window.id == main && window.popups.iter().any(|popup| !popup.nodes.is_empty())
        })
    }

    fn menu_pixels(&self, main: SurfaceId, pixels: &[u8], nested: bool) -> ([usize; 4], usize) {
        let popup = self
            .fixture
            .desktop
            .popups
            .iter()
            .filter(|popup| popup.owner == crate::desktop::renderer::PopupOwner::Application(main))
            .nth(usize::from(nested))
            .unwrap();
        let viewport = self.fixture.viewport();
        let rectangle = popup.mapping.canvas;
        let low = viewport
            .raster
            .point(rectangle.origin())
            .map(|value| value.max(0.0).floor() as u32);
        let high = viewport
            .raster
            .point(std::array::from_fn(|axis| {
                rectangle.origin()[axis] + rectangle.size()[axis]
            }))
            .map(|value| value.max(0.0).ceil() as u32);
        let high = [
            high[0].min(self.fixture.size[0]),
            high[1].min(self.fixture.size[1]),
        ];
        let middle = [(low[0] + high[0]) / 2, (low[1] + high[1]) / 2];
        let parent = self.geometry(main);
        let mut quadrants = [0; 4];
        let mut outside = 0;
        for y in low[1]..high[1] {
            for x in low[0]..high[0] {
                let index = (y as usize * self.fixture.size[0] as usize + x as usize) * 4;
                if pixels[index..index + 3]
                    .iter()
                    .zip(if nested {
                        [101_u8, 159, 136]
                    } else {
                        [214_u8, 168, 47]
                    })
                    .all(|(actual, expected)| actual.abs_diff(expected) < 12)
                {
                    quadrants[usize::from(x >= middle[0]) + 2 * usize::from(y >= middle[1])] += 1;
                    let point = viewport
                        .raster
                        .logical_point([f64::from(x) + 0.5, f64::from(y) + 0.5]);
                    outside += usize::from(parent.surface_point(point).is_none());
                }
            }
        }
        (quadrants, outside)
    }

    fn menu(&mut self, main: SurfaceId, right: bool) {
        self.tick("before-menu");
        let confirmations = self.events("menu-confirm");
        self.click(main, "controls", "menu", if right { 273 } else { 272 });
        self.wait("menu", |app| app.popup(main));
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let image = self.tick("menu-visible");
            let (quadrants, outside) = self.menu_pixels(main, &image, false);
            if quadrants.iter().all(|count| *count >= 8) && (right || outside >= 8) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Qt popup pixels are missing: quadrants={quadrants:?}, outside={outside}, right={right}"
            );
        }
        self.type_key(108);
        self.wait("menu-selection", |app| app.state()["menuIndex"] == 0);
        self.type_key(28);
        self.wait("menu-confirm", |app| {
            app.events("menu-confirm") == confirmations + 1
                && !app.popup(main)
                && !app.live_popup(main)
        });
    }
}

fn exercise(spatial: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut app = Toolkit::new(&runtime, directory.path(), spatial);
    app.wait("map", |app| {
        app.window("Park toolkit parent").is_some() && !app.state().is_null()
    });
    let main = app.window("Park toolkit parent").unwrap().id;
    app.fixture
        .desktop
        .worker
        .input(Input::Activate(main))
        .unwrap();
    app.tick("parent");
    app.configured(main);
    app.click(main, "controls", "field", 272);
    app.type_key(30);
    app.wait("parent-text", |app| app.state()["text"] == "a");
    app.normal_click(main, 1);
    app.menu(main, false);
    app.menu(main, true);
    app.nested_menu(main);
    app.normal_click(main, 2);
    app.click(main, "controls", "dialog", 272);
    app.wait("dialog", |app| app.window("Park toolkit dialog").is_some());
    let dialog = app.window("Park toolkit dialog").unwrap();
    assert_eq!(dialog.parent, Some(main));
    app.wait("dialog-focus", |app| {
        app.fixture
            .desktop
            .server
            .state
            .seat
            .get_keyboard()
            .unwrap()
            .current_focus()
            == Some(dialog.surface.clone())
    });
    app.configured_size(dialog.id, "dialogSize");
    app.type_key(48);
    app.wait("dialog-text", |app| app.state()["dialogText"] == "b");
    assert_eq!(app.state()["text"], "a");
    app.click(dialog.id, "dialogControls", "close", 272);
    app.wait("dialog-close", |app| {
        app.window("Park toolkit dialog").is_none()
    });
    let parent = app.window("Park toolkit parent").unwrap();
    app.wait("parent-focus", |app| {
        app.fixture
            .desktop
            .server
            .state
            .seat
            .get_keyboard()
            .unwrap()
            .current_focus()
            == Some(parent.surface.clone())
    });
    app.click(main, "controls", "field", 272);
    app.type_key(46);
    app.wait("parent-recovery", |app| app.state()["text"] == "ac");
    app.normal_click(main, 3);
    app.chord(&[(125, true), (20, true), (20, false), (125, false)]);
    app.tick("free");
    app.chord(&[(125, true), (14, true), (14, false), (125, false)]);
    app.configured(main);
    app.tick("free-focused");
    app.drag(main, false, false, true);
    app.normal_click(main, 4);
    app.drag(main, true, false, true);
    app.normal_click(main, 5);
    app.drag(main, false, true, false);
    app.normal_click(main, 6);
    app.drag(main, true, true, false);
    app.normal_click(main, 7);
    app.native_handoff(main);
    app.normal_click(main, 8);
    app.capture_handoff(main);
    app.normal_click(main, 9);
    app.dialog_handoff(main);
    app.normal_click(main, 10);
    app.menu(main, true);
    app.fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn flat_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input() {
    exercise(false);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn spatial_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input() {
    exercise(true);
}
