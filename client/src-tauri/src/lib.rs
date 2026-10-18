mod net;
mod store;

use std::sync::{mpsc::Sender, Mutex};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};

struct AppState {
    cmd: Mutex<Sender<net::Cmd>>,
}

#[tauri::command]
fn send_cmd(cmd: net::Cmd, state: State<'_, AppState>) -> Result<(), String> {
    state
        .cmd
        .lock()
        .map_err(|e| e.to_string())?
        .send(cmd)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn panic_logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(tx) = state.cmd.lock() {
        let _ = tx.send(net::Cmd::Logout);
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_settings() -> store::Settings {
    store::load_settings()
}

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if let Some(monitor) = w
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| w.primary_monitor().ok().flatten())
        {
            let scale = monitor.scale_factor();
            let mut size = w.outer_size().unwrap_or(tauri::PhysicalSize::new(0, 0));
            if size.width == 0 || size.height == 0 {
                size = tauri::PhysicalSize::new((380.0 * scale) as u32, (680.0 * scale) as u32);
            }
            let mpos = monitor.position();
            let msize = monitor.size();
            let margin = (16.0 * scale) as i32;
            let taskbar = (48.0 * scale) as i32;
            let x = mpos.x + msize.width as i32 - size.width as i32 - margin;
            let y = mpos.y + msize.height as i32 - size.height as i32 - margin - taskbar;
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_window(app);
        }
    }
}

fn tray_icon_rgba() -> Vec<u8> {
    let mut v = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..16 * 16 {
        v.extend_from_slice(&[15, 118, 110, 255]);
    }
    v
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let settings = store::load_settings();
            let cache = store::load_cache();
            let first_run = settings.class_id.is_none();
            let (cmd_tx, ev_rx) = net::spawn(settings, cache);
            app.manage(AppState { cmd: Mutex::new(cmd_tx) });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(ev) = ev_rx.recv() {
                    let _ = handle.emit("liaisonyy-ev", ev);
                }
            });

            let show = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let icon = Image::new_owned(tray_icon_rgba(), 16, 16);
            TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("liaisonyy")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => toggle_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if first_run {
                show_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![send_cmd, panic_logout, quit_app, get_settings])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if let Ok(tx) = state.cmd.lock() {
                        let _ = tx.send(net::Cmd::Logout);
                    }
                }
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running liaisonyy");
}
