use super::*;
use std::{os::fd::AsFd, time::Duration};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_region,
        wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_list_v1, ext_foreign_toplevel_handle_v1,
};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1, wp_fractional_scale_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewporter, wp_viewport};
use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notifier_v1, ext_idle_notification_v1};
use wayland_protocols::ext::workspace::v1::client::{ext_workspace_manager_v1, ext_workspace_handle_v1, ext_workspace_group_handle_v1};
use wayland_protocols::wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{zwlr_foreign_toplevel_manager_v1, zwlr_foreign_toplevel_handle_v1};
use wayland_protocols_misc::zwp_input_method_v2::client::{zwp_input_method_manager_v2, zwp_input_method_v2, zwp_input_popup_surface_v2, zwp_input_method_keyboard_grab_v2};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1};

#[path = "tests/output_control.rs"]
mod output_control_tests;
#[path = "tests/text_input.rs"]
mod text_input_tests;
#[path = "tests/power.rs"]
mod power_tests;
#[path = "tests/touch.rs"]
mod touch_tests;
#[path = "tests/tablet.rs"]
mod tablet_tests;
#[path = "tests/displays.rs"]
mod display_tests;
#[path = "tests/color.rs"]
mod color_tests;
#[path = "tests/dmabuf.rs"]
mod dmabuf_tests;
#[path = "tests/capture.rs"]
mod capture_tests;
#[path = "tests/popup_output.rs"]
mod popup_output_tests;
#[path = "tests/input_controls.rs"]
mod input_control_tests;
#[path = "tests/gestures.rs"]
mod gesture_tests;
#[path = "tests/activation.rs"]
mod activation_tests;
#[path = "tests/control.rs"]
mod control_tests;
#[cfg(feature = "nested")]
#[path = "tests/manipulation.rs"]
mod manipulation_tests;
#[cfg(feature = "nested")]
#[path = "tests/keyboard.rs"]
mod keyboard_tests;
#[cfg(feature = "nested")]
#[path = "tests/native_ime.rs"]
mod native_ime_tests;
#[cfg(feature = "nested")]
#[path = "tests/configuration.rs"]
mod configuration_tests;
#[cfg(feature = "nested")]
#[path = "tests/file_hover.rs"]
mod file_hover_tests;
#[path = "tests/idle_output.rs"]
mod idle_output_tests;
#[cfg(feature = "native-presentation")]
#[path = "tests/focus.rs"]
mod focus_tests;
#[cfg(feature = "native-presentation")]
#[path = "tests/transfers.rs"]
mod transfers_tests;
#[cfg(feature = "native-presentation")]
#[path = "tests/raster.rs"]
mod raster_tests;

#[derive(Default)]
struct Client {
    pointer: Option<wl_pointer::WlPointer>,
    tablet: tablet_tests::Capture,
    touch: touch_tests::Capture,
    color: color_tests::Capture,
    dmabuf: dmabuf_tests::Capture,
    control: control_tests::Capture,
    viewporter: Option<wp_viewporter::WpViewporter>,
    controls: input_control_tests::Controls,
    gestures: gesture_tests::Capture,
    capture: capture_tests::Capture,
    foreign_handles: Vec<ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1>,
    foreign_names: std::collections::HashMap<wayland_client::backend::ObjectId, String>,
    output_control: output_control_tests::Capture,
    workspace_manager: Option<ext_workspace_manager_v1::ExtWorkspaceManagerV1>,
    workspaces: Vec<ext_workspace_handle_v1::ExtWorkspaceHandleV1>,
    workspace_names: Vec<String>,
    workspace_name_events: Vec<(ext_workspace_handle_v1::ExtWorkspaceHandleV1, String)>,
    workspace_coordinates: Vec<(ext_workspace_handle_v1::ExtWorkspaceHandleV1, Vec<u32>)>,
    workspace_done: usize,
    workspace_removed: usize,
    workspace_groups: Vec<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1>,
    workspace_output_enters: usize,
    workspace_output_leaves: usize,
    workspace_active: Vec<bool>,
    management: Option<zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1>,
    managed: Vec<zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1>,
    management_done: usize,
    management_closed: usize,
    management_minimized: Vec<bool>,
    data_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    drop_payload: Vec<u8>,
    drop_finished: usize,
    drop_performed: usize,
    drop_cancelled: usize,
    drop_send_failed: bool,
    drop_hold: bool,
    drop_held: Vec<OwnedFd>,
    input_method: Option<zwp_input_method_manager_v2::ZwpInputMethodManagerV2>,
    text_input: text_input_tests::Capture,
    input_unavailable: Vec<wayland_client::backend::ObjectId>,
    virtual_keyboard: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    input_keymap: Option<(OwnedFd, u32)>,
    input_keys: Vec<(u32, bool)>,
    input_modifiers: Vec<[u32; 4]>,
    input_serial: u32,
    input_surrounding: Vec<(String, u32, u32)>,
    input_causes: Vec<u32>,
    input_active: bool,
    idle: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    idle_inhibit: Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>,
    idle_events: Vec<bool>,
    power: power_tests::Capture,
    globals: Vec<String>,
    foreign: Option<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1>,
    foreign_titles: Vec<String>,
    foreign_closed: usize,
    fractional: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    preferred_scales: Vec<u32>,
    compositor: Option<wl_compositor::WlCompositor>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    layer_configures: Vec<[u32; 2]>,
    configures: Vec<[i32; 2]>,
    window_configurations: std::collections::HashMap<wayland_client::backend::ObjectId, ([i32; 2], Vec<u32>)>,
    window_configure_sizes: std::collections::HashMap<wayland_client::backend::ObjectId, Vec<[i32; 2]>>,
    output: Option<wl_output::WlOutput>,
    outputs: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    output_name: String,
    output_size: [i32; 2],
    window_states: Vec<Vec<u32>>,
    serials: usize,
    surface_serials: std::collections::HashMap<wayland_client::backend::ObjectId, u32>,
    releases: usize,
    closed: bool,
    ignore_configure: bool,
    decorations: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    decoration_modes: Vec<zxdg_toplevel_decoration_v1::Mode>,
    keys: Vec<(u32, bool)>,
    keyboard_focus: Option<wl_surface::WlSurface>,
    key_recipients: Vec<(Option<wayland_client::backend::ObjectId>, u32, bool)>,
    modifiers: Vec<[u32; 4]>,
    keymaps: Vec<String>,
    repeat_info: Vec<(i32, i32)>,
    seat: Option<wl_seat::WlSeat>,
    key_serial: u32,
    pointer_serial: u32,
    pointer_enter_serial: u32,
    pointer_buttons: Vec<(u32, bool)>,
    pointer_focus: Option<wl_surface::WlSurface>,
    button_recipients: Vec<(Option<wayland_client::backend::ObjectId>, u32, bool)>,
    pointer_positions: Vec<[f64; 2]>,
    popup_configures: Vec<[i32; 4]>,
    popup_repositions: Vec<u32>,
    popup_done: usize,
    activation: Option<xdg_activation_v1::XdgActivationV1>,
    tokens: Vec<String>,
    lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    lock_replies: Vec<bool>,
    lock_sizes: Vec<[u32; 2]>,
}

struct Harness {
    server: Server,
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
}

struct Window {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    role: xdg_toplevel::XdgToplevel,
}

impl Harness {
    fn new() -> Self {
        Self::connect(Server::new([640, 480]).unwrap())
    }

    fn connect(server: Server) -> Self {
        Self::connect_authorized(server, None)
    }

    fn connect_authorized(mut server: Server, capabilities: Option<Vec<crate::launch::Capability>>) -> Self {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        if let Some(capabilities) = capabilities {
            server.insert_authorized_client(server_stream, capabilities, false).unwrap();
        } else {
            server.insert_client(server_stream).unwrap();
        }
        let connection = Connection::from_socket(client_stream).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut harness = Self {
            server,
            connection,
            queue,
            client: Client::default(),
        };
        harness.sync();
        harness.sync();
        harness
    }

    fn sync(&mut self) {
        synchronize(
            &mut self.server,
            &self.connection,
            &mut self.queue,
            &mut self.client,
        )
        .unwrap();
    }

    fn window(&mut self) -> Window {
        let qh = self.queue.handle();
        let surface = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let xdg = self
            .client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let role = xdg.get_toplevel(&qh, ());
        role.set_title("A real Wayland window".into());
        role.set_app_id("social.lince.park.test".into());
        role.set_min_size(120, 80);
        role.set_max_size(1200, 900);
        surface.attach(None, 0, 0);
        surface.commit();
        self.sync();
        self.sync();
        Window { surface, xdg, role }
    }

    fn attach(&mut self, window: &Window, size: [i32; 2]) -> wl_buffer::WlBuffer {
        let buffer = self.buffer(size);
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0], size[1]);
        window.surface.commit();
        self.sync();
        buffer
    }

    fn buffer(&self, size: [i32; 2]) -> wl_buffer::WlBuffer {
        let file = tempfile::tempfile().unwrap();
        file.set_len((size[0] * size[1] * 4) as u64).unwrap();
        let qh = self.queue.handle();
        let pool = self.client.shm.as_ref().unwrap().create_pool(
            file.as_fd(),
            size[0] * size[1] * 4,
            &qh,
            (),
        );
        let buffer = pool.create_buffer(
            0,
            size[0],
            size[1],
            size[0] * 4,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        pool.destroy();
        buffer
    }
}

