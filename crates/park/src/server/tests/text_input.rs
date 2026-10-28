use super::*;
use wayland_protocols::wp::text_input::zv3::client::{zwp_text_input_manager_v3::ZwpTextInputManagerV3 as Manager, zwp_text_input_v3::{self, ZwpTextInputV3 as TextInput}};

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<Manager>,
    pub(super) entered: std::collections::HashMap<wayland_client::backend::ObjectId, wl_surface::WlSurface>,
    pub(super) enters: Vec<(wayland_client::backend::ObjectId, wl_surface::WlSurface)>,
    pub(super) leaves: Vec<(wayland_client::backend::ObjectId, wl_surface::WlSurface)>,
    pub(super) commits: Vec<(wayland_client::backend::ObjectId, Option<String>)>,
    pub(super) preedits: Vec<(wayland_client::backend::ObjectId, Option<String>)>,
    pub(super) done: Vec<(wayland_client::backend::ObjectId, u32)>,
}

pub(super) fn bind(client: &mut Client, registry: &wl_registry::WlRegistry, name: u32, qh: &QueueHandle<Client>) {
    client.text_input.manager = Some(registry.bind(name, 1, qh, ()));
}

delegate_noop!(Client: ignore Manager);

impl Dispatch<TextInput, ()> for Client {
    fn event(client: &mut Self, proxy: &TextInput, event: zwp_text_input_v3::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_text_input_v3::Event::Enter { surface, .. } => {
                client.text_input.entered.insert(proxy.id(), surface.clone());
                client.text_input.enters.push((proxy.id(), surface));
            }
            zwp_text_input_v3::Event::Leave { surface, .. } => {
                if client.text_input.entered.get(&proxy.id()) == Some(&surface) { client.text_input.entered.remove(&proxy.id()); }
                client.text_input.leaves.push((proxy.id(), surface));
            }
            zwp_text_input_v3::Event::CommitString { text, .. } => client.text_input.commits.push((proxy.id(), text)),
            zwp_text_input_v3::Event::PreeditString { text, .. } => client.text_input.preedits.push((proxy.id(), text)),
            zwp_text_input_v3::Event::Done { serial, .. } => client.text_input.done.push((proxy.id(), serial)),
            _ => {}
        }
    }
}

#[cfg(feature = "nested")]
pub(super) fn input(client: &Client, qh: &QueueHandle<Client>) -> TextInput {
    client.text_input.manager.as_ref().unwrap().get_text_input(client.seat.as_ref().unwrap(), qh, ())
}

#[cfg(feature = "nested")]
pub(super) fn enable(input: &TextInput, rectangle: [i32; 4]) {
    input.enable(); input.set_surrounding_text("native caret fixture".into(), 0, 0);
    input.set_cursor_rectangle(rectangle[0], rectangle[1], rectangle[2], rectangle[3]); input.commit();
}

#[cfg(feature = "nested")]
#[test]
fn layer_caret_popups_snapshot_without_reentering_the_layer_map_and_follow_caret_changes() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    let qh = harness.queue.handle();
    let surface = harness.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = harness.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, None,
        zwlr_layer_shell_v1::Layer::Top, "caret-layer".into(), &qh, ());
    role.set_size(320, 120);
    role.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    role.set_margin(30, 0, 0, 40);
    role.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand);
    surface.commit(); harness.sync(); harness.sync();
    let buffer = harness.buffer([320, 120]);
    surface.attach(Some(&buffer), 0, 0); surface.commit(); harness.sync();
    let layer_surface = harness.server.layers()[0].surface.clone();
    harness.server.focus_surface(Some(layer_surface), smithay::utils::SERIAL_COUNTER.next_serial()); harness.sync();
    let input = input(&harness.client, &qh); harness.sync();
    let method = harness.client.input_method.as_ref().unwrap().get_input_method(harness.client.seat.as_ref().unwrap(), &qh, ()); harness.sync();
    enable(&input, [20, 30, 2, 20]); harness.sync();
    assert!(harness.client.input_active);
    let popup_surface = harness.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let popup = method.get_input_popup_surface(&popup_surface, &qh, ());
    let popup_buffer = harness.buffer([64, 32]);
    popup_surface.attach(Some(&popup_buffer), 0, 0); popup_surface.commit(); harness.sync();
    let layers = harness.server.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].origin, [40, 30]);
    assert_eq!(layers[0].popups.len(), 1);
    assert_eq!(layers[0].popups[0].origin, [20, 50]);
    input.set_cursor_rectangle(50, 60, 2, 20); input.commit(); harness.sync();
    assert_eq!(harness.server.layers()[0].popups[0].origin, [50, 80]);
    input.disable(); input.commit(); harness.sync();
    assert!(harness.server.layers()[0].popups.is_empty());
    popup.destroy(); popup_surface.destroy(); popup_buffer.destroy(); method.destroy();
    input.destroy(); role.destroy(); surface.destroy(); buffer.destroy(); harness.sync();
    assert!(harness.server.layers().is_empty());
}

