use super::*;
use bevy::{ecs::system::RunSystemOnce, input::InputPlugin, text::{EditableText, FontCx, LayoutCx}};
use native::{actions::{ActionButton, ActionSequence, ActionsPlugin}, workspace::Workspaces};
use std::{collections::BTreeSet, fs, os::unix::{fs::PermissionsExt, process::CommandExt}, path::{Path, PathBuf}, process::{Child, Command}, thread, time::{Duration, Instant}};
use zbus::{blocking::{Connection, Proxy, connection::Builder}, zvariant::{OwnedObjectPath, OwnedValue}};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(crate) type Object = (String, OwnedObjectPath);
type Announcement = (String, i32, i32, OwnedValue, BTreeMap<String, OwnedValue>);

const CHILD: &str = "LINCE_PARK_ATSPI_FIXTURE_ROOT";
const TEST: &str = "presentation::accessibility::platform::private_platform_adapter_discovers_controls_and_round_trips_actions_and_removal";
const ERROR: &str = "Use a factor between 1 and 4.";

#[cfg(feature = "nested")]
mod dispatcher;

struct Process { child: Child, group: bool }

impl Process {
    fn wait(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(status) = self.child.try_wait()? {
                return if status.success() { Ok(()) } else { Err(format!("The private accessibility fixture exited with {status}.").into()) };
            }
            if Instant::now() >= deadline { return Err("The private accessibility fixture exceeded its deadline.".into()); }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.group {
            let _ = rustix::process::kill_process_group(rustix::process::Pid::from_child(&self.child), rustix::process::Signal::KILL);
        } else { let _ = self.child.kill(); }
        let _ = self.child.wait();
    }
}

fn encoded_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().iter().map(|byte| {
        if byte.is_ascii_alphanumeric() || b"/_-.".contains(byte) { char::from(*byte).to_string() } else { format!("%{byte:02X}") }
    }).collect()
}

fn isolated(test: &str) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    for name in ["run", "config", "cache", "data", "state"] {
        let path = root.join(name);
        fs::create_dir(&path)?; fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    let config = root.join("session.conf");
    fs::write(&config, format!("<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><standard_session_servicedirs/><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>", encoded_path(&root.join("run/session-bus"))))?;
    let mut command = Command::new("dbus-run-session");
    command.arg("--config-file").arg(config).arg("--").arg(std::env::current_exe()?)
        .args(["--exact", test, "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD, root).env("XDG_RUNTIME_DIR", root.join("run"))
        .env("XDG_CONFIG_HOME", root.join("config")).env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data")).env("XDG_STATE_HOME", root.join("state"))
        .env_remove("AT_SPI_BUS_ADDRESS").env_remove("DBUS_STARTER_ADDRESS").env_remove("DBUS_STARTER_BUS_TYPE")
        .env_remove("DISPLAY").env_remove("XAUTHORITY");
    let transcript = root.join("provider.log");
    let output = fs::File::create(&transcript)?;
    command.stdout(output.try_clone()?).stderr(output);
    command.process_group(0);
    let mut process = Process { child: command.spawn()?, group: true };
    let result = process.wait();
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)?;
        fs::copy(&transcript, directory.join(format!("atspi-{}.log", test.rsplit("::").next().unwrap())))?;
    }
    if result.is_err() { eprintln!("{}", fs::read_to_string(&transcript).unwrap_or_else(|error| error.to_string())); }
    result?;
    if fs::read_to_string(root.join("complete")).ok().as_deref() != Some(test) { return Err("The private adapter child did not complete the exact fixture.".into()); }
    Ok(())
}

pub(crate) fn run(test: &str, task: impl FnOnce(PathBuf) -> Result<()>) -> Result<()> {
    match std::env::var_os(CHILD) {
        Some(root) => { let root = PathBuf::from(root); task(root.clone())?; fs::write(root.join("complete"), test)?; Ok(()) }
        None => isolated(test),
    }
}

pub(crate) fn proxy(bus: &Connection, object: &Object, interface: &str) -> zbus::Result<Proxy<'static>> {
    Proxy::new_owned(bus.clone(), object.0.clone(), object.1.clone(), interface.to_owned())
}