fn synchronize(
    server: &mut Server,
    connection: &Connection,
    queue: &mut EventQueue<Client>,
    client: &mut Client,
) -> Result<(), String> {
    let done = Arc::new(AtomicBool::new(false));
    connection.display().sync(&queue.handle(), done.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return Err("the Wayland client timed out".into());
        }
        if let Err(error) = connection.flush()
            && !matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == std::io::ErrorKind::WouldBlock) {
            return Err(error.to_string());
        }
        server.dispatch().map_err(|error| error.to_string())?;
        queue
            .dispatch_pending(client)
            .map_err(|error| error.to_string())?;
        if let Some(read) = queue.prepare_read() {
            let fd = read.connection_fd();
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let timeout = rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: 1_000_000,
            };
            rustix::event::poll(&mut poll, Some(&timeout)).map_err(|error| error.to_string())?;
            if poll[0].revents().contains(rustix::event::PollFlags::IN) {
                match read.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
        }
        queue
            .dispatch_pending(client)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.globals.push(interface.clone());
            match interface.as_str() {
                "lince_park_control_v1" => state.control.manager = Some(registry.bind(name, version.min(1), qh, ())),
                "zwp_pointer_gestures_v1" => gesture_tests::bind(state, registry, name, version, qh),
                "zwp_pointer_constraints_v1" | "zwp_relative_pointer_manager_v1" | "zwp_keyboard_shortcuts_inhibit_manager_v1" =>
                    input_control_tests::bind(state, registry, name, &interface, qh),
                "ext_output_image_capture_source_manager_v1" | "ext_foreign_toplevel_image_capture_source_manager_v1" | "ext_image_copy_capture_manager_v1" => capture_tests::bind(state, registry, name, &interface, qh),
                "zwlr_output_manager_v1" => output_control_tests::bind(state, registry, name, version, qh),
                "zwlr_output_power_manager_v1" => power_tests::bind(state, registry, name, version, qh),
                "zwp_tablet_manager_v2" => tablet_tests::bind(state, registry, name, version, qh),
                "wp_color_manager_v1" => color_tests::bind(state, registry, name, version, qh),
                "zwp_linux_dmabuf_v1" => dmabuf_tests::bind(state, registry, name, version, qh),
                "ext_workspace_manager_v1" => state.workspace_manager = Some(registry.bind(name, 1, qh, ())),
                "zwlr_foreign_toplevel_manager_v1" => state.management = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_data_device_manager" => state.data_manager = Some(registry.bind(name, version.min(3), qh, ())),
                "zwp_input_method_manager_v2" => state.input_method = Some(registry.bind(name, 1, qh, ())),
                "zwp_text_input_manager_v3" => text_input_tests::bind(state, registry, name, qh),
                "zwp_virtual_keyboard_manager_v1" => state.virtual_keyboard = Some(registry.bind(name, 1, qh, ())),
                "ext_idle_notifier_v1" => state.idle = Some(registry.bind(name, version.min(2), qh, ())),
                "zwp_idle_inhibit_manager_v1" => state.idle_inhibit = Some(registry.bind(name, 1, qh, ())),
                "wp_fractional_scale_manager_v1" => state.fractional = Some(registry.bind(name, 1, qh, ())),
                "wp_viewporter" => state.viewporter = Some(registry.bind(name, 1, qh, ())),
                "ext_foreign_toplevel_list_v1" => state.foreign = Some(registry.bind(name, 1, qh, ())),
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), qh, ())),
                "xdg_activation_v1" => state.activation = Some(registry.bind(name, 1, qh, ())),
                "ext_session_lock_manager_v1" => {
                    state.lock_manager = Some(registry.bind(name, 1, qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), qh, ());
                    seat.get_keyboard(qh, ());
                    state.pointer = Some(seat.get_pointer(qh, ()));
                    state.seat = Some(seat);
                }
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "zxdg_decoration_manager_v1" => {
                    state.decorations = Some(registry.bind(name, 1, qh, ()))
                }
                "wl_output" => state.output = Some(registry.bind(name, version.min(4), qh, ())),
                "zxdg_output_manager_v1" => {
                    state.outputs = Some(registry.bind(name, version.min(3), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, ()> for Client {
    fn event(
        state: &mut Self, _: &ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event { state.foreign_handles.push(toplevel); }
    }

    wayland_client::event_created_child!(Client, ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, [
        0 => (ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ()> for Client {
    fn event(
        state: &mut Self, handle: &ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.foreign_names.insert(handle.id(), title.clone());
                state.foreign_titles.push(title);
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => state.foreign_closed += 1,
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, ()> for Client {
    fn event(state: &mut Self, _: &ext_workspace_manager_v1::ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_workspace_manager_v1::Event::Workspace { workspace } => state.workspaces.push(workspace),
            ext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => state.workspace_groups.push(workspace_group),
            ext_workspace_manager_v1::Event::Done => state.workspace_done += 1,
            _ => {}
        }
    }
    wayland_client::event_created_child!(Client, ext_workspace_manager_v1::ExtWorkspaceManagerV1, [
        0 => (ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()),
        1 => (ext_workspace_handle_v1::ExtWorkspaceHandleV1, ())
    ]);
}

impl Dispatch<ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()> for Client {
    fn event(state: &mut Self, proxy: &ext_workspace_handle_v1::ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_workspace_handle_v1::Event::Name { name } => {
                state.workspace_name_events.push((proxy.clone(), name.clone()));
                state.workspace_names.push(name);
            }
            ext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                let coordinates = coordinates.chunks_exact(4)
                    .map(|value| u32::from_ne_bytes(value.try_into().unwrap())).collect();
                state.workspace_coordinates.push((proxy.clone(), coordinates));
            }
            ext_workspace_handle_v1::Event::Removed => state.workspace_removed += 1,
            ext_workspace_handle_v1::Event::State { state: wayland_client::WEnum::Value(flags) } => state.workspace_active.push(flags.contains(ext_workspace_handle_v1::State::Active)),
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()> for Client {
    fn event(state: &mut Self, _: &ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_workspace_group_handle_v1::Event::OutputEnter { .. } => state.workspace_output_enters += 1,
            ext_workspace_group_handle_v1::Event::OutputLeave { .. } => state.workspace_output_leaves += 1,
            _ => {}
        }
    }
}

fn workspace_snapshot(owner: u64) -> WorkspaceSnapshot {
    WorkspaceSnapshot { owner, entries: vec![
        WorkspaceEntry { id: 11, name: "Home".into(), active: true },
        WorkspaceEntry { id: 29, name: "Work".into(), active: false },
    ] }
}

#[test]
fn workspace_requests_require_control_permission_and_commit_atomically() {
    for control in [false, true] {
        let capability = if control { crate::launch::Capability::WindowControl } else { crate::launch::Capability::WindowList };
        let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![capability]));
        harness.server.workspaces(workspace_snapshot(41));
        harness.sync();
        assert_eq!(harness.client.workspace_names, vec!["Home", "Work"]);
        assert_eq!(harness.client.workspace_active, vec![true, false]);
        let manager = harness.client.workspace_manager.as_ref().unwrap().clone();
        harness.client.workspaces[0].activate();
        harness.client.workspaces[1].activate();
        harness.sync();
        assert!(harness.server.workspace_activations().is_empty());
        manager.commit();
        harness.sync();
        assert_eq!(harness.server.workspace_activations(), if control { vec![(41, 29)] } else { Vec::new() });
        let done = harness.client.workspace_done;
        for _ in 0..4 { harness.sync(); }
        assert_eq!(harness.client.workspace_done, done);
    }
}

#[test]
fn stale_workspace_handles_and_pending_requests_cannot_cross_canvas_owners() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    let old = harness.client.workspaces[0].clone();
    old.activate();
    harness.sync();
    harness.server.workspaces(workspace_snapshot(42));
    harness.sync();
    assert_eq!(harness.client.workspace_removed, 2);
    old.activate();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert!(harness.server.workspace_activations().is_empty());
    harness.client.workspaces[2].activate();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert_eq!(harness.server.workspace_activations(), vec![(42, 11)]);
}

#[test]
fn removed_or_destroyed_workspace_handles_remain_inert() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    harness.client.workspaces[0].destroy();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.workspaces.len(), 2);
    harness.client.workspaces[1].activate();
    harness.sync();
    let mut snapshot = workspace_snapshot(41);
    snapshot.entries.pop();
    harness.server.workspaces(snapshot);
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert!(harness.server.workspace_activations().is_empty());
    assert_eq!(harness.client.workspace_removed, 1);
}

#[test]
fn session_lock_discards_workspace_activation_including_pending_transactions() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    harness.client.workspaces[1].activate();
    harness.sync();
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert!(harness.server.workspace_activations().is_empty());
}

#[test]
fn workspace_names_active_state_and_output_membership_update_without_reannouncement() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowList]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    assert_eq!(harness.client.workspace_output_enters, 1);
    let mut snapshot = workspace_snapshot(41);
    snapshot.entries[0].name = "Renamed".into();
    snapshot.entries[0].active = false;
    snapshot.entries[1].active = true;
    harness.server.workspaces(snapshot);
    harness.sync();
    assert_eq!(harness.client.workspaces.len(), 2);
    assert_eq!(harness.client.workspace_names.last().map(String::as_str), Some("Renamed"));
    assert_eq!(&harness.client.workspace_active[2..], &[false, true]);
    harness.server.configure_outputs(&[Monitor { id: OutputId(2), name: "Replacement".into(), origin: [0, 0], size: [640, 480] }]).unwrap();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.workspace_output_enters, 2);
    assert_eq!(harness.client.workspace_output_leaves, 1);
    harness.client.workspace_groups[0].destroy();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.workspace_groups.len(), 1);
}

#[test]
fn reordered_workspaces_publish_coordinates_without_recreating_handles_or_redirecting_activation() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(),
        Some(vec![crate::launch::Capability::WindowList, crate::launch::Capability::WindowControl]));
    let mut snapshot = workspace_snapshot(41);
    harness.server.workspaces(snapshot.clone());
    harness.sync();
    let handles = harness.client.workspaces.clone();
    assert_eq!(handles.len(), 2);
    assert_eq!(harness.client.workspace_coordinates, vec![(handles[0].clone(), vec![0]), (handles[1].clone(), vec![1])]);
    let done = harness.client.workspace_done;
    snapshot.entries.swap(0, 1);
    harness.server.workspaces(snapshot.clone());
    harness.sync();
    assert_eq!(harness.client.workspaces, handles);
    assert_eq!(harness.client.workspace_removed, 0);
    assert_eq!(harness.client.workspace_names.len(), 2);
    assert_eq!(harness.client.workspace_done, done + 1);
    assert_eq!(&harness.client.workspace_coordinates[2..], &[(handles[1].clone(), vec![0]), (handles[0].clone(), vec![1])]);
    harness.server.workspaces(snapshot);
    harness.sync();
    assert_eq!(harness.client.workspace_done, done + 1);
    assert_eq!(harness.client.workspace_coordinates.len(), 4);
    handles[0].activate();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert_eq!(harness.server.workspace_activations(), vec![(41, 11)]);
}

#[test]
fn invalid_workspace_snapshots_do_not_replace_the_last_accepted_state() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    for invalid in 0..3 {
        let mut snapshot = workspace_snapshot(42);
        match invalid {
            0 => snapshot.entries[1].id = snapshot.entries[0].id,
            1 => snapshot.entries[0].name = "x".repeat(4097),
            _ => snapshot.entries[0].id = 0,
        }
        harness.server.workspaces(snapshot);
    }
    harness.client.workspaces[1].activate();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    assert_eq!(harness.server.workspace_activations(), vec![(41, 29)]);
    assert_eq!(harness.client.workspace_removed, 0);
}

#[test]
fn removed_workspace_requests_do_not_reappear_when_the_workspace_is_restored() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    harness.server.workspaces(workspace_snapshot(41));
    harness.sync();
    harness.client.workspaces[1].activate();
    harness.client.workspace_manager.as_ref().unwrap().commit();
    harness.sync();
    let mut snapshot = workspace_snapshot(41);
    snapshot.entries.pop();
    harness.server.workspaces(snapshot);
    harness.server.workspaces(workspace_snapshot(41));
    assert!(harness.server.workspace_activations().is_empty());
}

