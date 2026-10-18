mod net;
mod store;

use std::sync::{mpsc::Sender, Mutex};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

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
    if let Some(w) = app.get_webview_window("ime") {
        let _ = w.hide();
    }
    let _ = app.emit_to("main", "ime-closed", ());
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

#[tauri::command]
fn get_server_time() -> i64 {
    liaisonyy_core::clock::now()
}

#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

#[tauri::command]
fn ime_probe_enabled() -> bool {
    std::env::var("LIAISONYY_IME_PROBE").is_ok()
}

#[tauri::command]
fn ime_probe_result(ok: bool, detail: String) {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .unwrap_or_else(|| std::path::PathBuf::from("data"));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("ime-probe.txt"), format!("ok={ok}\n{detail}\n"));
}

fn ime_pos_file() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .unwrap_or_else(|| std::path::PathBuf::from("data"))
        .join("ime-window.json")
}

fn load_ime_pos() -> Option<(i32, i32)> {
    let s = std::fs::read_to_string(ime_pos_file()).ok()?;
    let mut it = s.trim().split(',');
    let x = it.next()?.parse().ok()?;
    let y = it.next()?.parse().ok()?;
    Some((x, y))
}

fn save_ime_pos(x: i32, y: i32) {
    let p = ime_pos_file();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, format!("{x},{y}"));
}

fn place_ime_near_main(app: &AppHandle, ime: &tauri::WebviewWindow) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Ok(mp) = main.outer_position() else {
        return;
    };
    let ms = main.outer_size().unwrap_or(tauri::PhysicalSize::new(380, 680));
    let is = ime.outer_size().unwrap_or(tauri::PhysicalSize::new(380, 320));
    let mut x = mp.x - is.width as i32 - 8;
    let mut y = mp.y + ms.height as i32 - is.height as i32;
    if x < 0 {
        x = mp.x;
        y = mp.y - is.height as i32 - 8;
    }
    let _ = ime.set_position(tauri::PhysicalPosition::new(x.max(0), y.max(0)));
}

#[tauri::command]
fn ime_show(app: AppHandle) {
    let Some(w) = app.get_webview_window("ime") else {
        return;
    };
    if load_ime_pos().is_none() {
        place_ime_near_main(&app, &w);
    }
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to("ime", "ime-open", ());
}

#[tauri::command]
fn ime_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("ime") {
        let _ = w.hide();
    }
    let _ = app.emit_to("main", "ime-closed", ());
}

#[tauri::command]
fn ime_start_drag(app: AppHandle) {
    if let Some(w) = app.get_webview_window("ime") {
        let _ = w.start_dragging();
    }
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
            if let Some(ime) = app.get_webview_window("ime") {
                let _ = ime.hide();
            }
            let _ = app.emit_to("main", "ime-closed", ());
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
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

            let _ = app.global_shortcut().register("Control+Shift+H");

            if let Some(ime) = app.get_webview_window("ime") {
                if let Some((x, y)) = load_ime_pos() {
                    let _ = ime.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }

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
        .invoke_handler(tauri::generate_handler![
            send_cmd,
            panic_logout,
            quit_app,
            get_settings,
            get_server_time,
            notify,
            ime_probe_enabled,
            ime_probe_result,
            ime_show,
            ime_hide,
            ime_start_drag
        ])
        .on_window_event(|window, event| {
            if window.label() == "ime" {
                match event {
                    tauri::WindowEvent::Moved(pos) => save_ime_pos(pos.x, pos.y),
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                        let _ = window.app_handle().emit_to("main", "ime-closed", ());
                    }
                    _ => {}
                }
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if let Ok(tx) = state.cmd.lock() {
                        let _ = tx.send(net::Cmd::Logout);
                    }
                }
                if let Some(ime) = window.app_handle().get_webview_window("ime") {
                    let _ = ime.hide();
                }
                let _ = window.app_handle().emit_to("main", "ime-closed", ());
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running liaisonyy");
}
