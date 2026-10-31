use super::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    os::unix::process::CommandExt,
    process::{Child, ChildStdin, Stdio},
    sync::mpsc,
};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
        }
        let _ = self.0.wait();
    }
}

struct Ordinary {
    fixture: Fixture,
    driver: Process,
    input: ChildStdin,
    replies: mpsc::Receiver<Value>,
    time: u32,
    directory: std::path::PathBuf,
}

impl Ordinary {
    fn request(&mut self, request: Value) -> Value {
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(50);
        loop {
            self.fixture.tick();
            if let Ok(response) = self.replies.try_recv() {
                assert_eq!(response["ok"], true, "WebDriver failed: {response}");
                return response["value"].clone();
            }
            assert!(
                self.driver.0.try_wait().unwrap().is_none(),
                "The private browser driver exited"
            );
            assert!(
                Instant::now() < deadline,
                "The private browser driver timed out: {request}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn js(&mut self, script: &str) -> Value {
        self.request(json!({"operation":"js", "script":script}))
    }

    fn key(&mut self, code: u32, pressed: bool) {
        self.time += 1;
        self.fixture.desktop.key(code, pressed, self.time).unwrap();
        self.fixture.tick();
    }

    fn tap(&mut self, code: u32) {
        self.key(code, true);
        self.key(code, false);
    }

    fn chord(&mut self, modifiers: &[u32], code: u32) {
        for &modifier in modifiers {
            self.key(modifier, true);
        }
        self.tap(code);
        for &modifier in modifiers.iter().rev() {
            self.key(modifier, false);
        }
        self.fixture.inspect(|_| ());
    }

    fn window(&mut self, browser: bool) -> SurfaceId {
        let deadline = Instant::now() + Duration::from_secs(70);
        loop {
            self.fixture.tick();
            let fixture = &self.fixture;
            let found = fixture
                .desktop
                .server
                .windows()
                .into_iter()
                .find(|window| {
                    window.mapped
                        && if browser {
                            window.app_id.to_lowercase().contains("firefox")
                        } else {
                            window.app_id.to_lowercase().contains("ghostty")
                        }
                })
                .map(|window| window.id);
            if let Some(id) = found {
                self.activate(id);
                return id;
            }
            assert!(
                Instant::now() < deadline,
                "Application did not map: browser={browser}, windows={:?}",
                fixture
                    .desktop
                    .server
                    .windows()
                    .iter()
                    .map(|window| (&window.app_id, &window.title, window.mapped))
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn activate(&mut self, id: SurfaceId) {
        self.fixture.desktop.send(Input::Application(id)).unwrap();
        self.fixture.inspect(|_| ());
    }

    fn point(&self, id: SurfaceId, local: [f64; 2]) -> [f64; 2] {
        let item = self
            .fixture
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
            .unwrap();
        let bounds = self
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
            .unwrap();
        item.mapping
            .canvas_point([local[0] - bounds.loc.x, local[1] - bounds.loc.y])
            .expect("The browser control needs visible geometry")
    }

    fn click(&mut self, id: SurfaceId, local: [f64; 2], button: u32) {
        let point = self.point(id, local);
        self.time += 1;
        self.fixture.desktop.move_pointer(point, self.time).unwrap();
        self.fixture.inspect(|_| ());
        for pressed in [true, false] {
            self.time += 1;
            self.fixture
                .desktop
                .button(button, pressed, self.time)
                .unwrap();
            self.fixture.tick();
        }
        self.fixture.inspect(|_| ());
    }

    fn field(&mut self) -> [f64; 2] {
        let value = self.js("if(!window.pointerTrace){window.pointerTrace=[];document.addEventListener('pointerdown',e=>pointerTrace.push({x:e.clientX,y:e.clientY,tag:e.target.tagName}));}const r=document.querySelector('input').getBoundingClientRect();return [r.x+r.width/2+window.mozInnerScreenX-window.screenX,r.y+r.height/2+window.mozInnerScreenY-window.screenY]");
        [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
    }

    fn value(&mut self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let observed = self.js("return document.querySelector('input').value");
            if observed
                .as_str()
                .is_some_and(|value| value.trim() == expected)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Browser input mismatch: {observed}, expected {expected}"
            );
        }
    }

    fn rectangle(&mut self, id: SurfaceId) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut previous = Value::Null;
        loop {
            let rectangle =
                self.request(json!({"operation":"command","method":"GET","path":"/window/rect"}));
            let current = self.fixture.desktop.current.as_ref().unwrap();
            let window = current
                .windows
                .iter()
                .find(|window| window.id == id)
                .unwrap();
            let root = window
                .nodes
                .iter()
                .find(|node| node.surface == window.surface)
                .unwrap();
            if previous == rectangle
                && [
                    rectangle["width"].as_f64().unwrap(),
                    rectangle["height"].as_f64().unwrap(),
                ] == root.layout.size
            {
                return rectangle;
            }
            assert!(
                Instant::now() < deadline,
                "The browser did not settle its committed geometry: rectangle={rectangle}, buffer={:?}",
                root.layout.size
            );
            previous = rectangle;
        }
    }
}

#[test]
#[ignore = "requires hardware Vulkan/EGL, Firefox, Ghostty and geckodriver"]
fn real_browser_and_terminal_keep_native_typing_popups_fullscreen_clipboard_and_file_drop() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("page.html"), "<!doctype html><title>Park ordinary workflow</title><style>body{background:#f5faf5;margin:20px}input{width:280px;height:40px}#drop{background:#20df80;height:180px;margin-top:20px}</style><input autofocus><div id=drop>Drop a file here</div><video autoplay muted loop width=220 src=video.webm></video><script>window.dropped=null;window.dragEvents=[];drop.ondragover=e=>{e.preventDefault();e.dataTransfer.dropEffect='copy';dragEvents.push(e.type)};drop.ondrop=async e=>{e.preventDefault();dragEvents.push(e.type);window.dropped=await Promise.all([...e.dataTransfer.files].map(async f=>({name:f.name,text:await f.text()})))}</script>").unwrap();
    std::fs::write(
        directory.join("video.webm"),
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../xtask/fixtures/showoff.webm"
        )),
    )
    .unwrap();
    std::fs::write(
        directory.join("driver.py"),
        include_str!("../../../../../fixtures/ordinary.py"),
    )
    .unwrap();
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[Monitor {
            id: OutputId(1),
            name: "Ordinary applications".into(),
            origin: [0, 0],
            size: [1000, 720],
        }])
        .unwrap();
    harness.sync();
    let source_window = harness.window();
    let _source_buffer = paint(&mut harness, &source_window, [320, 240], [20, 20, 240, 255]);
    let source_id = harness.server.windows()[0].id;
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
    let socket = fixture.desktop.server.listen().unwrap();
    let mut command = crate::launch::Application::new(
        std::env::var_os("LINCE_PARK_TEST_PYTHON").expect("Set LINCE_PARK_TEST_PYTHON"),
    )
    .unwrap()
    .arg(directory.join("driver.py"))
    .arg(&directory)
    .command(&socket)
    .unwrap();
    let mut driver = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(directory.join("driver.log")).unwrap())
        .process_group(0)
        .spawn()
        .unwrap();
    let input = driver.stdin.take().unwrap();
    let output = driver.stdout.take().unwrap();
    let (sender, replies) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str(&line) {
                let _ = sender.send(value);
            }
        }
    });
    let mut app = Ordinary {
        fixture,
        driver: Process(driver),
        input,
        replies,
        time: 100,
        directory,
    };
    let capabilities = app.request(json!({"operation":"start"}));
    let browser = app.window(true);
    let script = app.directory.join("terminal.sh");
    std::fs::write(&script, "printf 'Park terminal ready\\n'\ntouch terminal-ready\nwhile IFS= read -r line; do\nif [ -e clipboard-stage ]; then\nprintf '%s' \"$line\" > terminal-pasted\nprintf '\\033[2J\\033[3J\\033[HPark terminal clipboard'\nelse\nprintf '%s' \"$line\" > terminal-typed\nfi\ndone\n").unwrap();
    let mut application = crate::launch::Application::new(
        std::env::var_os("LINCE_PARK_TEST_TERMINAL").expect("Set LINCE_PARK_TEST_TERMINAL"),
    )
    .unwrap();
    let arguments: Vec<String> = std::env::var("LINCE_PARK_TEST_TERMINAL_ARGUMENTS")
        .map(|value| serde_json::from_str(&value).unwrap())
        .unwrap_or_default();
    for argument in arguments {
        application = application.arg(argument);
    }
    let mut terminal = application
        .arg("--config-default-files=false")
        .arg("--title=Park terminal")
        .arg("-e")
        .arg("bash")
        .arg("--noprofile")
        .arg("--norc")
        .arg(script)
        .current_dir(&app.directory)
        .command(&socket)
        .unwrap();
    let log = std::fs::File::create(app.directory.join("terminal.log")).unwrap();
    let _terminal = Process(
        terminal
            .process_group(0)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let mut results =
        json!({"task":"A2.5","browser":capabilities,"observations":[],"passed":false});
    for subject in [
        "native-typing",
        "extended-popup",
        "fullscreen-restore",
        "cross-application-clipboard",
        "file-drop",
    ] {
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match subject {
            "native-typing" => {
                app.activate(browser);
                let field = app.field();
                app.click(browser, field, 272);
                app.chord(&[29], 30);
                app.tap(30);
                app.tap(48);
                app.tap(46);
                app.value("abc");
                let terminal = app.window(false);
                app.fixture
                    .until(|_| app.directory.join("terminal-ready").exists());
                app.activate(terminal);
                app.tap(30);
                app.tap(48);
                app.tap(46);
                app.tap(28);
                app.fixture
                    .until(|_| app.directory.join("terminal-typed").exists());
                assert_eq!(
                    std::fs::read_to_string(app.directory.join("terminal-typed")).unwrap(),
                    "abc"
                );
                let frames = app.js("return document.querySelector('video').getVideoPlaybackQuality().totalVideoFrames");
                assert!(
                    frames.as_u64().is_some_and(|frames| frames > 0),
                    "The real video did not decode"
                );
                json!({"browser":"abc","terminal":"abc","decoded_video_frames":frames})
            }
            "extended-popup" => {
                app.activate(browser);
                for _ in 0..4 {
                    let width = app
                        .fixture
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
                        .find(|item| item.id == ItemId::Application(browser))
                        .unwrap()
                        .mapping
                        .canvas
                        .size()[0];
                    if width < 600.0 {
                        break;
                    }
                    app.fixture
                        .desktop
                        .send(Input::Action(DesktopAction::CycleItemWidth))
                        .unwrap();
                    app.fixture.inspect(|_| ());
                    app.js("return window.innerWidth");
                }
                app.js("Object.assign(document.querySelector('input').style,{position:'fixed',right:'8px',bottom:'8px',width:'80px'});return true");
                let field = app.field();
                app.click(browser, field, 272);
                let before = appearance::pixels(&mut app.fixture, "ordinary-before-popup");
                app.click(browser, field, 273);
                app.fixture
                    .until(|fixture| !fixture.desktop.popups.is_empty());
                let popup = app
                    .fixture
                    .desktop
                    .popups
                    .iter()
                    .find(|popup| popup.owner == renderer::PopupOwner::Application(browser))
                    .unwrap()
                    .clone();
                let parent = app
                    .fixture
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
                    .find(|item| item.id == ItemId::Application(browser))
                    .unwrap()
                    .clone();
                let deadline = Instant::now() + Duration::from_secs(10);
                let (changed, outside) = loop {
                    app.fixture.tick();
                    let pixels = appearance::pixels(&mut app.fixture, "ordinary-popup");
                    let mut changed = 0;
                    let mut outside = 0;
                    for (index, (before, after)) in before
                        .chunks_exact(4)
                        .zip(pixels.chunks_exact(4))
                        .enumerate()
                    {
                        let point = [(index % 1000) as f64 + 0.5, (index / 1000) as f64 + 0.5];
                        if popup.mapping.canvas.contains(point)
                            && before
                                .iter()
                                .zip(after)
                                .any(|(before, after)| before.abs_diff(*after) > 40)
                        {
                            changed += 1;
                            if parent.surface_point(point).is_none() {
                                outside += 1;
                            }
                        }
                    }
                    if changed > 200 && outside > 100 {
                        break (changed, outside);
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Popup pixels missing: changed={changed}, outside={outside}, popup={:?}, parent={:?}",
                        popup.mapping,
                        parent.mapping
                    );
                };
                app.tap(108);
                app.tap(28);
                app.fixture
                    .until(|fixture| fixture.desktop.popups.is_empty());
                app.js("document.querySelector('input').removeAttribute('style');return true");
                json!({"popup":format!("{:?}",popup.mapping),"changed_pixels":changed,"outside_parent_pixels":outside,"keyboard_selection":"passed"})
            }
            "fullscreen-restore" => {
                let original = app.rectangle(browser);
                let slot = app
                    .fixture
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
                    .find(|item| item.id == ItemId::Application(browser))
                    .unwrap()
                    .mapping
                    .canvas;
                let full = app
                    .request(json!({"operation":"command","path":"/window/fullscreen","value":{}}));
                assert_eq!(app.js("return window.fullScreen"), true);
                app.request(json!({"operation":"command","path":"/window/rect","value":{"width":720,"height":480}}));
                let restored = app.rectangle(browser);
                assert_eq!(app.js("return window.fullScreen"), false);
                let restored_slot = app
                    .fixture
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
                    .find(|item| item.id == ItemId::Application(browser))
                    .unwrap()
                    .mapping
                    .canvas;
                assert_eq!(
                    restored_slot, slot,
                    "Leaving fullscreen changed the compositor-owned tile"
                );
                json!({"original":original,"fullscreen":full,"restored":restored,"tile_before":format!("{slot:?}"),"tile_after":format!("{restored_slot:?}"),"policy":"Restore the compositor-owned slot; the client may change its decorations and buffer dimensions"})
            }
            "cross-application-clipboard" => {
                app.activate(browser);
                let field = app.field();
                app.click(browser, field, 272);
                app.chord(&[29], 30);
                app.tap(30);
                app.tap(48);
                app.tap(46);
                app.value("abc");
                app.chord(&[29], 30);
                let selection = app.js("const i=document.querySelector('input');return {active:document.activeElement.tagName,value:i.value,start:i.selectionStart,end:i.selectionEnd,trace:window.pointerTrace,inner:[innerWidth,innerHeight],outer:[outerWidth,outerHeight],offset:[mozInnerScreenX-screenX,mozInnerScreenY-screenY]}");
                eprintln!(
                    "Browser selection: {selection}; bounds={:?}; mapping={:?}",
                    app.fixture
                        .desktop
                        .server
                        .windows()
                        .iter()
                        .find(|window| window.id == browser)
                        .unwrap()
                        .bounds(),
                    app.fixture
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
                        .find(|item| item.id == ItemId::Application(browser))
                        .unwrap()
                        .mapping
                );
                assert_eq!(
                    selection["active"], "INPUT",
                    "The native click missed the browser field: {selection}"
                );
                assert_eq!(
                    selection["start"], 0,
                    "The selection did not start at the beginning: {selection}"
                );
                assert_eq!(
                    selection["end"], 3,
                    "The browser did not select its native typed text: {selection}"
                );
                app.chord(&[29], 46);
                let terminal = app.window(false);
                app.fixture
                    .until(|_| app.directory.join("terminal-ready").exists());
                std::fs::write(app.directory.join("clipboard-stage"), []).unwrap();
                app.activate(terminal);
                app.chord(&[29], 22);
                app.chord(&[29, 42], 47);
                app.tap(28);
                app.fixture
                    .until(|_| app.directory.join("terminal-pasted").exists());
                assert_eq!(
                    std::fs::read_to_string(app.directory.join("terminal-pasted")).unwrap(),
                    "abc"
                );
                app.chord(&[29, 42], 30);
                app.chord(&[29, 42], 46);
                app.activate(browser);
                let field = app.field();
                app.click(browser, field, 272);
                app.chord(&[29], 30);
                app.chord(&[29], 47);
                app.value("Park terminal clipboard");
                json!({"browser_to_terminal":"abc","terminal_to_browser":"Park terminal clipboard"})
            }
            "file-drop" => {
                app.activate(browser);
                app.js("window.allDragTrace=[];for(const type of ['dragenter','dragover','drop'])document.addEventListener(type,e=>allDragTrace.push({type:e.type,target:e.target.tagName,id:e.target.id,types:[...e.dataTransfer.types]}));return true");
                let point = app.js("const r=drop.getBoundingClientRect();return [r.x+r.width/2+window.mozInnerScreenX-window.screenX,r.y+r.height/2+window.mozInnerScreenY-window.screenY]");
                let point = app.point(
                    browser,
                    [point[0].as_f64().unwrap(), point[1].as_f64().unwrap()],
                );
                let path = app.directory.join("ordinary-drop.txt");
                std::fs::write(&path, "Park real file drop").unwrap();
                let surface = app
                    .fixture
                    .desktop
                    .server
                    .windows()
                    .into_iter()
                    .find(|window| window.id == source_id)
                    .unwrap()
                    .surface;
                app.fixture.desktop.server.focus_surface(
                    Some(surface.clone()),
                    smithay::utils::SERIAL_COUNTER.next_serial(),
                );
                app.fixture
                    .desktop
                    .server
                    .pointer_motion(Some((surface, [20.0, 20.0])), 300)
                    .unwrap();
                app.fixture.desktop.server.pointer_button(272, true, 301);
                app.fixture.exchange();
                let qh = app.fixture.queue.handle();
                let manager = app.fixture.client.data_manager.as_ref().unwrap();
                let device =
                    manager.get_data_device(app.fixture.client.seat.as_ref().unwrap(), &qh, ());
                let source = manager.create_data_source(&qh, ());
                source.offer("text/uri-list".into());
                source.set_actions(wl_data_device_manager::DndAction::Copy);
                app.fixture.client.drop_payload =
                    format!("file://{}\r\n", path.display()).into_bytes();
                device.start_drag(
                    Some(&source),
                    &source_window.surface,
                    None,
                    app.fixture.client.pointer_serial,
                );
                app.fixture.exchange();
                assert!(
                    app.fixture.desktop.server.dragging(),
                    "The file source did not establish a real drag"
                );
                app.fixture.desktop.move_pointer(point, 302).unwrap();
                app.fixture.inspect(|_| ());
                assert!(
                    matches!(app.fixture.desktop.target(), crate::desktop::input::Target::Application(id, _, _) if id == browser),
                    "The drop did not hit the browser at {point:?}"
                );
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut motion = 304;
                loop {
                    app.fixture
                        .desktop
                        .move_pointer(
                            [
                                point[0] + if motion % 2 == 0 { 1.0 } else { -1.0 },
                                point[1],
                            ],
                            motion,
                        )
                        .unwrap();
                    motion += 1;
                    if app.js("return window.dragEvents.includes('dragover')") == true {
                        if app.fixture.client.drop_action
                            == Some(wl_data_device_manager::DndAction::Copy.bits())
                        {
                            break;
                        }
                    }
                    let action = app.fixture.client.drop_action;
                    assert!(
                        Instant::now() < deadline,
                        "The browser did not negotiate the native copy: action={:?}, events={:?}",
                        action,
                        app.js("return {drop:window.dragEvents,all:window.allDragTrace}")
                    );
                }
                app.fixture.desktop.button(272, false, motion).unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                let dropped = loop {
                    let dropped = app.js("return window.dropped");
                    if !dropped.is_null() {
                        break dropped;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Firefox did not receive the real Wayland file drop"
                    );
                };
                assert_eq!(
                    dropped,
                    json!([{"name":"ordinary-drop.txt","text":"Park real file drop"}])
                );
                device.release();
                source.destroy();
                dropped
            }
            _ => unreachable!(),
        }));
        let observation = match outcome {
            Ok(evidence) => {
                json!({"subject":subject,"passed":true,"evidence":evidence,"seconds":started.elapsed().as_secs_f64()})
            }
            Err(error) => {
                json!({"subject":subject,"passed":false,"error":error.downcast_ref::<String>().cloned().or_else(|| error.downcast_ref::<&str>().map(|value|value.to_string())).unwrap_or_else(||"panic".into()),"seconds":started.elapsed().as_secs_f64()})
            }
        };
        eprintln!("Ordinary workflow: {observation}");
        results["observations"]
            .as_array_mut()
            .unwrap()
            .push(observation);
        std::fs::write(
            app.directory.join("ordinary-results.json"),
            serde_json::to_vec_pretty(&results).unwrap(),
        )
        .unwrap();
        app.activate(browser);
        app.tap(1);
    }
    results["passed"] = json!(
        results["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|observation| observation["passed"] == true)
    );
    std::fs::write(
        app.directory.join("ordinary-results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    assert_eq!(
        results["passed"], true,
        "Ordinary application workflow failed: {results}"
    );
}
