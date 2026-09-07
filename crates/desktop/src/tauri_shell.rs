use crate::runtime;
use std::env;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, image::Image};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::{
    WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[cfg(any(target_os = "macos", windows))]
use tauri_plugin_autostart::ManagerExt;

const MAIN_WINDOW_LABEL: &str = "main";
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/logo/black_in_white.png");

pub fn run(tokio_runtime: tokio::runtime::Runtime, args: Vec<String>, listen_addr: String) {
    tauri::async_runtime::set(tokio_runtime.handle().clone());
    let builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        restore_main_window(app);
    }));
    let builder = builder.setup(move |app| {
        #[cfg(any(target_os = "macos", windows))]
        app.handle().plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--desktop-autostart"]),
        ))?;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        install_tray(app)?;
        let handle = app.handle().clone();
        let autostart_launch = args.iter().any(|arg| arg == "--desktop-autostart");
        tauri::async_runtime::spawn(async move {
            match runtime::start_desktop_server(listen_addr).await {
                Ok(runtime) => {
                    eprintln!("Lince desktop serving {}", runtime.url);
                    #[cfg(any(target_os = "macos", windows))]
                    if let Err(error) = sync_autostart(&handle, runtime.start_on_login) {
                        eprintln!("Failed to sync Lince desktop autostart: {error}");
                    }
                    let should_open_window =
                        !(autostart_launch && runtime.start_on_login && runtime.start_silent);
                    if should_open_window
                        && let Err(error) = open_main_window(&handle, &runtime.url)
                    {
                        eprintln!("Failed to open Lince desktop window: {error}");
                    }
                }
                Err(error) => eprintln!("Failed to start Lince desktop server: {error}"),
            }
        });
        Ok(())
    });
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.on_window_event(|window, event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });
    builder
        .run(tauri::generate_context!())
        .expect("error while running Lince desktop application");
    drop(tokio_runtime);
}

#[cfg(any(target_os = "macos", windows))]
fn sync_autostart(app: &tauri::AppHandle, enabled: bool) -> tauri::Result<()> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|error| tauri::Error::Anyhow(anyhow::anyhow!(error)))?;
    } else if autolaunch
        .is_enabled()
        .map_err(|error| tauri::Error::Anyhow(anyhow::anyhow!(error)))?
    {
        autolaunch
            .disable()
            .map_err(|error| tauri::Error::Anyhow(anyhow::anyhow!(error)))?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn install_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Lince", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app_icon()?;
    TrayIconBuilder::new()
        .tooltip("Lince")
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => restore_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                restore_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn open_main_window(app: &tauri::AppHandle, url: &str) -> tauri::Result<()> {
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        restore_main_window(app);
        return Ok(());
    }
    let url = url
        .parse()
        .map(WebviewUrl::External)
        .map_err(|error| tauri::Error::Anyhow(anyhow::anyhow!(error)))?;
    #[cfg(debug_assertions)]
    {
        let open_devtools = env::var_os("LINCE_DESKTOP_DEVTOOLS").is_some();
        let window = WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, url)
            .icon(app_icon()?)?
            .title("Lince")
            .inner_size(1440.0, 960.0)
            .min_inner_size(980.0, 680.0)
            .devtools(true)
            .on_page_load(move |window, payload| {
                if open_devtools && payload.event() == tauri::webview::PageLoadEvent::Finished {
                    eprintln!("Opening Lince desktop WebKit devtools");
                    window.open_devtools();
                }
            })
            .build()?;
        if open_devtools {
            eprintln!("Opening Lince desktop WebKit devtools");
            window.open_devtools();
        }
    }
    #[cfg(not(debug_assertions))]
    {
        WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, url)
            .icon(app_icon()?)?
            .title("Lince")
            .inner_size(1440.0, 960.0)
            .min_inner_size(980.0, 680.0)
            .devtools(false)
            .build()?;
    }
    Ok(())
}

fn app_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(APP_ICON_PNG)
}

fn restore_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