impl Dispatch<zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1, ()> for Client {
    fn event(state: &mut Self, _: &zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event { state.managed.push(toplevel); }
    }
    wayland_client::event_created_child!(Client, zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1, [
        0 => (zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1, ()> for Client {
    fn event(state: &mut Self, _: &zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Done => state.management_done += 1,
            zwlr_foreign_toplevel_handle_v1::Event::Closed => state.management_closed += 1,
            zwlr_foreign_toplevel_handle_v1::Event::State { state: flags } => state.management_minimized.push(flags.chunks_exact(4)
                .any(|flag| u32::from_ne_bytes(flag.try_into().unwrap()) == zwlr_foreign_toplevel_handle_v1::State::Minimized as u32)),
            _ => {}
        }
    }
}

#[test]
fn taskbar_control_requires_separate_connection_authorization() {
    for control in [false, true] {
        let capability = if control { crate::launch::Capability::WindowControl } else { crate::launch::Capability::WindowList };
        let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![capability]));
        let window = harness.window();
        let _buffer = harness.attach(&window, [320, 240]);
        harness.sync();
        let handle = harness.client.managed[0].clone();
        let id = harness.server.windows()[0].id;
        handle.set_maximized();
        handle.set_fullscreen(None);
        handle.set_minimized();
        handle.activate(harness.client.seat.as_ref().unwrap());
        handle.close();
        harness.sync();
        let requests = harness.server.requests();
        assert_eq!(requests.len(), usize::from(control));
        if control {
            assert_eq!(requests[0], (id, WindowRequest { minimized: Some(true), fullscreen: Some(true), maximized: Some(true) }));
            assert_eq!(harness.server.activations(), vec![id]);
        } else { assert!(harness.server.activations().is_empty()); }
        assert_eq!(harness.client.closed, control);
    }
}

#[test]
fn xdg_and_taskbar_minimization_use_policy_requests_and_report_confirmed_state() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    let id = harness.server.windows()[0].id;
    let handle = harness.client.managed[0].clone();
    assert_eq!(harness.client.management_minimized.last(), Some(&false));
    window.role.set_minimized();
    harness.sync();
    assert_eq!(harness.server.requests(), vec![(id, WindowRequest { minimized: Some(true), ..Default::default() })]);
    assert_eq!(harness.client.management_minimized.last(), Some(&false));
    harness.server.configure_state(id, [320, 240], WindowState { minimized: true, ..Default::default() }).unwrap();
    harness.sync();
    assert_eq!(harness.client.management_minimized.last(), Some(&true));
    handle.unset_minimized();
    harness.sync();
    assert_eq!(harness.server.requests(), vec![(id, WindowRequest { minimized: Some(false), ..Default::default() })]);
    harness.server.configure_state(id, [320, 240], WindowState::default()).unwrap();
    harness.sync();
    assert_eq!(harness.client.management_minimized.last(), Some(&false));
    handle.set_minimized();
    harness.sync();
    assert_eq!(harness.server.requests(), vec![(id, WindowRequest { minimized: Some(true), ..Default::default() })]);
}

#[test]
fn taskbar_handles_close_once_and_unchanged_dispatch_does_not_emit_updates() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowList]));
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    assert_eq!(harness.client.managed.len(), 1);
    let done = harness.client.management_done;
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.management_done, done);
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.management_closed, 1);
    harness.client.managed[0].destroy();
    harness.sync();
}

#[test]
fn destroying_a_taskbar_handle_does_not_reannounce_the_same_window() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowList]));
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    harness.client.managed[0].destroy();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.managed.len(), 1);
}

#[test]
fn a_closed_taskbar_handle_remains_inert_after_the_window_remaps() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    let old = harness.client.managed[0].clone();
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    for _ in 0..4 { harness.sync(); }
    window.surface.commit();
    for _ in 0..2 { harness.sync(); }
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    assert_eq!(harness.client.management_closed, 1);
    assert_eq!(harness.client.managed.len(), 2);
    old.close();
    old.activate(harness.client.seat.as_ref().unwrap());
    old.set_maximized();
    harness.sync();
    assert!(!harness.client.closed);
    assert!(harness.server.requests().is_empty());
    assert!(harness.server.activations().is_empty());
}

#[test]
fn authorized_taskbar_controls_are_denied_during_session_lock() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowControl]));
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    let handle = harness.client.managed[0].clone();
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    handle.close();
    handle.activate(harness.client.seat.as_ref().unwrap());
    handle.set_maximized();
    handle.set_fullscreen(None);
    handle.set_minimized();
    harness.sync();
    assert!(!harness.client.closed);
    assert!(harness.server.requests().is_empty());
    assert!(harness.server.activations().is_empty());
}

#[test]
fn helper_capabilities_are_connection_scoped_and_independent() {
    use crate::launch::Capability;
    for (capabilities, clipboard, listing, management) in [
        (Vec::new(), false, false, false),
        (vec![Capability::WindowList], false, true, true),
        (vec![Capability::WindowControl], false, false, true),
        (vec![Capability::Clipboard], true, false, false),
    ] {
        let harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(capabilities));
        assert_eq!(harness.client.globals.iter().any(|name| name == "zwlr_data_control_manager_v1"), clipboard);
        assert_eq!(harness.client.foreign.is_some(), listing);
        assert_eq!(harness.client.management.is_some(), management);
        assert_eq!(harness.client.workspace_manager.is_some(), management);
    }
}

#[test]
fn foreign_listing_follows_mapping_metadata_and_unmapping() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowList]));
    let window = harness.window();
    assert!(harness.client.foreign_titles.is_empty());
    let _buffer = harness.attach(&window, [640, 480]);
    harness.sync();
    assert_eq!(harness.client.foreign_titles.last().map(String::as_str), Some("A real Wayland window"));
    window.role.set_title("Updated title".into());
    harness.sync();
    assert_eq!(harness.client.foreign_titles.last().map(String::as_str), Some("Updated title"));
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert_eq!(harness.client.foreign_closed, 1);
}

#[test]
fn damage_history_survives_skipped_commits_and_snapshots_remain_immutable() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _initial = harness.attach(&window, [100, 100]);
    let initial = harness.server.windows()[0].nodes[0].clone();
    let commit = initial.damage.current_commit();
    let first = harness.buffer([100, 100]);
    window.surface.attach(Some(&first), 0, 0);
    window.surface.damage_buffer(10, 10, 8, 8);
    window.surface.commit();
    harness.sync();
    let second = harness.buffer([100, 100]);
    window.surface.attach(Some(&second), 0, 0);
    window.surface.damage_buffer(40, 40, 8, 8);
    window.surface.commit();
    harness.sync();
    let current = harness.server.windows()[0].nodes[0].clone();
    let changed = current.buffer_damage_since(Some(commit));
    assert!(changed.iter().any(|rectangle| rectangle.contains((12, 12))));
    assert!(changed.iter().any(|rectangle| rectangle.contains((42, 42))));
    assert!(!changed.iter().any(|rectangle| rectangle.contains((90, 90))));
    assert!(initial.buffer_damage_since(Some(commit)).is_empty());
    let region = harness.client.compositor.as_ref().unwrap().create_region(&harness.queue.handle(), ());
    region.add(0, 0, 100, 100);
    window.surface.set_opaque_region(Some(&region));
    window.surface.commit();
    harness.sync();
    assert!(harness.server.windows()[0].nodes[0].fully_opaque());
    assert!(!initial.fully_opaque());
    region.subtract(40, 40, 20, 20);
    window.surface.set_opaque_region(Some(&region));
    window.surface.commit();
    harness.sync();
    assert!(!harness.server.windows()[0].nodes[0].fully_opaque());
}