fn connection(address: Option<&str>) -> zbus::Result<Connection> {
    let builder = match address { Some(address) => Builder::address(address)?, None => Builder::session()? };
    builder.method_timeout(Duration::from_secs(2)).build()
}

pub(crate) fn wait<T>(mut attempt: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(value) = attempt()? { return Ok(value); }
        if Instant::now() >= deadline { return Err("The private accessibility observation did not arrive.".into()); }
        thread::sleep(Duration::from_millis(5));
    }
}

fn named(bus: &Connection, label: &str) -> zbus::Result<Option<Object>> {
    named_role(bus, label, None)
}

pub(crate) fn named_role(bus: &Connection, label: &str, role: Option<atspi_common::Role>) -> zbus::Result<Option<Object>> {
    let root = ("org.a11y.atspi.Registry".to_owned(), OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")?);
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(object) = pending.pop() {
        if !seen.insert(format!("{}:{}", object.0, object.1)) { continue; }
        if seen.len() > 128 { return Err(zbus::Error::Failure("The private accessibility tree exceeds its fixture bound.".into())); }
        let node = proxy(bus, &object, "org.a11y.atspi.Accessible")?;
        if node.get_property::<String>("Name").ok().as_deref() == Some(label)
            && role.is_none_or(|role| node.call::<_, _, atspi_common::Role>("GetRole", &()).ok() == Some(role)) { return Ok(Some(object)); }
        if let Ok(children) = node.call::<_, _, Vec<Object>>("GetChildren", &()) { pending.extend(children); }
    }
    Ok(None)
}

#[derive(Component, Default)]
struct Clicks(usize);

struct Validate { field: Entity, status: Entity }

impl native::actions::Action for Validate {
    fn apply(&self, world: &mut World, target: Entity) {
        world.get_mut::<Clicks>(target).unwrap().0 += 1;
        let mut input = world.get_mut::<EditableText>(self.field).unwrap();
        input.queue_edit(bevy::text::TextEdit::SelectAll);
        input.queue_edit(bevy::text::TextEdit::Insert("aé界".into()));
        world.get_mut::<bevy::a11y::AccessibilityNode>(self.status).unwrap().set_label(ERROR);
    }
}

struct Fixture {
    app: App,
    bridge: Option<Bridge>,
    views: Vec<crate::native_input::projection::View>,
    root: Entity,
    button: Entity,
    field: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        native::laboratory::isolate(app.world_mut());
        app.add_plugins((InputPlugin, ActionsPlugin)).insert_resource(WakeSignal::new(|| {}));
        app.init_resource::<FontCx>().init_resource::<LayoutCx>().init_resource::<bevy::clipboard::Clipboard>();
        let font = Font::from_bytes(include_bytes!("../../../../../institute/assets/fonts/Lato/Lato-Regular.ttf").to_vec());
        app.world_mut().resource_mut::<FontCx>().collection.register_fonts(font.data, None);
        app.world_mut().resource_mut::<FontCx>().set_sans_serif_family("Lato").unwrap();
        let root = app.world_mut().spawn((Node::default(), Workspaces::default(), Clicks::default(), InheritedVisibility::VISIBLE)).id();
        let mut field = accesskit::Node::new(accesskit::Role::TextInput); field.set_label("Platform fixture value");
        let field = app.world_mut().spawn((Node::default(), EditableText::new("draft"), bevy::a11y::AccessibilityNode(field), InheritedVisibility::VISIBLE, ChildOf(root))).id();
        let mut status = accesskit::Node::new(accesskit::Role::Status); status.set_label("Ready"); status.set_live(accesskit::Live::Polite);
        let status = app.world_mut().spawn((Node::default(), bevy::a11y::AccessibilityNode(status), InheritedVisibility::VISIBLE, ChildOf(root))).id();
        let mut button = accesskit::Node::new(accesskit::Role::Button); button.set_label("Validate fixture value");
        let button = app.world_mut().spawn((Node::default(), bevy::a11y::AccessibilityNode(button), InheritedVisibility::VISIBLE,
            ActionButton::new(root, ActionSequence::default().then(Validate { field, status })), ChildOf(root))).id();
        let mut secret = accesskit::Node::new(accesskit::Role::TextInput); secret.set_label("Platform fixture secret");
        let secret = app.world_mut().spawn((Node::default(), EditableText::new("must-never-be-exported"), bevy::a11y::AccessibilityNode(secret), InheritedVisibility::VISIBLE, ChildOf(root))).id();
        native::accessibility::secret(app.world_mut(), secret);
        app.world_mut().run_system_once(bevy::text::apply_text_edits).unwrap();
        let bridge = Bridge::new(app.world_mut());
        let views = vec![crate::native_input::projection::View { root: Some(root), output: lince_interface::host::OutputId(1),
            image: Handle::<Image>::default().id(), raster: Default::default(), origin: [0.0; 2], viewport: None }];
        Self { app, bridge: Some(bridge), views, root, button, field }
    }

    fn pump(&mut self) {
        let Some(bridge) = &mut self.bridge else { return; };
        for request in bridge.requests(self.app.world(), &self.views) {
            if native::accessibility::context(self.app.world(), request.target.entity, true).as_ref() == Some(&request.target.context) {
                native::accessibility::perform(self.app.world_mut(), request.target.entity, &request.target.context, &request.native_action()).unwrap();
            }
        }
        self.app.update();
        self.app.world_mut().run_system_once(bevy::text::apply_text_edits).unwrap();
        bridge.publish(self.app.world_mut(), &self.views, None);
        if let Some(receipt) = bridge.receipt() { for view in &self.views { receipt.presented(view.output); } }
        bridge.flush(self.app.world(), &self.views);
    }

    fn clicks(&self) -> usize { self.app.world().get::<Clicks>(self.root).unwrap().0 }
}

