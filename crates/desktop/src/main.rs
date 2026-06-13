#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap_config;
mod runtime;

#[cfg(any(target_os = "macos", windows))]
use std::time::Duration;
use std::{env, io::Error};
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::PageLoadEvent,
};
#[cfg(any(target_os = "macos", windows))]
use tauri_plugin_autostart::ManagerExt;
use utils::desktop_setup::{DesktopInstallSetup, detected_language_default, write_staged_setup};

const MAIN_WINDOW_LABEL: &str = "main";
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/black_in_white.png");

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| arg == "--stage-desktop-install-setup")
    {
        if let Err(error) = stage_desktop_install_setup(&args) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .setup(|app| {
            #[cfg(any(target_os = "macos", windows))]
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--desktop-autostart"]),
            ))?;

            install_tray(app)?;
            let handle = app.handle().clone();
            let autostart_launch = env::args().any(|arg| arg == "--desktop-autostart");
            tauri::async_runtime::spawn(async move {
                match runtime::start_desktop_server().await {
                    Ok(runtime) => {
                        eprintln!("Lince desktop serving {}", runtime.url);

                        #[cfg(any(target_os = "macos", windows))]
                        if let Err(error) = sync_autostart(&handle, runtime.start_on_login) {
                            eprintln!("Failed to sync Lince desktop autostart: {error}");
                        }

                        #[cfg(any(target_os = "macos", windows))]
                        spawn_autostart_sync_loop(handle.clone(), runtime.services.clone());

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
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Lince desktop application");
}

fn stage_desktop_install_setup(args: &[String]) -> Result<(), Error> {
    let auth_enabled = has_arg(args, "--auth-enabled");
    let initial_admin_password = arg_value(args, "--initial-admin-password");
    if auth_enabled
        && initial_admin_password
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(Error::other(
            "--initial-admin-password is required when --auth-enabled is set",
        ));
    }

    let setup = DesktopInstallSetup {
        start_on_login: Some(has_arg(args, "--start-on-login")),
        start_silent: Some(has_arg(args, "--start-silent")),
        language: arg_value(args, "--language").or_else(detected_language_default),
        auth_enabled: Some(auth_enabled),
        initial_admin_password,
    };

    write_staged_setup(&setup)
}

fn has_arg(args: &[String], expected: &str) -> bool {
    args.iter().any(|arg| arg == expected)
}

fn arg_value(args: &[String], expected: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == expected).then(|| window[1].clone()))
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

#[cfg(any(target_os = "macos", windows))]
fn spawn_autostart_sync_loop(
    app: tauri::AppHandle,
    services: injection::cross_cutting::InjectedServices,
) {
    tauri::async_runtime::spawn(async move {
        let mut last_enabled = None;
        loop {
            match services.repository.configuration.get_active().await {
                Ok(configuration) => {
                    let enabled = configuration.desktop_start_on_login == Some(1);
                    if last_enabled != Some(enabled) {
                        if let Err(error) = sync_autostart(&app, enabled) {
                            eprintln!("Failed to sync Lince desktop autostart: {error}");
                        } else {
                            last_enabled = Some(enabled);
                        }
                    }
                }
                Err(error) => eprintln!("Failed to read desktop startup configuration: {error}"),
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

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

    let open_devtools = env::var_os("LINCE_DESKTOP_DEVTOOLS").is_some();
    let window = WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, url)
        .icon(app_icon()?)?
        .title("Lince")
        .inner_size(1440.0, 960.0)
        .min_inner_size(980.0, 680.0)
        .devtools(cfg!(debug_assertions))
        .on_page_load(move |window, payload| {
            #[cfg(debug_assertions)]
            if open_devtools && payload.event() == PageLoadEvent::Finished {
                eprintln!("Opening Lince desktop WebKit devtools");
                window.open_devtools();
            }
        })
        .build()?;

    #[cfg(debug_assertions)]
    if open_devtools {
        eprintln!("Opening Lince desktop WebKit devtools");
        window.open_devtools();
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