#[cfg(feature = "nested")]
#[test]
fn native_client_caret_is_committed_owned_and_retired_without_an_ime_provider_or_popup() {
    use lince_interface::host::{Rectangle, magnification::Caret};
    let mut harness = Harness::new();
    let first = harness.window(); let _first = harness.attach(&first, [320, 240]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window(); let _second = harness.attach(&second, [320, 240]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    harness.server.focus(Some(first_id), smithay::utils::SERIAL_COUNTER.next_serial()); harness.sync();
    let input = input(&harness.client, &harness.queue.handle()); harness.sync();
    assert_eq!(harness.client.text_input.entered.get(&input.id()), Some(&first.surface));
    input.enable(); input.set_cursor_rectangle(20, 40, 2, 18); harness.sync();
    assert!(harness.server.magnification_caret().is_none(), "Uncommitted metadata became a compositor caret");
    input.commit(); harness.sync();
    assert_eq!(harness.server.magnification_caret(), Some(Caret::Application { surface: first_id, rectangle: Rectangle::new([20.0, 40.0], [2.0, 18.0]).unwrap() }));
    let enters = harness.client.text_input.enters.len();
    let inactive = self::input(&harness.client, &harness.queue.handle()); harness.sync();
    assert_eq!(harness.client.text_input.enters.len(), enters + 1);
    assert_eq!(harness.client.text_input.enters.last(), Some(&(inactive.id(), first.surface.clone())));
    enable(&inactive, [90, 80, 3, 19]); harness.sync();
    assert_eq!(harness.server.magnification_caret(), Some(Caret::Application { surface: first_id, rectangle: Rectangle::new([20.0, 40.0], [2.0, 18.0]).unwrap() }));
    inactive.destroy(); harness.sync();
    assert!(harness.server.magnification_caret().is_some());
    input.disable(); input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_none());
    input.enable(); input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_none(), "Re-enable reused a previous field's rectangle");
    input.set_cursor_rectangle(30, 50, 0, 20); input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_none());
    input.set_cursor_rectangle(30, 50, 2, 20); input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_some());
    input.enable(); input.set_cursor_rectangle(90, 90, 2, 20); harness.sync();
    harness.server.focus(Some(second_id), smithay::utils::SERIAL_COUNTER.next_serial()); harness.sync();
    assert_eq!(harness.client.text_input.entered.get(&input.id()), Some(&second.surface));
    assert!(harness.server.magnification_caret().is_none());
    input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_none(), "Uncommitted metadata from the departed surface acquired its replacement");
    input.set_cursor_rectangle(90, 90, 2, 20); input.commit(); harness.sync(); assert!(harness.server.magnification_caret().is_none());
    enable(&input, [60, 70, 3, 21]); harness.sync();
    assert_eq!(harness.server.magnification_caret(), Some(Caret::Application { surface: second_id, rectangle: Rectangle::new([60.0, 70.0], [3.0, 21.0]).unwrap() }));
    input.destroy(); harness.sync(); assert!(harness.server.magnification_caret().is_none());
    assert!(harness.server.native_popups().is_empty());
}