pub(crate) struct Provider { pub(crate) status: Proxy<'static>, pub(crate) bus: Connection, _registry: Process, _launcher: Process }

impl Provider {
    pub(crate) fn new(root: &Path) -> Result<Self> {
    let runtime = root.join("run");
    assert_eq!(std::env::var_os("XDG_RUNTIME_DIR"), Some(runtime.clone().into_os_string()));
    assert_eq!(fs::metadata(&runtime)?.permissions().mode() & 0o777, 0o700);
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    assert!(address.starts_with(&format!("unix:path={}", encoded_path(&runtime.join("session-bus")))));
    assert!(std::env::var_os("AT_SPI_BUS_ADDRESS").is_none());
    let launcher = std::env::var_os("LINCE_PARK_ATSPI_BUS_LAUNCHER").ok_or("Set LINCE_PARK_ATSPI_BUS_LAUNCHER to the selected at-spi-bus-launcher executable.")?;
    let registry = Path::new(&launcher).with_file_name("at-spi2-registryd");
    let launcher = Process { child: Command::new(launcher).arg("--launch-immediately").spawn()?, group: false };
    let session = connection(None)?;
    let owner = ("org.a11y.Bus".to_owned(), OwnedObjectPath::try_from("/org/a11y/bus")?);
    let address: String = wait(|| Ok(proxy(&session, &owner, "org.a11y.Bus").ok().and_then(|proxy| proxy.call("GetAddress", &()).ok())))?;
    assert!(address.starts_with(&format!("unix:path={}", encoded_path(&runtime))));
    let status = proxy(&session, &owner, "org.a11y.Status")?;
    status.set_property("ScreenReaderEnabled", false)?;
    let bus = connection(Some(&address))?;
    let registry = Process { child: Command::new(registry).env("AT_SPI_BUS_ADDRESS", &address)
        .env("DBUS_SESSION_BUS_ADDRESS", &address).spawn()?, group: false };
    let daemon = proxy(&bus, &("org.freedesktop.DBus".into(), OwnedObjectPath::try_from("/org/freedesktop/DBus")?), "org.freedesktop.DBus")?;
    wait(|| Ok(daemon.call::<_, _, bool>("NameHasOwner", &("org.a11y.atspi.Registry",))?.then_some(())))?;
    Ok(Self { status, bus, _registry: registry, _launcher: launcher })
    }
}

fn child(root: PathBuf) -> Result<()> {
    let provider = Provider::new(&root)?;
    let bus = &provider.bus;
    let status = &provider.status;
    let mut fixture = Fixture::new();
    status.set_property("ScreenReaderEnabled", true)?;
    let field = wait(|| { fixture.pump(); Ok(named(&bus, "Platform fixture value")?) })?;
    let button = wait(|| { fixture.pump(); Ok(named(&bus, "Validate fixture value")?) })?;
    let feedback = wait(|| { fixture.pump(); Ok(named(&bus, "Ready")?) })?;
    let secret = wait(|| { fixture.pump(); Ok(named(&bus, "Platform fixture secret")?) })?;
    let children: Vec<Object> = proxy(&bus, &secret, "org.a11y.atspi.Accessible")?.call("GetChildren", &())?;
    assert!(children.is_empty());
    if let Ok(text) = proxy(&bus, &secret, "org.a11y.atspi.Text")?.call::<_, _, String>("GetText", &(0_i32, -1_i32)) {
        assert!(!text.contains("must-never-be-exported"));
    }
    let reader = proxy(&bus, &field, "org.a11y.atspi.Text")?;
    assert_eq!(reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, "draft");
    assert!(reader.call::<_, _, bool>("SetCaretOffset", &(1_i32,))?);
    wait(|| { fixture.pump(); Ok((fixture.app.world().resource::<bevy::input_focus::InputFocus>().get() == Some(fixture.field)).then_some(())) })?;
    let event = proxy(&bus, &feedback, "org.a11y.atspi.Event.Object")?;
    let mut events = event.receive_signal("Announcement")?;
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let listener = thread::spawn(move || {
        for event in &mut events {
            let body = match event.body().deserialize::<Announcement>() {
                Ok(body) => body,
                Err(error) => { let _ = sent.send(Err(error.to_string())); return; }
            };
            if String::try_from(body.3).ok().as_deref() == Some(ERROR) { let _ = sent.send(Ok(())); return; }
        }
        let _ = sent.send(Err("The private announcement subscription ended early.".to_owned()));
    });
    let action = proxy(&bus, &button, "org.a11y.atspi.Action")?;
    assert!(action.call::<_, _, bool>("DoAction", &(0_i32,))?);
    wait(|| { fixture.pump(); Ok((fixture.clicks() == 1).then_some(())) })?;
    wait(|| { fixture.pump(); Ok((reader.call::<_, _, String>("GetText", &(0_i32, -1_i32))? == "aé界").then_some(())) })?;
    wait(|| {
        fixture.pump();
        match received.try_recv() {
            Ok(Ok(())) => Ok(Some(())),
            Ok(Err(error)) => Err(error.into()),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("The announcement observer stopped before the expected event.".into()),
        }
    })?;
    listener.join().unwrap();
    assert!(reader.call::<_, _, bool>("SetSelection", &(0_i32, 1_i32, 3_i32))?);
    wait(|| {
        fixture.pump();
        Ok((fixture.app.world().get::<EditableText>(fixture.field).unwrap().editor.raw_selection().text_range() == (1..6)).then_some(()))
    })?;
    assert!(action.call::<_, _, bool>("DoAction", &(0_i32,))?);
    fixture.app.world_mut().despawn(fixture.button);
    wait(|| { fixture.pump(); Ok(named(&bus, "Validate fixture value")?.is_none().then_some(())) })?;
    assert_eq!(fixture.clicks(), 1);
    assert!(action.call::<_, _, bool>("DoAction", &(0_i32,)).is_err());
    status.set_property("ScreenReaderEnabled", false)?;
    wait(|| { fixture.pump(); Ok((!fixture.bridge.as_ref().unwrap().shared.requested.get()).then_some(())) })?;
    assert!(fixture.bridge.as_ref().unwrap().shared.catalog.lock().unwrap().targets.is_empty());
    status.set_property("ScreenReaderEnabled", true)?;
    let returned = wait(|| { fixture.pump(); Ok(named(&bus, "Platform fixture value")?) })?;
    assert_ne!(returned, field);
    assert_eq!(proxy(&bus, &returned, "org.a11y.atspi.Text")?.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, "aé界");
    drop(fixture.bridge.take());
    wait(|| Ok(named(&bus, "Lince Park")?.is_none().then_some(())))?;
    Ok(())
}

#[test]
#[ignore = "requires the private D-Bus and AT-SPI providers from the interface development shell"]
fn private_platform_adapter_discovers_controls_and_round_trips_actions_and_removal() -> Result<()> {
    run(TEST, child)
}