delegate_noop!(Client: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
delegate_noop!(Client: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(Client: ignore wp_viewporter::WpViewporter);
delegate_noop!(Client: ignore wp_viewport::WpViewport);
delegate_noop!(Client: ignore zwp_input_method_manager_v2::ZwpInputMethodManagerV2);
delegate_noop!(Client: ignore zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2);
delegate_noop!(Client: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(Client: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

impl Dispatch<zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2, ()> for Client {
    fn event(state: &mut Self, _: &zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Keymap { fd, size, .. } => state.input_keymap = Some((fd, size)),
            zwp_input_method_keyboard_grab_v2::Event::Key { key, state: wayland_client::WEnum::Value(value), .. } => {
                state.input_keys.push((key, value == wl_keyboard::KeyState::Pressed));
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } => {
                state.input_modifiers.push([mods_depressed, mods_latched, mods_locked, group]);
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_input_method_v2::ZwpInputMethodV2, ()> for Client {
    fn event(state: &mut Self, proxy: &zwp_input_method_v2::ZwpInputMethodV2,
        event: zwp_input_method_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_input_method_v2::Event::Done => state.input_serial = state.input_serial.wrapping_add(1),
            zwp_input_method_v2::Event::SurroundingText { text, cursor, anchor } => state.input_surrounding.push((text, cursor, anchor)),
            zwp_input_method_v2::Event::TextChangeCause { cause: wayland_client::WEnum::Value(cause) } => state.input_causes.push(cause as u32),
            zwp_input_method_v2::Event::Activate => state.input_active = true,
            zwp_input_method_v2::Event::Deactivate => state.input_active = false,
            zwp_input_method_v2::Event::Unavailable => state.input_unavailable.push(proxy.id()),
            _ => {}
        }
    }
}

#[test]
fn input_method_and_keyboard_injection_are_connection_scoped() {
    let ordinary = Harness::new();
    assert!(ordinary.client.input_method.is_none());
    assert!(!ordinary.client.globals.iter().any(|name| name == "zwp_virtual_keyboard_manager_v1"));
    let mut authorized = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    assert!(authorized.client.input_method.is_some());
    assert!(authorized.client.globals.iter().any(|name| name == "zwp_virtual_keyboard_manager_v1"));
    let method = authorized.client.input_method.as_ref().unwrap().get_input_method(
        authorized.client.seat.as_ref().unwrap(), &authorized.queue.handle(), (),
    );
    authorized.sync();
    assert!(!authorized.client.input_active);
    assert!(authorized.client.input_surrounding.is_empty());
    method.destroy();
    authorized.sync();
}

#[cfg(feature = "nested")]
#[test]
fn native_text_input_transports_unicode_preedit_deletion_and_secret_context_without_stale_commits() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    let method = harness.client.input_method.as_ref().unwrap().get_input_method(
        harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    for _ in 0..8 { harness.sync(); }
    assert!(harness.server.native_ready());
    let context = crate::native_input::Editor { owner: 1, revision: 1, projection: 0, from_ime: false, selection: [2, 2], surrounding: "é世".into(),
        cursor: 2, anchor: 2, sensitive: false, multiline: false, caret: [20, 20, 2, 20] };
    harness.server.native_editor(Some(context.clone()));
    harness.server.native_focus();
    for _ in 0..8 { harness.sync(); }
    assert!(harness.client.input_active);
    assert_eq!(harness.client.input_surrounding.last(), Some(&("é世".into(), 2, 2)));
    assert_eq!(harness.client.input_causes.last(), Some(&1));
    method.delete_surrounding_text(2, 0);
    method.commit_string("Olá".into());
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(harness.client.input_serial);
    for _ in 0..4 { harness.sync(); }
    let events = harness.server.native_events();
    let edits: Vec<_> = events.into_iter().filter_map(|event| match event {
        crate::native_input::client::Event::Edit(edit) => Some(edit), _ => None,
    }).collect();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].context, context);
    assert_eq!(edits[0].delete, Some((2, 0)));
    assert_eq!(edits[0].commit.as_deref(), Some("Olá"));
    assert_eq!(edits[0].preedit, Some(("界".into(), Some((0, 3)))));
    let mut popup_context = context.clone();
    popup_context.from_ime = true;
    popup_context.caret = [620, 450, 2, 20];
    harness.server.native_editor(Some(popup_context));
    let bounds = lince_interface::host::Rectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
    harness.server.state.presented_geometry = Some(lince_interface::host::GeometrySnapshot {
        revision: 1, items: Vec::new(), focused: None, outputs: vec![lince_interface::host::OutputViewport { rotation: Default::default(),
            id: OutputId(1), bounds, usable: lince_interface::host::Rectangle::new([0.0, 30.0], [640.0, 450.0]),
            physical_size: [640, 480], raster: lince_interface::host::raster::RasterMapping { origin: (bounds).origin(), scale: [1.0; 2] }, workspace: 1,
        }],
    });
    for _ in 0..8 { harness.sync(); }
    assert_eq!(harness.client.input_causes.last(), Some(&0));
    let popup_surface = harness.client.compositor.as_ref().unwrap().create_surface(&harness.queue.handle(), ());
    let popup = method.get_input_popup_surface(&popup_surface, &harness.queue.handle(), ());
    let buffer = harness.buffer([64, 32]);
    popup_surface.attach(Some(&buffer), 0, 0);
    popup_surface.commit();
    for _ in 0..4 { harness.sync(); }
    let popups = harness.server.native_popups();
    assert_eq!(popups.len(), 1);
    assert_eq!(popups[0].origin, [576, 418]);
    let previous = harness.client.input_serial;
    let protected = crate::native_input::Editor { owner: 2, revision: 2, sensitive: true, surrounding: String::new(),
        cursor: 0, anchor: 0, selection: [8, 8], ..context };
    harness.server.native_editor(Some(protected.clone()));
    for _ in 0..8 { harness.sync(); }
    assert_eq!(harness.client.input_surrounding.last(), Some(&(String::new(), 0, 0)));
    method.commit_string("stale".into());
    method.commit(previous);
    for _ in 0..4 { harness.sync(); }
    assert!(harness.server.native_events().is_empty());
    harness.server.native_editor(None);
    for _ in 0..8 { harness.sync(); }
    assert!(!harness.client.input_active);
    harness.server.focus(None, 100.into());
    assert!(harness.server.native_popups().is_empty());
    popup.destroy();
    popup_surface.destroy();
    method.destroy();
    harness.sync();
    harness.client.input_serial = 0;
    let restarted = harness.client.input_method.as_ref().unwrap().get_input_method(
        harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.server.native_editor(Some(protected));
    harness.server.native_focus();
    for _ in 0..8 { harness.sync(); }
    assert!(harness.client.input_active);
    assert_eq!(harness.client.input_surrounding.last(), Some(&(String::new(), 0, 0)));
    restarted.destroy();
    harness.sync();
}

#[cfg(feature = "nested")]
#[test]
fn internal_native_keyboard_receives_one_pair_of_physical_key_events() {
    let mut harness = Harness::new();
    for _ in 0..8 { harness.sync(); }
    harness.server.native_focus();
    for _ in 0..4 { harness.sync(); }
    harness.server.native_events();
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.server.keyboard(30, false, 2, |_| false).unwrap();
    for _ in 0..4 { harness.sync(); }
    let keys: Vec<_> = harness.server.native_events().into_iter().filter_map(|event| match event {
        crate::native_input::client::Event::Key { code, pressed, event } => Some((code, pressed, event.text)),
        _ => None,
    }).collect();
    assert_eq!(keys, vec![(30, true, Some("a".into())), (30, false, Some("a".into()))]);
}

#[cfg(feature = "nested")]
#[test]
fn input_method_grabs_suppress_direct_native_keys_and_virtual_passthrough_arrives_once() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    let method = harness.client.input_method.as_ref().unwrap().get_input_method(
        harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    for _ in 0..8 { harness.sync(); }
    harness.server.native_focus();
    for _ in 0..4 { harness.sync(); }
    let grab = method.grab_keyboard(&harness.queue.handle(), ());
    for _ in 0..4 { harness.sync(); }
    assert!(harness.server.input_method_grabbed());
    harness.server.native_events();
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.server.keyboard(30, false, 2, |_| false).unwrap();
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.input_keys, [(30, true), (30, false)]);
    assert!(harness.server.native_events().is_empty());
    let keyboard = harness.client.virtual_keyboard.as_ref().unwrap().create_virtual_keyboard(
        harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    let (fd, size) = harness.client.input_keymap.as_ref().unwrap();
    keyboard.keymap(1, fd.as_fd(), *size);
    keyboard.key(3, 30, 1);
    keyboard.key(4, 30, 0);
    for _ in 0..4 { harness.sync(); }
    let keys: Vec<_> = harness.server.native_events().into_iter().filter_map(|event| match event {
        crate::native_input::client::Event::Key { code, pressed, .. } => Some((code, pressed)), _ => None,
    }).collect();
    assert_eq!(keys, [(30, true), (30, false)]);
    keyboard.destroy();
    grab.release();
    method.destroy();
    harness.sync();
}
delegate_noop!(Client: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
delegate_noop!(Client: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(Client: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for Client {
    fn event(state: &mut Self, _: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.idle_events.push(true),
            ext_idle_notification_v1::Event::Resumed => state.idle_events.push(false),
            _ => {}
        }
    }
}

#[test]
fn idle_notifications_resume_on_activity_and_destroy_clears_the_deadline() {
    let mut harness = Harness::new();
    let notification = harness.client.idle.as_ref().unwrap().get_idle_notification(
        0, harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.sync();
    assert_eq!(harness.client.idle_events, [true]);
    assert!(harness.server.idle_timeout().is_none());
    harness.server.activity();
    harness.sync();
    assert_eq!(harness.client.idle_events, [true, false, true]);
    notification.destroy();
    harness.sync();
    let notification = harness.client.idle.as_ref().unwrap().get_input_idle_notification(
        60_000, harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.sync();
    assert!(harness.server.idle_timeout().is_some());
    notification.destroy();
    harness.sync();
    assert!(harness.server.idle_timeout().is_none());
}

#[test]
fn hidden_surface_inhibitors_do_not_prevent_idle_and_are_reference_counted() {
    let mut harness = Harness::new();
    let window = harness.window();
    let manager = harness.client.idle_inhibit.as_ref().unwrap();
    let first = manager.create_inhibitor(&window.surface, &harness.queue.handle(), ());
    let second = manager.create_inhibitor(&window.surface, &harness.queue.handle(), ());
    let notification = harness.client.idle.as_ref().unwrap().get_idle_notification(
        0, harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.sync();
    assert_eq!(harness.client.idle_events, [true]);
    first.destroy();
    harness.sync();
    second.destroy();
    notification.destroy();
    harness.sync();
    assert!(harness.server.idle_timeout().is_none());
}

#[test]
fn visible_inhibitors_respect_reference_counts_input_idle_and_session_lock() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    let rectangle = lince_interface::host::Rectangle::new([0.0; 2], [640.0, 480.0]).unwrap();
    harness.server.state.presented_geometry = Some(lince_interface::host::GeometrySnapshot {
        revision: 1, outputs: Vec::new(), focused: None,
        items: vec![lince_interface::host::ItemGeometry {
            id: lince_interface::host::ItemId::Application(id), output: OutputId(1),
            mapping: lince_interface::host::ContentMapping { corners: None, canvas: rectangle, surface: rectangle },
            floating: false, clip: None,
        }],
    });
    let manager = harness.client.idle_inhibit.as_ref().unwrap();
    let first = manager.create_inhibitor(&window.surface, &harness.queue.handle(), ());
    let second = manager.create_inhibitor(&window.surface, &harness.queue.handle(), ());
    harness.sync();
    let ordinary = harness.client.idle.as_ref().unwrap().get_idle_notification(
        0, harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.sync();
    assert!(harness.client.idle_events.is_empty());
    first.destroy();
    harness.sync();
    assert!(harness.client.idle_events.is_empty());
    let input = harness.client.idle.as_ref().unwrap().get_input_idle_notification(
        0, harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), (),
    );
    harness.sync();
    assert_eq!(harness.client.idle_events, [true]);
    input.destroy();
    harness.sync();
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert_eq!(harness.client.idle_events, [true, true]);
    second.destroy();
    ordinary.destroy();
    harness.sync();
    assert!(harness.server.idle_timeout().is_none());
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for Client {
    fn event(state: &mut Self, _: &wp_fractional_scale_v1::WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scales.push(scale);
        }
    }
}

#[test]
fn fractional_output_geometry_and_client_preference_agree() {
    let mut server = Server::new([800, 600]).unwrap();
    let monitor = Monitor { id: OutputId(1), name: "Park".into(), origin: [0, 0], size: [800, 600] };
    server.configure_scaled_outputs(std::slice::from_ref(&monitor), &BTreeMap::from([(monitor.id, 1.25)])).unwrap();
    let viewport = &server.output_viewports()[0].1;
    assert_eq!(viewport.bounds.size(), [640.0, 480.0]);
    assert_eq!(viewport.physical_size, [800, 600]);
    let mut harness = Harness::connect(server);
    let window = harness.window();
    let _scale = harness.client.fractional.as_ref().unwrap().get_fractional_scale(&window.surface, &harness.queue.handle(), ());
    harness.sync();
    assert_eq!(harness.client.preferred_scales.last(), Some(&150));
    let before = harness.server.output_viewports();
    for scale in [0.0, f64::NAN, 10.0, 1.234] {
        assert!(harness.server.configure_scaled_outputs(std::slice::from_ref(&monitor), &BTreeMap::from([(monitor.id, scale)])).is_err());
        assert_eq!(harness.server.output_viewports(), before);
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => state.lock_replies.push(true),
            ext_session_lock_v1::Event::Finished => state.lock_replies.push(false),
            _ => {}
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure { serial, width, height, .. } = event {
            state.lock_sizes.push([width, height]);
            proxy.ack_configure(serial);
        }
    }
}

#[test]
fn parented_windows_use_the_actual_parent_and_survive_its_destruction() {
    let mut harness = Harness::new();
    let parent = harness.window();
    harness.attach(&parent, [400, 300]);
    let parent_id = harness.server.windows()[0].id;
    harness.server.configure(parent_id, [600, 400]).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_parent(Some(&parent.role));
    surface.commit();
    harness.sync();
    let child = harness
        .server
        .windows()
        .into_iter()
        .find(|window| window.id != parent_id)
        .unwrap();
    assert_eq!(child.parent, Some(parent_id));
    assert_eq!(
        harness.server.state.windows[&child.id].configured_size,
        [450, 300]
    );
    parent.role.destroy();
    parent.xdg.destroy();
    parent.surface.destroy();
    harness.sync();
    let windows = harness.server.windows();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].id, child.id);
    assert_eq!(windows[0].parent, None);
}

#[test]
fn lock_confirmation_waits_for_presentation_and_rejected_locks_cannot_unlock() {
    let mut harness = Harness::new();
    let manager = harness.client.lock_manager.as_ref().unwrap().clone();
    let _owner = manager.lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert!(harness.client.lock_replies.is_empty());
    harness.server.lock_presented();
    harness.sync();
    assert_eq!(harness.client.lock_replies, [true]);
    let rejected = manager.lock(&harness.queue.handle(), ());
    harness.sync();
    assert_eq!(harness.client.lock_replies, [true, false]);
    rejected.unlock_and_destroy();
    assert!(
        synchronize(
            &mut harness.server,
            &harness.connection,
            &mut harness.queue,
            &mut harness.client
        )
        .is_err()
    );
    assert!(harness.server.locked());
}

#[test]
fn locker_disconnect_keeps_the_session_locked_and_new_outputs_protected() {
    let mut server = {
        let mut harness = Harness::new();
        let _owner = harness
            .client
            .lock_manager
            .as_ref()
            .unwrap()
            .lock(&harness.queue.handle(), ());
        harness.sync();
        harness.server.lock_presented();
        harness.sync();
        harness.server
    };
    server.dispatch().unwrap();
    assert!(server.locked());
    server
        .configure_outputs(&[
            Monitor {
                id: OutputId(1),
                name: "one".into(),
                origin: [0, 0],
                size: [640, 480],
            },
            Monitor {
                id: OutputId(2),
                name: "two".into(),
                origin: [640, 0],
                size: [800, 600],
            },
        ])
        .unwrap();
    assert!(server.locked());
    assert!(server.lock_surfaces().is_empty());
}

#[test]
fn only_the_lock_surface_can_receive_focus_while_locked() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    let owner = harness
        .client
        .lock_manager
        .as_ref()
        .unwrap()
        .lock(&harness.queue.handle(), ());
    harness.sync();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&harness.queue.handle(), ());
    let _role = owner.get_lock_surface(
        &surface,
        harness.client.output.as_ref().unwrap(),
        &harness.queue.handle(),
        (),
    );
    harness.sync();
    let buffer = harness.buffer([640, 480]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    harness.server.lock_presented();
    harness.server.focus(Some(id), 80.into());
    assert!(
        harness
            .server
            .state
            .seat
            .get_keyboard()
            .unwrap()
            .current_focus()
            .is_none()
    );
    harness.server.focus_lock(Some(OutputId(1)));
    assert!(
        harness
            .server
            .state
            .seat
            .get_keyboard()
            .unwrap()
            .current_focus()
            .is_some()
    );
    owner.unlock_and_destroy();
    harness.sync();
    assert!(!harness.server.locked());
}

impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for Client {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Release);
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.serials += 1;
            state.surface_serials.insert(proxy.id(), serial);
            if !state.ignore_configure {
                proxy.ack_configure(serial);
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                state.configures.push([width, height]);
                state.window_configure_sizes.entry(proxy.id()).or_default().push([width, height]);
                let states: Vec<_> = states
                    .chunks_exact(4)
                    .map(|value| u32::from_ne_bytes(value.try_into().unwrap()))
                    .collect();
                state.window_states.push(states.clone());
                state.window_configurations.insert(proxy.id(), ([width, height], states));
            }
            xdg_toplevel::Event::Close => state.closed = true,
            _ => {}
        }
    }
}

#[test]
fn client_window_state_requests_are_coalesced_until_map_and_confirmed_by_policy() {
    let mut harness = Harness::new();
    let window = harness.window();
    window.role.set_maximized();
    window.role.unset_maximized();
    window.role.set_maximized();
    window.role.set_fullscreen(None);
    window.role.unset_fullscreen();
    harness.sync();
    assert!(harness.server.requests().is_empty());
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    assert_eq!(
        harness.server.requests(),
        [(
            id,
            WindowRequest {
                minimized: None,
                maximized: Some(true),
                fullscreen: Some(false)
            }
        )]
    );
    assert!(harness.server.requests().is_empty());
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                minimized: false,
                maximized: true,
                fullscreen: false,
                windowed_fullscreen: false,
                resizing: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.configures.last(), Some(&[620, 440]));
    assert!(
        harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Maximized as u32))
    );
    assert!(
        !harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Fullscreen as u32))
    );
    let serials = harness.client.serials;
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                minimized: false,
                maximized: true,
                fullscreen: false,
                windowed_fullscreen: false,
                resizing: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.serials, serials);
    window.role.set_maximized();
    harness.sync();
    assert_eq!(harness.server.requests().len(), 1);
    harness
        .server
        .configure_state(
            id,
            [620, 440],
            WindowState {
                minimized: false,
                maximized: true,
                fullscreen: false,
                windowed_fullscreen: false,
                resizing: false,
            },
        )
        .unwrap();
    harness.sync();
    assert_eq!(harness.client.serials, serials + 1);
    window.role.set_fullscreen(None);
    harness.sync();
    assert_eq!(
        harness.server.requests(),
        [(
            id,
            WindowRequest {
                minimized: None,
                maximized: None,
                fullscreen: Some(true)
            }
        )]
    );
    harness
        .server
        .configure_state(
            id,
            [640, 480],
            WindowState {
                minimized: false,
                maximized: false,
                fullscreen: true,
                windowed_fullscreen: false,
                resizing: false,
            },
        )
        .unwrap();
    harness.sync();
    assert!(
        harness
            .client
            .window_states
            .last()
            .unwrap()
            .contains(&(xdg_toplevel::State::Fullscreen as u32))
    );
    window.role.unset_fullscreen();
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert!(harness.server.requests().is_empty());
}