#[cfg(feature = "nested")]
#[test]
fn provider_join_loss_and_unavailable_peer_destruction_preserve_the_real_text_input_owner_and_reenable_without_a_popup() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    let window = harness.window(); let _buffer = harness.attach(&window, [320, 240]);
    let id = harness.server.windows()[0].id;
    harness.server.focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial()); harness.sync();
    let input = input(&harness.client, &harness.queue.handle()); harness.sync();
    enable(&input, [20, 30, 2, 20]); harness.sync(); assert!(harness.server.magnification_caret().is_some());
    let manager = harness.client.input_method.as_ref().unwrap();
    let seat = harness.client.seat.as_ref().unwrap();
    let first = manager.get_input_method(seat, &harness.queue.handle(), ()); harness.sync();
    assert!(harness.server.magnification_caret().is_none());
    assert_eq!(harness.client.text_input.entered.get(&input.id()), Some(&window.surface));
    enable(&input, [40, 50, 3, 18]); harness.sync();
    let current = harness.server.magnification_caret(); assert!(current.is_some()); assert!(harness.client.input_active);
    let second = harness.client.input_method.as_ref().unwrap().get_input_method(harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), ()); harness.sync();
    assert_eq!(harness.client.input_unavailable, [second.id()]);
    assert_eq!(harness.server.magnification_caret(), current);
    second.destroy(); harness.sync(); assert_eq!(harness.server.magnification_caret(), current);
    first.commit_string("owned".into()); first.commit(harness.client.input_serial); harness.sync();
    assert_eq!(harness.client.text_input.commits.last(), Some(&(input.id(), Some("owned".into()))));
    let surface = harness.client.compositor.as_ref().unwrap().create_surface(&harness.queue.handle(), ());
    let popup = first.get_input_popup_surface(&surface, &harness.queue.handle(), ());
    let revision = harness.server.window_revision();
    let popup_buffer = harness.buffer([64, 32]); surface.attach(Some(&popup_buffer), 0, 0); surface.commit(); harness.sync();
    assert_eq!(harness.server.windows()[0].popups.len(), 1);
    assert!(harness.server.window_revision() > revision, "The committed input-method popup must reach window snapshot consumers");
    let revision = harness.server.window_revision();
    let repaint = harness.buffer([64, 32]); surface.attach(Some(&repaint), 0, 0); surface.damage_buffer(0, 0, 64, 32); surface.commit(); harness.sync();
    assert!(harness.server.window_revision() > revision, "Repainting the input-method popup must invalidate its window snapshot");
    let revision = harness.server.window_revision();
    input.set_cursor_rectangle(50, 60, 3, 18); input.commit(); harness.sync();
    assert!(harness.server.window_revision() > revision, "Moving the input-method popup must invalidate its window snapshot");
    assert_eq!(harness.server.windows()[0].popups[0].origin, [50, 78]);
    let old_grab = first.grab_keyboard(&harness.queue.handle(), ()); harness.sync(); assert!(harness.server.input_method_grabbed());
    first.destroy(); harness.sync(); assert!(harness.server.magnification_caret().is_none());
    assert!(!harness.server.input_method_grabbed());
    enable(&input, [60, 70, 2, 22]); harness.sync(); assert!(harness.server.magnification_caret().is_some());
    assert!(harness.server.windows()[0].popups.is_empty(), "A destroyed provider's still-live popup returned when the client caret re-enabled");
    assert!(harness.server.native_popups().is_empty());
    harness.client.keys.clear();
    harness.server.keyboard(30, true, 1, |_| false).unwrap(); harness.server.keyboard(30, false, 2, |_| false).unwrap(); harness.sync();
    assert_eq!(harness.client.keys, [(30, true), (30, false)]);
    let fresh = harness.client.input_method.as_ref().unwrap().get_input_method(harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), ()); harness.sync();
    enable(&input, [70, 80, 2, 22]); harness.sync();
    let fresh_grab = fresh.grab_keyboard(&harness.queue.handle(), ()); harness.sync(); assert!(harness.server.input_method_grabbed());
    old_grab.release(); harness.sync(); assert!(harness.server.input_method_grabbed(), "A retired provider's grab released its replacement");
    fresh_grab.release(); fresh.destroy(); harness.sync(); assert!(!harness.server.input_method_grabbed());
    popup.destroy(); surface.destroy(); popup_buffer.destroy(); repaint.destroy(); harness.sync();
}
