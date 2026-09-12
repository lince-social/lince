use crate::{instance::InstanceEvents, wake::WakeSignal};
use bevy::{
    prelude::*,
    ui_widgets::Activate,
    window::{PrimaryWindow, WindowCloseRequested},
};
use std::sync::mpsc;

#[derive(Clone, Copy, Debug)]
pub enum TrayMessage {
    Show,
    Quit,
    Available(bool),
}

#[derive(Resource, Clone)]
pub struct TraySender {
    sender: mpsc::Sender<TrayMessage>,
    wake: WakeSignal,
}

impl TraySender {
    pub fn send(&self, message: TrayMessage) {
        if self.sender.send(message).is_ok() {
            self.wake.ring();
        }
    }
}

struct TrayState {
    receiver: mpsc::Receiver<TrayMessage>,
    saved_window: Option<(Entity, Window)>,
}

#[derive(Resource, Clone, Copy)]
pub struct InterfaceWindowSettings {
    pub close_suspends: bool,
}

impl Default for InterfaceWindowSettings {
    fn default() -> Self {
        Self {
            close_suspends: true,
        }
    }
}

#[derive(Component)]
pub struct QuitLince;

pub struct TrayPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandleTray;
impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InterfaceWindowSettings>()
            .add_systems(Startup, setup)
            .add_systems(Update, (close_requests, drain).chain().in_set(HandleTray))
            .add_observer(
                |event: On<Activate>,
                 buttons: Query<&QuitLince>,
                 mut exit: MessageWriter<AppExit>| {
                    if buttons.contains(event.entity) {
                        exit.write(AppExit::Success);
                    }
                },
            );
    }
}

fn setup(world: &mut World) {
    let wake = world.resource::<WakeSignal>().clone();
    if let Some(instance) = world.get_non_send::<InstanceEvents>() {
        instance.attach(wake.clone());
    }
    let (sender, receiver) = mpsc::channel();
    let sender = TraySender { sender, wake };
    world.insert_non_send(TrayState {
        receiver,
        saved_window: None,
    });
    world.insert_resource(sender.clone());
    start_platform(world, sender);
}

fn close_requests(
    mut requests: MessageReader<WindowCloseRequested>,
    settings: Res<InterfaceWindowSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut state: NonSendMut<TrayState>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(mut window) = windows.get_mut(request.window) else {
            continue;
        };
        if settings.close_suspends {
            state.saved_window = Some((request.window, window.clone()));
            commands
                .entity(request.window)
                .remove::<(Window, bevy::window::RawHandleWrapper)>();
        } else {
            window.visible = false;
            window.set_minimized(true);
        }
    }
}

fn drain(world: &mut World) {
    let Some(mut state) = world.remove_non_send::<TrayState>() else {
        return;
    };
    let mut messages = Vec::new();
    if world
        .get_non_send::<InstanceEvents>()
        .is_some_and(InstanceEvents::take_show)
    {
        messages.push(TrayMessage::Show);
    }
    while let Ok(message) = state.receiver.try_recv() {
        messages.push(message);
    }
    for message in messages {
        match message {
            TrayMessage::Available(_) => {}
            TrayMessage::Quit => {
                world.write_message(AppExit::Success);
            }
            TrayMessage::Show => {
                if let Some((entity, mut window)) = state.saved_window.take() {
                    window.visible = true;
                    window.set_minimized(false);
                    window.focused = true;
                    world.entity_mut(entity).insert(window);
                } else {
                    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
                    for mut window in windows.iter_mut(world) {
                        window.visible = true;
                        window.set_minimized(false);
                        window.focused = true;
                    }
                }
            }
        }
    }
    world.insert_non_send(state);
}

fn icon_rgba() -> Result<image::RgbaImage, image::ImageError> {
    image::load_from_memory(include_bytes!("../../../assets/logo/black_in_white.png")).map(|icon| {
        icon.resize(32, 32, image::imageops::FilterType::Lanczos3)
            .into_rgba8()
    })
}

#[cfg(target_os = "linux")]
struct LinuxTray {
    sender: TraySender,
    icon: ksni::Icon,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "lince".into()
    }
    fn title(&self) -> String {
        "Lince".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.sender.send(TrayMessage::Show);
    }
    fn watcher_online(&self) {
        self.sender.send(TrayMessage::Available(true));
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        self.sender.send(TrayMessage::Available(false));
        true
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "Show Lince".into(),
                activate: Box::new(|tray: &mut Self| tray.sender.send(TrayMessage::Show)),
                ..default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Quit Lince".into(),
                activate: Box::new(|tray: &mut Self| tray.sender.send(TrayMessage::Quit)),
                ..default()
            }
            .into(),
        ]
    }
}