impl Dispatch<wl_buffer::WlBuffer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.releases += 1;
        }
    }
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(Client: ignore wl_subsurface::WlSubsurface);
delegate_noop!(Client: ignore wl_seat::WlSeat);
delegate_noop!(Client: ignore wl_region::WlRegion);
delegate_noop!(Client: ignore xdg_positioner::XdgPositioner);
delegate_noop!(Client: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(Client: ignore wl_output::WlOutput);
delegate_noop!(Client: ignore zxdg_output_manager_v1::ZxdgOutputManagerV1);

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::Name { name } => state.output_name = name,
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                state.output_size = [width, height]
            }
            _ => {}
        }
    }
}

#[test]
fn external_bars_receive_output_identity_and_logical_resize_information() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let _output = harness.client.outputs.as_ref().unwrap().get_xdg_output(
        harness.client.output.as_ref().unwrap(),
        &qh,
        (),
    );
    harness.sync();
    assert_eq!(harness.client.output_name, "Park");
    assert_eq!(harness.client.output_size, [640, 480]);
    harness.server.resize([800, 600]).unwrap();
    harness.sync();
    assert_eq!(harness.client.output_size, [800, 600]);
    assert_eq!(harness.client.output_name, "Park");
}

#[test]
fn bars_reserve_only_the_requested_output_and_close_on_disconnect() {
    let mut harness = Harness::new();
    let monitors = [
        Monitor {
            id: OutputId(1),
            name: "Park".into(),
            origin: [0, 0],
            size: [640, 480],
        },
        Monitor {
            id: OutputId(2),
            name: "DP-1".into(),
            origin: [640, 0],
            size: [800, 600],
        },
    ];
    harness.server.configure_outputs(&monitors).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let role = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            harness.client.output.as_ref(),
            zwlr_layer_shell_v1::Layer::Top,
            "second-output-bar".into(),
            &qh,
            (),
        );
    role.set_size(0, 30);
    role.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    role.set_exclusive_zone(30);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures.last(), Some(&[800, 30]));
    let buffer = harness.buffer([800, 30]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(
        harness.server.usable_output(OutputId(1)),
        Some(Rectangle::from_size((640, 480).into()))
    );
    assert_eq!(
        harness.server.usable_output(OutputId(2)),
        Some(Rectangle::new((0, 30).into(), (800, 570).into()))
    );
    let layers = harness.server.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].output, OutputId(2));
    assert_eq!(layers[0].origin, [640, 0]);
    harness.server.configure_outputs(&monitors[..1]).unwrap();
    harness.sync();
    assert!(harness.server.layers().is_empty());
    assert_eq!(harness.server.output_viewports().len(), 1);
}

#[test]
fn invalid_output_configurations_leave_the_active_session_unchanged() {
    let mut server = Server::new([640, 480]).unwrap();
    let before = server.output_viewports();
    let invalid = Monitor {
        id: OutputId(1),
        name: "Park".into(),
        origin: [i32::MAX, 0],
        size: [640, 480],
    };
    assert!(server.configure_outputs(&[invalid]).is_err());
    assert_eq!(server.output_viewports(), before);
}
delegate_noop!(Client: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure {
            mode: wayland_client::WEnum::Value(mode),
        } = event
        {
            state.decoration_modes.push(mode);
        }
    }
}

#[test]
fn decoration_negotiation_preserves_client_controls_and_initial_configure_order() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    let decoration = harness
        .client
        .decorations
        .as_ref()
        .unwrap()
        .get_toplevel_decoration(&role, &qh, ());
    decoration.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
    harness.sync();
    assert_eq!(harness.client.serials, 0);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.serials, 1);
    assert_eq!(
        harness.client.decoration_modes,
        [zxdg_toplevel_decoration_v1::Mode::ClientSide]
    );
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);
            state.layer_configures.push([width, height]);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Keymap { fd, size, .. } = event {
            use std::io::{Read, Seek, SeekFrom};
            assert!(size <= 1024 * 1024);
            let mut keymap = String::new();
            let mut file = std::fs::File::from(fd);
            file.seek(SeekFrom::Start(0)).unwrap();
            let bytes = file.take(u64::from(size)).read_to_string(&mut keymap).unwrap();
            assert_eq!(bytes, size as usize);
            state.keymaps.push(keymap.trim_end_matches('\0').to_owned());
        } else if let wl_keyboard::Event::RepeatInfo { rate, delay } = event {
            state.repeat_info.push((rate, delay));
        } else if let wl_keyboard::Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } = event {
            state.modifiers.push([mods_depressed, mods_latched, mods_locked, group]);
        } else if let wl_keyboard::Event::Enter { surface, .. } = event {
            state.keyboard_focus = Some(surface);
        } else if let wl_keyboard::Event::Leave { surface, .. } = event {
            if state.keyboard_focus.as_ref() == Some(&surface) { state.keyboard_focus = None; }
        } else if let wl_keyboard::Event::Key {
            key,
            serial,
            state: key_state,
            ..
        } = event
        {
            state.key_serial = serial;
            state.keys.push((
                key,
                key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed),
            ));
            state.key_recipients.push((state.keyboard_focus.as_ref().map(|surface| surface.id()), key,
                key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)));
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => state.popup_configures.push([x, y, width, height]),
            xdg_popup::Event::Repositioned { token } => state.popup_repositions.push(token),
            xdg_popup::Event::PopupDone => state.popup_done += 1,
            _ => {}
        }
    }
}

#[test]
fn popup_configure_reposition_mapping_and_authorized_grab_follow_the_parent() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(160, 80);
    positioner.set_anchor_rect(620, 460, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    positioner.set_constraint_adjustment(
        xdg_positioner::ConstraintAdjustment::SlideX | xdg_positioner::ConstraintAdjustment::SlideY,
    );
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    popup.grab(
        harness.client.seat.as_ref().unwrap(),
        harness.client.key_serial,
    );
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.popup_done, 0);
    assert_eq!(
        harness.client.popup_configures.last(),
        Some(&[480, 400, 160, 80])
    );
    assert!(harness.server.popup_grab_active());
    let buffer = harness.buffer([160, 80]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.server.windows()[0].popups.len(), 1);
    positioner.set_anchor_rect(10, 20, 1, 1);
    popup.reposition(&positioner, 77);
    harness.sync();
    harness.sync();
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_repositions, [77]);
    assert_eq!(
        harness.client.popup_configures.last(),
        Some(&[11, 21, 160, 80])
    );
    assert_eq!(harness.server.windows()[0].popups[0].origin, [11, 21]);
    harness.server.dismiss_popup_grab();
    harness.sync();
    assert!(!harness.server.popup_grab_active());
    assert_eq!(harness.client.popup_done, 1);
}

