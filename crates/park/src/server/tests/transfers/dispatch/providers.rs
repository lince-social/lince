use super::*;
use crate::launch::{Application, Capability, Description};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Child,
};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.0.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Provider {
    fixture: Fixture,
    process: Option<Process>,
    program: PathBuf,
    socket: PathBuf,
    directory: PathBuf,
}

impl Drop for Provider {
    fn drop(&mut self) {
        if let Ok(pid) = std::fs::read_to_string(self.directory.join("terminal.pid"))
            && let Ok(pid) = pid.parse::<i32>()
            && let Some(pid) = rustix::process::Pid::from_raw(pid)
        {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
        }
    }
}

impl Provider {
    fn tick(&mut self) {
        self.fixture.tick();
        self.fixture
            .composition
            .capture(&mut self.fixture.renderer, &mut self.fixture.desktop, true)
            .unwrap();
        self.fixture.exchange();
    }

    fn wait(&mut self, subject: &str, mut ready: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            self.tick();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Provider {subject} timed out: usable={:?}, layers={:?}, log={}",
                self.usable(),
                self.fixture
                    .desktop
                    .server
                    .layers()
                    .iter()
                    .map(|layer| (&layer.namespace, layer.origin))
                    .collect::<Vec<_>>(),
                std::fs::read_to_string(self.directory.join("provider.log")).unwrap_or_default()
            );
            if let Some(process) = &mut self.process {
                assert!(
                    process.0.try_wait().unwrap().is_none(),
                    "The provider exited during {subject}"
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn ipc(&mut self, arguments: &[&str]) -> Value {
        let output = self.directory.join("ipc.stdout");
        let error = self.directory.join("ipc.stderr");
        let mut command = Application::new(self.program.as_os_str())
            .unwrap()
            .arg("msg");
        for argument in arguments {
            command = command.arg(*argument);
        }
        let mut command = command.command(&self.socket).unwrap();
        let mut process = Process(
            command
                .stdout(std::fs::File::create(&output).unwrap())
                .stderr(std::fs::File::create(&error).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.tick();
            if let Some(status) = process.0.try_wait().unwrap() {
                let output = std::fs::read_to_string(output).unwrap();
                assert!(
                    status.success(),
                    "Provider IPC {arguments:?} failed: {output}; {}",
                    std::fs::read_to_string(error).unwrap()
                );
                return serde_json::from_str(&output).unwrap_or(Value::String(output));
            }
            assert!(
                Instant::now() < deadline,
                "Provider IPC {arguments:?} timed out"
            );
        }
    }

    fn usable(&self) -> [i32; 4] {
        let usable = self
            .fixture
            .desktop
            .server
            .usable_output(OutputId(1))
            .unwrap();
        [usable.loc.x, usable.loc.y, usable.size.w, usable.size.h]
    }

    fn published(&mut self) {
        self.wait("published reserved space", |app| {
            let usable = app.usable();
            app.fixture
                .desktop
                .current
                .as_ref()
                .and_then(|frame| frame.frame.geometry.as_ref())
                .is_some_and(|geometry| {
                    geometry
                        .outputs
                        .iter()
                        .find(|output| output.id == OutputId(1))
                        .is_some_and(|output| {
                            output.usable.is_some_and(|rectangle| {
                                rectangle.origin() == [usable[0], usable[1]].map(f64::from)
                                    && rectangle.size() == [usable[2], usable[3]].map(f64::from)
                            })
                        })
                })
        });
    }
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the selected Noctalia provider"]
fn real_external_provider_reserves_restores_launches_and_captures_through_standard_wayland_interfaces()
 {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(directory.join("screenshots")).unwrap();
    let config =
        PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").expect("Use a private XDG_CONFIG_HOME"))
            .join("noctalia");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("config.toml"), format!("[bar]\norder=[\"default\"]\n[bar.default]\nstart=[]\ncenter=[\"clock\"]\nend=[\"launcher\"]\nposition=\"bottom\"\nthickness=32\npadding=0\nmargin_ends=0\npanel_overlap=0\nreserve_space=true\nradius=0\nconcave_edge_corners=false\n[shell]\nlaunch_apps_as_systemd_services=false\nsetup_wizard_enabled=false\noffline_mode=true\n[shell.screenshot]\ndirectory={}\nsave_to_file=true\ncopy_to_clipboard=false\nannotate=false\n", serde_json::to_string(&directory.join("screenshots")).unwrap())).unwrap();
    let applications =
        PathBuf::from(std::env::var_os("XDG_DATA_HOME").expect("Use a private XDG_DATA_HOME"))
            .join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    let launch = directory.join("launch-terminal");
    let terminal = std::env::var("LINCE_PARK_TEST_TERMINAL").expect("Set LINCE_PARK_TEST_TERMINAL");
    script(
        &launch,
        &format!(
            "printf '%s' \"$$\" > '{}'\nexec '{}' --config-default-files=false --title=Park-provider-launch -e bash --noprofile --norc '{}'",
            directory.join("terminal.pid").display(),
            terminal,
            directory.join("terminal-client").display()
        ),
    );
    script(
        &directory.join("terminal-client"),
        &format!(
            "printf ready > '{}'\nsleep 60",
            directory.join("launch-ready").display()
        ),
    );
    std::fs::write(applications.join("park-provider-validation.desktop"), format!("[Desktop Entry]\nType=Application\nName=Park Provider Validation\nExec={}\nTerminal=false\n",launch.display())).unwrap();
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[Monitor {
            id: OutputId(1),
            name: "Park provider".into(),
            origin: [0, 0],
            size: [1000, 720],
        }])
        .unwrap();
    harness.sync();
    let window = harness.window();
    window.role.set_title("Provider reference".into());
    let _buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let reference = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, temporary.path());
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        native::workspace_config::set_physics(world, root, 1, false);
        world
            .get_mut::<native::topology::view::View>(root)
            .unwrap()
            .spatial = false;
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Tiled))
            .unwrap();
    });
    fixture.desktop.send(Input::Application(reference)).unwrap();
    fixture.inspect(|_| ());
    let socket = fixture.desktop.server.listen().unwrap();
    let program = PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_PROVIDER").expect("Set LINCE_PARK_TEST_PROVIDER"),
    );
    let wrapper = directory.join("provider-wrapper");
    script(
        &wrapper,
        &format!(
            "export DBUS_SYSTEM_BUS_ADDRESS=\"$DBUS_SESSION_BUS_ADDRESS\"\nexec '{}' > '{}' 2>&1",
            program.display(),
            directory.join("provider.log").display()
        ),
    );
    let process = fixture
        .desktop
        .server
        .spawn_helper(
            &Description {
                program: wrapper.to_string_lossy().into_owned(),
                arguments: vec![],
                directory: None,
                capabilities: vec![
                    Capability::Capture,
                    Capability::WindowList,
                    Capability::Clipboard,
                ],
            },
            &socket,
        )
        .unwrap();
    let mut app = Provider {
        fixture,
        process: Some(Process(process)),
        program,
        socket,
        directory,
    };
    let mut results = json!({"task":"A2.6","observations":[],"passed":false});
    for subject in ["bar-reservation", "launcher", "authorized-capture"] {
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match subject {
            "bar-reservation" => {
                app.wait("bar map", |app| app.usable()[3] < 720);
                app.published();
                let shown = app.usable();
                assert_eq!(shown, [0, 0, 1000, 688]);
                app.ipc(&["bar-hide"]);
                app.wait("bar hides", |app| app.usable() == [0, 0, 1000, 720]);
                app.published();
                let hidden = app.usable();
                app.ipc(&["bar-show"]);
                app.wait("bar restores", |app| app.usable() == shown);
                app.published();
                json!({"shown":shown,"hidden":hidden,"restored":app.usable(),"status":app.ipc(&["status"])})
            }
            "launcher" => {
                app.ipc(&["panel-open", "launcher", "Park Provider Validation"]);
                app.wait("launcher input geometry", |app| {
                    let desktop = &app.fixture.desktop;
                    desktop.pending_output.is_none() && desktop.layers.iter().any(|layer| {
                        if layer.namespace != "noctalia-panel"
                            || layer.keyboard == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::None
                        { return false; }
                        let Some(node) = layer.nodes.first() else { return false; };
                        let point = [f64::from(layer.origin[0]) + node.layout.size[0] * 0.5,
                            f64::from(layer.origin[1]) + 48.0];
                        matches!(desktop.target_at(point), crate::desktop::input::Target::Layer(ref root, _) if root == &layer.surface)
                    })
                });
                let layer = app
                    .fixture
                    .desktop
                    .layers
                    .iter()
                    .find(|layer| layer.namespace == "noctalia-panel")
                    .unwrap()
                    .clone();
                let point = [
                    f64::from(layer.origin[0]) + layer.nodes[0].layout.size[0] * 0.5,
                    f64::from(layer.origin[1]) + 48.0,
                ];
                app.fixture.desktop.move_pointer(point, 90).unwrap();
                app.tick();
                for pressed in [true, false] {
                    app.fixture.desktop.button(272, pressed, 91).unwrap();
                    app.tick();
                }
                app.wait("launcher focused", |app| {
                    app.fixture.desktop.layer_focus.as_ref() == Some(&layer.surface)
                });
                for pressed in [true, false] {
                    app.fixture.desktop.key(28, pressed, 100).unwrap();
                    app.tick();
                }
                app.wait("application map", |app| {
                    app.fixture
                        .desktop
                        .server
                        .windows()
                        .iter()
                        .any(|window| window.mapped && window.title == "Park-provider-launch")
                });
                let launched = app
                    .fixture
                    .desktop
                    .server
                    .windows()
                    .into_iter()
                    .find(|window| window.mapped && window.title == "Park-provider-launch")
                    .unwrap()
                    .id;
                app.fixture
                    .desktop
                    .send(Input::Application(launched))
                    .unwrap();
                app.fixture.inspect(|_| ());
                app.wait("application readiness", |app| {
                    app.directory.join("launch-ready").exists()
                });
                json!({"desktop_entry":"Park Provider Validation","native_application":"Park-provider-launch"})
            }
            "authorized-capture" => {
                app.ipc(&["panel-close"]);
                app.fixture
                    .desktop
                    .send(Input::Application(reference))
                    .unwrap();
                app.fixture.inspect(|_| ());
                app.fixture
                    .desktop
                    .move_pointer([400.0, 300.0], 200)
                    .unwrap();
                app.tick();
                let selection = app.directory.join("selection.txt");
                let image = app.directory.join("capture.ppm");
                let pipeline = app.directory.join("capture-region");
                let grim = std::env::var("LINCE_PARK_TEST_GRIM").expect("Set LINCE_PARK_TEST_GRIM");
                let slurp =
                    std::env::var("LINCE_PARK_TEST_SLURP").expect("Set LINCE_PARK_TEST_SLURP");
                script(
                    &pipeline,
                    &format!(
                        "exec > '{}' 2>&1\nregion=$(env -u WAYLAND_SOCKET '{}' -f '%x,%y %wx%h') || exit 1\nprintf '%s' \"$region\" > '{}'\nexec '{}' -t ppm -g \"$region\" '{}'",
                        app.directory.join("capture.log").display(),
                        slurp,
                        selection.display(),
                        grim,
                        image.display()
                    ),
                );
                let mut process = Process(
                    app.fixture
                        .desktop
                        .server
                        .spawn_helper(
                            &Description {
                                program: pipeline.to_string_lossy().into_owned(),
                                arguments: vec![],
                                directory: None,
                                capabilities: vec![Capability::Capture],
                            },
                            &app.socket,
                        )
                        .unwrap(),
                );
                app.wait("region selector", |app| {
                    app.fixture
                        .desktop
                        .server
                        .layers()
                        .iter()
                        .any(|layer| layer.namespace == "selection")
                });
                app.fixture.inspect(|_| ());
                app.fixture
                    .desktop
                    .move_pointer([200.0, 180.0], 201)
                    .unwrap();
                app.tick();
                app.fixture.desktop.button(272, true, 202).unwrap();
                app.tick();
                app.fixture
                    .desktop
                    .move_pointer([440.0, 360.0], 203)
                    .unwrap();
                app.tick();
                app.fixture.desktop.button(272, false, 204).unwrap();
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    app.tick();
                    if let Some(status) = process.0.try_wait().unwrap() {
                        assert!(
                            status.success(),
                            "Grim/Slurp failed: {status}; {}",
                            std::fs::read_to_string(app.directory.join("capture.log")).unwrap()
                        );
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Grim/Slurp did not finish its authorized region capture"
                    );
                }
                let region = std::fs::read_to_string(selection)
                    .unwrap()
                    .trim()
                    .to_owned();
                assert_eq!(
                    region, "200,180 241x181",
                    "The external selector returned the wrong pointer geometry"
                );
                let image = std::fs::read(image).unwrap();
                let pixels = image
                    .strip_prefix(b"P6\n241 181\n255\n")
                    .expect("Grim returned the wrong region dimensions");
                assert_eq!(pixels.len(), 241 * 181 * 3);
                let green = pixels
                    .chunks_exact(3)
                    .filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40)
                    .count();
                assert!(
                    green > 200,
                    "Grim lost the known application's actual pixels"
                );
                json!({"provider":"Grim 1.5 / Slurp 1.5","region":region,"size":[241,181],"green_pixels":green,"protocol":"ext-image-copy-capture","noctalia_screenshot":"The installed Noctalia's built-in screenshot command still requires wlr-screencopy; the tested external workflow uses Grim/Slurp"})
            }
            _ => unreachable!(),
        }));
        let observation = match outcome {
            Ok(evidence) => {
                json!({"subject":subject,"passed":true,"evidence":evidence,"seconds":started.elapsed().as_secs_f64()})
            }
            Err(error) => {
                json!({"subject":subject,"passed":false,"error":error.downcast_ref::<String>().cloned().or_else(||error.downcast_ref::<&str>().map(|value|value.to_string())).unwrap_or_else(||"panic".into()),"seconds":started.elapsed().as_secs_f64()})
            }
        };
        eprintln!("Provider workflow: {observation}");
        results["observations"]
            .as_array_mut()
            .unwrap()
            .push(observation);
        std::fs::write(
            app.directory.join("provider-results.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
    }
    app.process.take();
    app.wait("provider exit restores space", |app| {
        app.usable() == [0, 0, 1000, 720]
    });
    results["exit_restored_space"] = json!(true);
    results["passed"] = json!(
        results["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|observation| observation["passed"] == true)
    );
    std::fs::write(
        app.directory.join("provider-results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    assert_eq!(
        results["passed"], true,
        "External provider workflow failed: {results}"
    );
}