#[cfg(target_os = "linux")]
struct LinuxService(tokio::task::JoinHandle<()>);
#[cfg(target_os = "linux")]
impl Drop for LinuxService {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(target_os = "linux")]
struct LinuxHandle(ksni::Handle<LinuxTray>);
#[cfg(target_os = "linux")]
impl Drop for LinuxHandle {
    fn drop(&mut self) {
        drop(self.0.shutdown());
    }
}

#[cfg(target_os = "linux")]
fn start_platform(world: &mut World, sender: TraySender) {
    use ksni::TrayMethods;
    let icon = match icon_rgba() {
        Ok(icon) => icon,
        Err(error) => {
            warn!("Could not load tray icon: {error}");
            return;
        }
    };
    let argb = icon
        .pixels()
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect();
    let tray = LinuxTray {
        sender: sender.clone(),
        icon: ksni::Icon {
            width: icon.width() as i32,
            height: icon.height() as i32,
            data: argb,
        },
    };
    let task = tokio::spawn(async move {
        match tray.spawn().await {
            Ok(handle) => {
                let _handle = LinuxHandle(handle);
                sender.send(TrayMessage::Available(true));
                std::future::pending::<()>().await;
            }
            Err(error) => {
                warn!("Tray unavailable; closing Lince will quit: {error}");
                sender.send(TrayMessage::Available(false));
            }
        }
    });
    world.insert_non_send(LinuxService(task));
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct DesktopTray {
    _icon: tray_icon::TrayIcon,
    _menu: muda::Menu,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn start_platform(world: &mut World, sender: TraySender) {
    let result = (|| -> Result<DesktopTray, Box<dyn std::error::Error>> {
        let menu = muda::Menu::new();
        let show = muda::MenuItem::new("Show Lince", true, None);
        let quit = muda::MenuItem::new("Quit Lince", true, None);
        menu.append_items(&[&show, &quit])?;
        let show_id = show.id().clone();
        let quit_id = quit.id().clone();
        let menu_sender = sender.clone();
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            if event.id == show_id {
                menu_sender.send(TrayMessage::Show);
            }
            if event.id == quit_id {
                menu_sender.send(TrayMessage::Quit);
            }
        }));
        let click_sender = sender.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
            ) {
                click_sender.send(TrayMessage::Show);
            }
        }));
        let icon = icon_rgba()?;
        let icon =
            tray_icon::Icon::from_rgba(icon.clone().into_raw(), icon.width(), icon.height())?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_tooltip("Lince")
            .with_icon(icon)
            .with_menu(Box::new(menu.clone()))
            .build()?;
        Ok(DesktopTray {
            _icon: tray,
            _menu: menu,
        })
    })();
    match result {
        Ok(tray) => {
            world.insert_non_send(tray);
            sender.send(TrayMessage::Available(true));
        }
        Err(error) => {
            warn!("Tray unavailable: {error}");
            sender.send(TrayMessage::Available(false));
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn start_platform(_: &mut World, sender: TraySender) {
    sender.send(TrayMessage::Available(false));
}

pub(crate) mod tests {
    use super::*;

    fn fixture() -> (App, Entity, TraySender) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_message::<WindowCloseRequested>()
            .add_message::<AppExit>()
            .add_systems(Update, (close_requests, drain).chain())
            .insert_resource(WakeSignal::new(|| {}));
        let (sender, receiver) = mpsc::channel();
        let sender = TraySender {
            sender,
            wake: WakeSignal::new(|| {}),
        };
        app.insert_non_send(TrayState {
            receiver,
            saved_window: None,
        })
        .insert_resource(InterfaceWindowSettings::default());
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow, Name::new("preserved")))
            .id();
        (app, window, sender)
    }

    #[cfg_attr(test, test)]
    fn close_request_suspends_the_window_until_the_tray_restores_it() {
        let (mut app, window, sender) = fixture();
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        assert!(app.world().get::<Window>(window).is_none());
        assert_eq!(app.should_exit(), None);
        sender.send(TrayMessage::Show);
        app.update();
        assert!(app.world().get::<Window>(window).unwrap().visible);
        assert_eq!(
            app.world().get::<Name>(window).unwrap().as_str(),
            "preserved"
        );
        sender.send(TrayMessage::Quit);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
    }

    #[cfg_attr(test, test)]
    fn disabling_close_suspension_minimizes_without_closing_the_window() {
        let (mut app, window, _) = fixture();
        app.world_mut()
            .resource_mut::<InterfaceWindowSettings>()
            .close_suspends = false;
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        let window = app.world().get::<Window>(window).unwrap();
        assert!(!window.visible);
        assert_eq!(app.should_exit(), None);
    }

    crate::laboratory_cases! {
        close_request_suspends_the_window_until_the_tray_restores_it,
        disabling_close_suspension_minimizes_without_closing_the_window,
    }
}