#[test]
fn a_popup_cannot_grab_focus_with_an_unrelated_serial() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(0, 0, 1, 1);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    popup.grab(harness.client.seat.as_ref().unwrap(), 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_done, 1);
    assert!(!harness.server.popup_grab_active());
    assert_eq!(harness.server.windows().len(), 1);
    assert!(harness.client.popup_configures.is_empty());
    assert!(harness.server.state.popups.surfaces.is_empty());
}

#[test]
fn extreme_positioners_disconnect_only_the_malformed_client() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    for geometry in [[16385, 50, 0, 0], [100, 50, i32::MAX, i32::MIN]] {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        harness.server.insert_client(server_stream).unwrap();
        let connection = Connection::from_socket(client_stream).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = Client::default();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        let positioner = client.shell.as_ref().unwrap().create_positioner(&qh, ());
        positioner.set_size(geometry[0], geometry[1]);
        positioner.set_anchor_rect(geometry[2], geometry[3], 1, 1);
        let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let _popup = xdg.get_popup(None, &positioner, &qh, ());
        surface.commit();
        assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
        let _next = harness.attach(&window, [400, 300]);
        assert_eq!(
            harness.server.windows()[0].nodes[0].content.size,
            [400, 300]
        );
        assert_eq!(harness.server.clients.load(Ordering::Acquire), 1);
    }
}

#[test]
fn extreme_window_and_subsurface_coordinates_do_not_escape_client_isolation() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    for subsurface in [false, true] {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        harness.server.insert_client(server_stream).unwrap();
        let connection = Connection::from_socket(client_stream).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = Client::default();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let _role = xdg.get_toplevel(&qh, ());
        surface.commit();
        synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
        if subsurface {
            let child = client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let role =
                client
                    .subcompositor
                    .as_ref()
                    .unwrap()
                    .get_subsurface(&child, &surface, &qh, ());
            role.set_position(i32::MAX, i32::MIN);
            let file = tempfile::tempfile().unwrap();
            file.set_len(16).unwrap();
            let pool = client
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), 16, &qh, ());
            let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &qh, ());
            child.attach(Some(&buffer), 0, 0);
            child.commit();
            surface.commit();
        } else {
            xdg.set_window_geometry(i32::MAX, i32::MIN, 100, 100);
            surface.commit();
        }
        assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
        let _next = harness.attach(&window, [400, 300]);
        assert_eq!(harness.server.windows().len(), 1);
        assert_eq!(
            harness.server.windows()[0].nodes[0].content.size,
            [400, 300]
        );
    }
}

#[test]
fn slow_popup_buffers_do_not_block_other_windows_and_parent_unmap_dismisses_them() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(20, 20, 1, 1);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let _popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_configures.len(), 1);
    let other = harness.window();
    let _other_buffer = harness.attach(&other, [400, 300]);
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .filter(|window| window.mapped)
            .count(),
        2
    );
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert_eq!(harness.client.popup_done, 1);
    assert!(harness.server.state.popups.surfaces.is_empty());
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .filter(|window| window.mapped)
            .count(),
        1
    );
}

#[test]
fn popup_parent_chains_are_bounded_before_manager_traversal() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(100, 50);
    positioner.set_anchor_rect(0, 0, 1, 1);
    let mut parent = window.xdg.clone();
    let mut popups = Vec::new();
    for _ in 0..35 {
        let surface = harness
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let xdg = harness
            .client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, ());
        let popup = xdg.get_popup(Some(&parent), &positioner, &qh, ());
        surface.commit();
        harness.sync();
        parent = xdg.clone();
        popups.push((surface, xdg, popup));
    }
    assert_eq!(harness.server.state.popups.surfaces.len(), 32);
    assert_eq!(harness.client.popup_done, 3);
    assert_eq!(harness.client.popup_configures.len(), 32);
    let _next = harness.attach(&window, [400, 300]);
    assert_eq!(
        harness.server.windows()[0].nodes[0].content.size,
        [400, 300]
    );
}

#[test]
fn another_client_cannot_use_a_window_owners_input_serial() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let stolen_serial = harness.client.key_serial;
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    harness.server.insert_client(server_stream).unwrap();
    let connection = Connection::from_socket(client_stream).unwrap();
    let mut queue = connection.new_event_queue();
    let mut client = Client::default();
    connection.display().get_registry(&queue.handle(), ());
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let qh = queue.handle();
    let request = client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(stolen_serial, client.seat.as_ref().unwrap());
    request.commit();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    assert_eq!(client.tokens.len(), 1);
    assert_eq!(harness.server.state.activation.tokens().count(), 0);
    assert!(harness.server.activations().is_empty());
    assert_eq!(harness.server.windows().len(), 1);
}

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.tokens.push(token);
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for Client {
    fn event(state: &mut Self, _: &wl_data_source::WlDataSource, event: wl_data_source::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_source::Event::Send { fd, .. } => {
                use std::io::Write;
                if state.drop_hold { state.drop_held.push(fd); assert!(state.drop_held.len() <= 8); }
                else { state.drop_send_failed = std::fs::File::from(fd).write_all(&state.drop_payload).is_err(); }
            }
            wl_data_source::Event::DndDropPerformed => state.drop_performed += 1,
            wl_data_source::Event::DndFinished => state.drop_finished += 1,
            wl_data_source::Event::Cancelled => state.drop_cancelled += 1,
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for Client {
    fn event(_: &mut Self, _: &wl_data_device::WlDataDevice, _: wl_data_device::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    wayland_client::event_created_child!(Client, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);
}

delegate_noop!(Client: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(Client: ignore wl_data_offer::WlDataOffer);

#[cfg(feature = "nested")]
#[test]
fn native_drop_finishes_only_after_destination_acknowledges_the_payload() {
    native_drop_completion(Some(true));
}

#[cfg(feature = "nested")]
#[test]
fn native_drop_rejection_never_reports_success_to_the_source() {
    native_drop_completion(Some(false));
}

#[cfg(feature = "nested")]
#[test]
fn native_drop_cancellation_invalidates_queued_frontend_payload() {
    native_drop_completion(None);
}

#[cfg(feature = "nested")]
#[test]
fn native_drop_output_change_invalidates_queued_frontend_payload() {
    native_drop_completion_for(None, NativeDropChange::Output);
}

#[cfg(feature = "nested")]
#[test]
fn native_drop_target_removal_invalidates_queued_frontend_payload() {
    native_drop_completion_for(None, NativeDropChange::Target);
}

#[cfg(feature = "nested")]
#[test]
fn native_drop_keyboard_focus_change_invalidates_queued_frontend_payload() {
    native_drop_completion_for(None, NativeDropChange::Focus);
}

#[cfg(feature = "nested")]
enum NativeDropChange { Suspend, Output, Target, Focus, Cancel }

#[cfg(feature = "nested")]
#[test]
fn cancelling_an_accepted_pointer_drag_cannot_perform_its_drop_and_retires_once() {
    native_drop_completion_for(None, NativeDropChange::Cancel);
}

#[cfg(feature = "nested")]
fn native_drop_completion(accept: Option<bool>) {
    native_drop_completion_for(accept, NativeDropChange::Suspend);
}

#[cfg(feature = "nested")]
fn native_drop_completion_for(accept: Option<bool>, change: NativeDropChange) {
    use crate::native_input::{Editor, client::Event, transfer::{Destination, Payload, Target}};
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    for _ in 0..8 { harness.sync(); }
    let surface = harness.server.windows()[0].surface.clone();
    harness.server.focus_surface(Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.pointer_motion(Some((surface, [20.0, 20.0])), 1).unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    let qh = harness.queue.handle();
    let manager = harness.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(harness.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/plain;charset=utf-8".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    harness.client.drop_payload = "Olá 世界".as_bytes().to_vec();
    device.start_drag(Some(&source), &window.surface, None, harness.client.pointer_serial);
    for _ in 0..4 { harness.sync(); }
    let target = Target { window: 1, output: lince_interface::host::OutputId(1), mapping_revision: None, pointer: [100.0, 100.0],
        destination: Destination::Editor {
            context: Editor { owner: 1, revision: 1, projection: 0, from_ime: false, selection: [0, 0], surrounding: String::new(),
                cursor: 0, anchor: 0, sensitive: false, multiline: false, caret: [0, 0, 1, 20] },
            point: [10.0, 10.0], transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], size: [100.0; 2], scale: 1.0,
            content_origin: [-50.0; 2],
        } };
    harness.server.native_drop_target(Some(target.clone()));
    let native = harness.server.native_surface().unwrap();
    harness.server.pointer_motion(Some((native, [100.0, 100.0])), 3).unwrap();
    for _ in 0..8 { harness.sync(); }
    if matches!(change, NativeDropChange::Cancel) {
        assert!(harness.server.pointer_dragging());
        assert_eq!(harness.server.cancel_pointer_drag(4), vec![272]);
        assert!(harness.server.cancel_pointer_drag(5).is_empty());
        for _ in 0..8 { harness.sync(); }
        assert!(!harness.server.dragging());
        assert!(harness.server.native_file_drag().is_none());
        assert!(!harness.server.native_events().iter().any(|event| matches!(event, Event::Drop(_))));
        assert_eq!(harness.client.drop_cancelled, 1);
        assert_eq!(harness.client.drop_performed, 0);
        assert_eq!(harness.client.drop_finished, 0);
        source.destroy();
        device.release();
        harness.sync();
        assert_eq!(harness.client.drop_cancelled, 1);
        return;
    }
    harness.server.pointer_button(272, false, 4);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let request = loop {
        harness.sync();
        if let Some(request) = harness.server.native_events().into_iter().find_map(|event| match event {
            Event::Drop(request) => Some(request), _ => None,
        }) { break request; }
        assert!(std::time::Instant::now() < deadline, "native drop did not arrive");
        std::thread::yield_now();
    };
    assert_eq!(request.target, target);
    assert_eq!(request.payload, Payload::Text("Olá 世界".into()));
    assert!(!harness.client.drop_send_failed);
    assert_eq!(harness.client.drop_performed, 1);
    assert_eq!(harness.client.drop_finished, 0);
    if let Some(accepted) = accept {
        harness.server.finish_native_drop(request.token, accepted);
    } else {
        match change {
            NativeDropChange::Output => {
                let mut replacement = target.clone();
                replacement.output = lince_interface::host::OutputId(2);
                harness.server.native_drop_target(Some(replacement));
            }
            NativeDropChange::Target => harness.server.native_drop_target(None),
            NativeDropChange::Suspend => harness.server.suspend_native_drop(),
            NativeDropChange::Focus => harness.server.native_focus(),
            NativeDropChange::Cancel => unreachable!("A cancelled drag must return before receiving a drop"),
        }
        assert!(request.cancelled.load(std::sync::atomic::Ordering::Acquire));
        harness.server.finish_native_drop(request.token, true);
    }
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.drop_finished, usize::from(accept == Some(true)));
    assert!(harness.server.native_timeout().is_none());
    source.destroy();
    device.release();
    harness.sync();
}

impl Dispatch<wl_pointer::WlPointer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Button { serial, button, state: button_state, .. } => {
                state.pointer_serial = serial;
                state.pointer_buttons.push((button, button_state == wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)));
                state.button_recipients.push((state.pointer_focus.as_ref().map(|surface| surface.id()), button,
                    button_state == wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)));
            }
            wl_pointer::Event::Enter { serial, surface, surface_x, surface_y, .. } => {
                state.pointer_enter_serial = serial;
                state.pointer_focus = Some(surface);
                state.pointer_positions.push([surface_x, surface_y]);
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if state.pointer_focus.as_ref() == Some(&surface) { state.pointer_focus = None; }
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => state.pointer_positions.push([surface_x, surface_y]),
            _ => {}
        }
    }
}

#[test]
fn layer_shell_popup_grabs_use_the_bar_click_without_keyboard_interactivity() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let layer = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "popup-bar".into(),
            &qh,
            (),
        );
    layer.set_size(0, 30);
    layer.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    surface.commit();
    harness.sync();
    harness.sync();
    let buffer = harness.buffer([640, 30]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    let server_surface = harness.server.layers()[0].surface.clone();
    harness
        .server
        .pointer_motion(Some((server_surface, [12.0, 28.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(120, 80);
    positioner.set_anchor_rect(12, 28, 1, 1);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    let popup_surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&popup_surface, &qh, ());
    let popup = xdg.get_popup(None, &positioner, &qh, ());
    layer.get_popup(&popup);
    popup.grab(
        harness.client.seat.as_ref().unwrap(),
        harness.client.pointer_serial,
    );
    popup_surface.commit();
    harness.sync();
    harness.sync();
    assert!(harness.server.popup_grab_active());
    assert_eq!(harness.client.popup_done, 0);
    let buffer = harness.buffer([120, 80]);
    popup_surface.attach(Some(&buffer), 0, 0);
    popup_surface.commit();
    harness.sync();
    let layers = harness.server.layers();
    assert_eq!(layers[0].popups.len(), 1);
    let mapping = layers[0].popups[0].layer_mapping([0.0; 2]).unwrap();
    assert!(mapping.canvas.origin()[1] + mapping.canvas.size()[1] > 30.0);
}

#[test]
fn activation_requires_recent_owned_input_and_tokens_are_single_use() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _source_buffer = harness.attach(&source, [640, 480]);
    let target = harness.window();
    let _target_buffer = harness.attach(&target, [640, 480]);
    let windows = harness.server.windows();
    harness.server.focus(
        Some(windows[0].id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.set_surface(&source.surface);
    request.commit();
    harness.sync();
    let token = harness.client.tokens.last().unwrap().clone();
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(token.clone(), &target.surface);
    harness.sync();
    assert_eq!(harness.server.activations(), [windows[1].id]);
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(token, &target.surface);
    harness.sync();
    assert!(harness.server.activations().is_empty());
    let invalid = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    invalid.set_serial(0, harness.client.seat.as_ref().unwrap());
    invalid.set_surface(&source.surface);
    invalid.commit();
    harness.sync();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &target.surface,
    );
    harness.sync();
    assert!(harness.server.activations().is_empty());
}

#[test]
fn newer_native_input_revokes_activation_and_surface_hint_is_optional() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _source_buffer = harness.attach(&source, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &source.surface,
    );
    harness.sync();
    assert_eq!(harness.server.activations(), [id]);
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    harness.server.native_input();
    harness.client.activation.as_ref().unwrap().activate(
        harness.client.tokens.last().unwrap().clone(),
        &source.surface,
    );
    harness.sync();
    assert!(harness.server.activations().is_empty());
}

#[test]
fn activation_waits_for_map_and_expired_tokens_or_pending_requests_are_discarded() {
    let mut harness = Harness::new();
    let source = harness.window();
    let _buffer = harness.attach(&source, [640, 480]);
    let target = harness.window();
    let ids: Vec<_> = harness
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect();
    harness
        .server
        .focus(Some(ids[0]), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let qh = harness.queue.handle();
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&qh, ());
    request.set_serial(
        harness.client.key_serial,
        harness.client.seat.as_ref().unwrap(),
    );
    request.commit();
    harness.sync();
    let (token, mut data) = harness
        .server
        .state
        .activation
        .tokens()
        .next()
        .map(|(token, data)| (token.clone(), data.clone()))
        .unwrap();
    harness.server.state.activation.remove_token(&token);
    data.timestamp = Instant::now() - Duration::from_secs(6);
    let expired = harness
        .server
        .state
        .activation
        .create_external_token(data)
        .0
        .to_string();
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(expired, &target.surface);
    harness.sync();
    assert!(harness.server.activations().is_empty());
    assert!(harness.server.state.activations.is_empty());
    for expired in [true, false] {
        let request = harness
            .client
            .activation
            .as_ref()
            .unwrap()
            .get_activation_token(&qh, ());
        request.set_serial(
            harness.client.key_serial,
            harness.client.seat.as_ref().unwrap(),
        );
        request.commit();
        harness.sync();
        harness.client.activation.as_ref().unwrap().activate(
            harness.client.tokens.last().unwrap().clone(),
            &target.surface,
        );
        harness.sync();
        assert!(harness.server.activations().is_empty());
        assert_eq!(harness.server.state.activations.len(), 1);
        if expired {
            harness.server.state.activations.front_mut().unwrap().1 =
                Instant::now() - Duration::from_secs(6);
            assert!(harness.server.activations().is_empty());
            assert!(harness.server.state.activations.is_empty());
        }
    }
    let _target_buffer = harness.attach(&target, [400, 300]);
    assert_eq!(harness.server.activations(), [ids[1]]);
    assert!(harness.server.activations().is_empty());
}

#[test]
fn configure_commit_resize_and_buffer_lifetime() {
    let mut harness = Harness::new();
    let window = harness.window();
    assert_eq!(harness.client.configures, [[640, 480]]);
    assert_eq!(harness.client.serials, 1);
    let before = harness.server.windows().remove(0);
    assert!(!before.mapped);
    assert_eq!(before.title, "A real Wayland window");
    assert_eq!(before.app_id, "social.lince.park.test");
    assert_eq!(before.minimum, [120, 80]);
    assert_eq!(before.maximum, [1200, 900]);
    let _first = harness.attach(&window, [640, 480]);
    let snapshot = harness.server.windows().remove(0);
    assert!(snapshot.mapped);
    assert_eq!(snapshot.nodes[0].content.size, [640, 480]);
    assert_eq!(harness.client.releases, 0);
    harness.server.configure(before.id, [320, 240]).unwrap();
    harness.server.configure(before.id, [320, 240]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.configures, [[640, 480], [320, 240]]);
    let _second = harness.attach(&window, [320, 240]);
    assert_eq!(harness.client.releases, 0);
    snapshot.nodes[0].content.release();
    snapshot.nodes[0].content.release();
    drop(snapshot);
    harness.sync();
    assert_eq!(harness.client.releases, 1);
    let current = harness.server.windows().remove(0);
    current.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 2);
    harness.server.close(before.id);
    harness.sync();
    assert!(harness.client.closed);
}

#[test]
fn unmap_remap_keeps_role_identity_and_destruction_discards_it() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _first = harness.attach(&window, [640, 480]);
    let first = harness.server.windows().remove(0).id;
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    let unmapped = harness.server.windows().remove(0);
    assert_eq!(unmapped.id, first);
    assert!(!unmapped.mapped);
    assert!(unmapped.nodes.is_empty());
    window.surface.commit();
    harness.sync();
    harness.sync();
    let _second = harness.attach(&window, [640, 480]);
    let remapped = harness.server.windows().remove(0);
    assert_eq!(remapped.id, first);
    assert!(remapped.mapped);
    window.role.destroy();
    window.xdg.destroy();
    window.surface.destroy();
    harness.sync();
    assert!(harness.server.windows().is_empty());
    let _new = harness.window();
    assert_ne!(harness.server.windows().remove(0).id, first);
}

#[test]
fn disconnect_reclaims_registry_and_buffer_leases() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let held = harness.server.windows().remove(0);
    let Harness {
        mut server,
        connection,
        queue,
        client,
    } = harness;
    drop((window, connection, queue, client));
    for _ in 0..8 {
        server.dispatch().unwrap();
    }
    assert!(server.windows().is_empty());
    let native: HashSet<_> = server.state.native_surface.iter().cloned().collect();
    assert_eq!(server.state.surfaces, native);
    assert!(server.state.buffers.is_empty());
    assert_eq!(server.clients.load(Ordering::Acquire), 0);
    drop(held);
}

#[test]
fn repeated_buffer_commits_release_only_after_every_gpu_use_finishes() {
    let mut harness = Harness::new();
    let window = harness.window();
    let buffer = harness.attach(&window, [640, 480]);
    let first = harness.server.windows().remove(0);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    harness.sync();
    let second = harness.server.windows().remove(0);
    first.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 0);
    second.nodes[0].content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 1);
    drop((first, second));
    harness.sync();
    assert_eq!(harness.client.releases, 1);
}

#[test]
fn synchronized_subsurface_commits_wait_for_parent_and_keep_offsets() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _root_buffer = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let sub = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    sub.set_position(18, -12);
    let buffer = harness.buffer([80, 60]);
    child.attach(Some(&buffer), 0, 0);
    child.commit();
    harness.sync();
    assert_eq!(harness.server.windows()[0].nodes.len(), 1);
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    assert_eq!(snapshot.nodes.len(), 2);
    assert_eq!(snapshot.nodes[0].origin, [18, -12]);
    assert_eq!(snapshot.nodes[0].content.size, [80, 60]);
    let frame = Arc::new(AtomicBool::new(false));
    child.frame(&qh, frame.clone());
    child.commit();
    window.surface.commit();
    harness.sync();
    assert!(!frame.load(Ordering::Acquire));
    harness.server.presented(&[snapshot.id]);
    harness.sync();
    assert!(frame.load(Ordering::Acquire));
}

#[test]
fn an_invalid_client_does_not_disconnect_an_unrelated_window() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let good = harness.server.windows().remove(0).id;
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    harness.server.insert_client(server_stream).unwrap();
    let connection = Connection::from_socket(client_stream).unwrap();
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let mut client = Client {
        ignore_configure: true,
        ..Default::default()
    };
    connection.display().get_registry(&qh, ());
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let surface = client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let _role = xdg.get_toplevel(&qh, ());
    surface.commit();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let file = tempfile::tempfile().unwrap();
    file.set_len(16).unwrap();
    let pool = client
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), 16, &qh, ());
    let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    assert!(synchronize(&mut harness.server, &connection, &mut queue, &mut client).is_err());
    harness.sync();
    let remaining = harness.server.windows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, good);
    assert!(remaining[0].mapped);
    assert_eq!(harness.server.clients.load(Ordering::Acquire), 1);
}

#[test]
fn surface_mapping_handles_rotation_scale_and_viewport_crop() {
    let turns = [
        (Transform::Normal, [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]),
        (Transform::_90, [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]),
        (Transform::_180, [1.0, 1.0], [-1.0, 0.0], [0.0, -1.0]),
        (Transform::_270, [0.0, 1.0], [0.0, -1.0], [1.0, 0.0]),
        (Transform::Flipped, [1.0, 0.0], [-1.0, 0.0], [0.0, 1.0]),
        (Transform::Flipped90, [0.0, 0.0], [0.0, 1.0], [1.0, 0.0]),
        (Transform::Flipped180, [0.0, 1.0], [1.0, 0.0], [0.0, -1.0]),
        (Transform::Flipped270, [1.0, 1.0], [0.0, -1.0], [-1.0, 0.0]),
    ];
    for (transform, origin, x, y) in turns {
        let layout = SurfaceLayout::new([240, 160], 2, transform, Default::default()).unwrap();
        assert_eq!(layout.uv_origin, origin);
        assert_eq!(layout.uv_x, x);
        assert_eq!(layout.uv_y, y);
        assert!(layout.contains([0.0, 0.0]));
        assert!(!layout.contains(layout.size));
        let size =
            transform.transform_size(smithay::utils::Size::<_, Logical>::from((120.0, 80.0)));
        assert_eq!(layout.size, [size.w, size.h]);
    }
    let crop = ViewportCachedState {
        src: Some(Rectangle::new((30.0, 10.0).into(), (60.0, 40.0).into())),
        dst: Some((180, 120).into()),
    };
    let layout = SurfaceLayout::new([240, 160], 2, Transform::Normal, crop).unwrap();
    assert_eq!(layout.size, [180.0, 120.0]);
    assert_eq!(layout.uv_origin, [0.25, 0.125]);
    assert_eq!(layout.uv_x, [0.5, 0.0]);
    assert_eq!(layout.uv_y, [0.0, 0.5]);
    assert!(SurfaceLayout::new([60, 40], 2, Transform::Normal, crop).is_none());
}

#[test]
fn layer_shell_bars_own_their_space_and_unmap_restores_it() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let role = harness
        .client
        .layer_shell
        .as_ref()
        .unwrap()
        .get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "park-test-bar".into(),
            &qh,
            (),
        );
    role.set_size(0, 30);
    role.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    role.set_exclusive_zone(30);
    surface.attach(None, 0, 0);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures, [[640, 30]]);
    let buffer = harness.buffer([640, 30]);
    let canvas_revision = harness.server.window_revision();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(harness.server.window_revision(), canvas_revision);
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 30).into(), (640, 450).into())
    );
    assert_eq!(harness.server.layers()[0].origin, [0, 0]);
    role.set_exclusive_zone(42);
    role.set_size(0, 42);
    surface.commit();
    harness.sync();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 42).into(), (640, 438).into())
    );
    harness.server.resize([800, 600]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(harness.client.layer_configures.last(), Some(&[800, 42]));
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 42).into(), (800, 558).into())
    );
    surface.attach(None, 0, 0);
    surface.commit();
    harness.sync();
    assert!(harness.server.layers().is_empty());
    assert_eq!(
        harness.server.usable(),
        Rectangle::from_size((800, 600).into())
    );
    role.set_size(0, 28);
    role.set_exclusive_zone(28);
    surface.commit();
    harness.sync();
    harness.sync();
    let buffer = harness.buffer([800, 28]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 28).into(), (800, 572).into())
    );
    role.destroy();
    surface.destroy();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::from_size((800, 600).into())
    );
}

#[test]
fn changing_scale_without_attaching_a_buffer_validates_current_dimensions() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [641, 480]);
    window.surface.set_buffer_scale(2);
    window.surface.commit();
    assert!(
        synchronize(
            &mut harness.server,
            &harness.connection,
            &mut harness.queue,
            &mut harness.client
        )
        .is_err()
    );
    harness.server.dispatch().unwrap();
    assert!(harness.server.windows().is_empty());
}

#[test]
fn consumed_desktop_keys_do_not_reach_the_client_or_leave_stuck_modifiers() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.sync();
    harness.server.keyboard(125, true, 1, |_| false).unwrap();
    let event = harness
        .server
        .keyboard(35, true, 2, |event| event.modifiers.logo)
        .unwrap();
    assert!(event.modifiers.logo);
    assert_eq!(event.text.as_deref(), Some("h"));
    harness.server.keyboard(35, false, 3, |_| true).unwrap();
    let event = harness.server.keyboard(125, false, 4, |_| false).unwrap();
    assert!(!event.modifiers.logo);
    harness.server.keyboard(30, true, 5, |_| false).unwrap();
    harness.server.keyboard(30, false, 6, |_| false).unwrap();
    harness.sync();
    assert_eq!(
        harness.client.keys,
        [(125, true), (125, false), (30, true), (30, false)]
    );
    assert!(harness.server.keyboard(768, true, 7, |_| false).is_err());
    harness.server.clear_input(8);
    harness.server.keyboard(48, true, 9, |_| false).unwrap();
    harness.sync();
    assert_eq!(harness.client.keys.len(), 4);
}

#[test]
fn hit_testing_obeys_surface_geometry_subsurface_stacking_and_input_regions() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _root = harness.attach(&window, [640, 480]);
    let qh = harness.queue.handle();
    let compositor = harness.client.compositor.as_ref().unwrap();
    let child = compositor.create_surface(&qh, ());
    let sub = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    sub.set_position(20, 30);
    let empty = compositor.create_region(&qh, ());
    child.set_input_region(Some(&empty));
    let buffer = harness.buffer([80, 60]);
    child.attach(Some(&buffer), 0, 0);
    child.commit();
    window.xdg.set_window_geometry(10, 20, 600, 400);
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    assert_eq!(
        snapshot.bounds(),
        Some(Rectangle::new((10.0, 20.0).into(), (600.0, 400.0).into()))
    );
    assert_eq!(snapshot.hit_content([15.0, 15.0]).unwrap().1, [25.0, 35.0]);
    assert_eq!(
        snapshot.hit_content([15.0, 15.0]).unwrap().0,
        snapshot.surface
    );
    assert!(snapshot.hit_content([600.0, 0.0]).is_none());
    child.set_input_region(None);
    child.commit();
    window.surface.commit();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    let (target, point) = snapshot.hit_content([15.0, 15.0]).unwrap();
    assert_ne!(target, snapshot.surface);
    assert_eq!(point, [5.0, 5.0]);
}

#[test]
fn destroying_the_buffer_proxy_keeps_committed_storage_available_until_import() {
    let mut harness = Harness::new();
    let window = harness.window();
    let buffer = harness.attach(&window, [640, 480]);
    buffer.destroy();
    harness.sync();
    let snapshot = harness.server.windows().remove(0);
    let node = &snapshot.nodes[0];
    assert!(!node.content.buffer().is_alive());
    let size = smithay::wayland::shm::with_buffer_contents(node.content.buffer(), |_, _, data| {
        [data.width, data.height]
    })
    .unwrap();
    assert_eq!(size, [640, 480]);
    node.content.release();
    harness.sync();
    assert_eq!(harness.client.releases, 0);
}

#[test]
fn presentation_acknowledges_only_callbacks_belonging_to_the_drawn_snapshot() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [640, 480]);
    let first = Arc::new(AtomicBool::new(false));
    window.surface.frame(&harness.queue.handle(), first.clone());
    window.surface.commit();
    harness.sync();
    let drawn = harness.server.windows().remove(0);
    let second = Arc::new(AtomicBool::new(false));
    window
        .surface
        .frame(&harness.queue.handle(), second.clone());
    window.surface.commit();
    harness.sync();
    let pending = harness.server.windows().remove(0);
    harness.server.presented_nodes(&drawn.nodes);
    harness.sync();
    assert!(first.load(Ordering::Acquire));
    assert!(!second.load(Ordering::Acquire));
    harness.server.presented_nodes(&pending.nodes);
    harness.sync();
    assert!(second.load(Ordering::Acquire));
    assert!(harness.server.state.callbacks.is_empty());
}

#[test]
fn extreme_input_region_coordinates_do_not_overflow_during_hit_testing() {
    use smithay::wayland::compositor::RectangleKind;
    let mut region = RegionAttributes::default();
    region.rects.push((
        RectangleKind::Add,
        Rectangle::new((1, 1).into(), (i32::MAX, i32::MAX).into()),
    ));
    assert!(input_contains(&region, [2.0, 2.0]));
    region.rects.push((
        RectangleKind::Subtract,
        Rectangle::new((i32::MAX, i32::MAX).into(), (i32::MAX, i32::MAX).into()),
    ));
    assert!(input_contains(&region, [2.0, 2.0]));
    assert!(!input_contains(&region, [0.0, 0.0]));
}

#[test]
fn buffer_memory_limit_rejects_the_new_client_and_keeps_existing_windows() {
    let mut good = Harness::new();
    let mut windows = Vec::new();
    let mut buffers = Vec::new();
    for _ in 0..4 {
        let window = good.window();
        buffers.push(good.attach(&window, [16384, 4096]));
        windows.push(window);
    }
    let identities: Vec<_> = good
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect();
    let Harness {
        server,
        connection,
        queue,
        client,
    } = good;
    let mut bad = Harness::connect(server);
    let window = bad.window();
    let buffer = bad.buffer([16384, 4096]);
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.commit();
    assert!(
        synchronize(
            &mut bad.server,
            &bad.connection,
            &mut bad.queue,
            &mut bad.client
        )
        .is_err()
    );
    let remaining = bad.server.windows();
    assert_eq!(remaining.len(), 4);
    assert!(
        remaining
            .iter()
            .all(|window| window.mapped && identities.contains(&window.id))
    );
    assert_eq!(bad.server.clients.load(Ordering::Acquire), 1);
    drop((connection, queue, client, windows, buffers));
}

#[test]
fn top_and_bottom_bars_reserve_independent_space_through_resize_and_removal() {
    let mut harness = Harness::new();
    let qh = harness.queue.handle();
    let mut bars = Vec::new();
    for (edge, layer, height) in [
        (
            zwlr_layer_surface_v1::Anchor::Top,
            zwlr_layer_shell_v1::Layer::Top,
            32,
        ),
        (
            zwlr_layer_surface_v1::Anchor::Bottom,
            zwlr_layer_shell_v1::Layer::Bottom,
            28,
        ),
    ] {
        let surface = harness
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let role = harness
            .client
            .layer_shell
            .as_ref()
            .unwrap()
            .get_layer_surface(&surface, None, layer, "park-bar".into(), &qh, ());
        role.set_size(0, height);
        role.set_anchor(
            edge | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right,
        );
        role.set_exclusive_zone(height as i32);
        surface.commit();
        harness.sync();
        harness.sync();
        let buffer = harness.buffer([640, height as i32]);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        harness.sync();
        bars.push((surface, role, buffer));
    }
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (640, 420).into())
    );
    harness.server.resize([900, 600]).unwrap();
    harness.sync();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (900, 540).into())
    );
    let (surface, role, _) = bars.pop().unwrap();
    role.destroy();
    surface.destroy();
    harness.sync();
    assert_eq!(
        harness.server.usable(),
        Rectangle::new((0, 32).into(), (900, 568).into())
    );
}
