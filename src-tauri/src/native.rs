use crate::{Core, NativeState};
use filemanager_core::{Result, err, paths, process};
use gtk::prelude::*;
use serde_json::{Value, json};
use std::{cell::RefCell, sync::mpsc};
use tauri::{
    Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
thread_local! {static DRAG_URIS:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};}
thread_local! {static WATCHES:RefCell<std::collections::HashMap<String,Vec<gtk::gio::FileMonitor>>>=RefCell::new(std::collections::HashMap::new());}
fn uris(core: &Core, args: &Value) -> Result<Vec<String>> {
    args["paths"]
        .as_array()
        .ok_or("未选择文件")?
        .iter()
        .map(|v| {
            let p = paths::decode(core, v.as_str().ok_or("路径无效")?)?;
            std::fs::symlink_metadata(&p).map_err(err)?;
            url::Url::from_file_path(p)
                .map(|u| u.to_string())
                .map_err(|_| "无法转换文件 URI".into())
        })
        .collect()
}
async fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    action: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (tx, rx) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _ = tx.send(action());
    })
    .map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(err)?)
        .await
        .map_err(err)?
}
fn clipboard() -> gtk::Clipboard {
    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
}
fn targets() -> Vec<gtk::TargetEntry> {
    vec![
        gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 1),
        gtk::TargetEntry::new("x-special/gnome-copied-files", gtk::TargetFlags::empty(), 2),
        gtk::TargetEntry::new(
            "application/x-kde-cutselection",
            gtk::TargetFlags::empty(),
            3,
        ),
        gtk::TargetEntry::new("UTF8_STRING", gtk::TargetFlags::empty(), 4),
        gtk::TargetEntry::new("text/plain;charset=utf-8", gtk::TargetFlags::empty(), 4),
    ]
}
fn drag_targets() -> Vec<gtk::TargetEntry> {
    vec![
        gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 1),
        gtk::TargetEntry::new("text/plain;charset=utf-8", gtk::TargetFlags::empty(), 4),
    ]
}
pub fn install_drag(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let app = window.app_handle().clone();
    window.with_webview(move |webview| {
        use webkit2gtk::{SettingsExt, WebContextExt, WebViewExt};
        let widget = webview.inner();
        if let Some(context) = widget.context() {
            context.set_cache_model(webkit2gtk::CacheModel::DocumentViewer);
        }
        if let Some(settings) = WebViewExt::settings(&widget) {
            settings.set_enable_page_cache(false);
        }
        widget.connect_drag_data_get(|_, _, data, info, _| {
            DRAG_URIS.with(|v| {
                let uris = v.borrow();
                if info == 1 {
                    let refs: Vec<_> = uris.iter().map(String::as_str).collect();
                    data.set_uris(&refs);
                } else {
                    data.set(
                        &gdk::Atom::intern("UTF8_STRING"),
                        8,
                        uris.join("\n").as_bytes(),
                    );
                }
            })
        });
        widget.connect_drag_end(move |_, _| {
            DRAG_URIS.with(|v| v.borrow_mut().clear());
            let _ = app.emit("filemanager-drag-ended", ());
        });
    })
}
pub async fn action(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    core: Core,
    action: String,
    args: Value,
) -> Result<Value> {
    match action.as_str() {
        "open_with" => {
            let path = paths::decode(&core, filemanager_core::str_arg(&args, "path")?)?;
            on_main(&app, move || {
                let parent = window.gtk_window().map_err(err)?;
                let file = gtk::gio::File::for_path(path);
                let dialog =
                    gtk::AppChooserDialog::new(Some(&parent), gtk::DialogFlags::MODAL, &file);
                let response = dialog.run();
                let result = if response == gtk::ResponseType::Ok {
                    dialog
                        .app_info()
                        .ok_or_else(|| "请选择应用".to_string())?
                        .launch(&[file], None::<&gtk::gio::AppLaunchContext>)
                        .map_err(err)
                } else {
                    Ok(())
                };
                dialog.close();
                result?;
                Ok(json!({"ok":true}))
            })
            .await
        }
        "watch_directories" => {
            let paths = args["paths"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .take(2)
                .map(|p| paths::decode(&core, p))
                .collect::<Result<Vec<_>>>()?;
            let label = window.label().to_owned();
            let a = app.clone();
            on_main(&app, move || {
                let mut monitors = Vec::new();
                let pending = std::rc::Rc::new(std::cell::Cell::new(false));
                for path in paths {
                    let monitor = gtk::gio::File::for_path(path)
                        .monitor_directory(
                            gtk::gio::FileMonitorFlags::WATCH_MOVES,
                            None::<&gtk::gio::Cancellable>,
                        )
                        .map_err(err)?;
                    let app = a.clone();
                    let pending = pending.clone();
                    monitor.connect_changed(move |_, _, _, _| {
                        if pending.replace(true) {
                            return;
                        }
                        let app = app.clone();
                        let pending = pending.clone();
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(200),
                            move || {
                                pending.set(false);
                                let _ = app.emit("filemanager-files-changed", ());
                            },
                        );
                    });
                    monitors.push(monitor);
                }
                WATCHES.with(|w| {
                    if let Some(old) = w.borrow_mut().insert(label, monitors) {
                        for m in old {
                            m.cancel();
                        }
                    }
                });
                Ok(json!({"ok":true}))
            })
            .await
        }
        "initial_route" => Ok(app
            .state::<NativeState>()
            .pending_route
            .lock()
            .map_err(err)?
            .take()
            .unwrap_or(Value::Null)),
        "clipboard_write_files" => {
            let uris = uris(&core, &args)?;
            let cut = args["kind"] == "move";
            on_main(&app, move || {
                let cb = clipboard();
                write_file_clipboard(&cb, uris, cut)?;
                Ok(json!({"ok":true}))
            })
            .await
        }
        "clipboard_read_files" => {
            on_main(&app, || {
                let cb = clipboard();
                read_file_clipboard(&cb)
            })
            .await
        }
        "clipboard_write_text" => {
            let text = args["text"].as_str().unwrap_or("").to_owned();
            on_main(&app, move || {
                clipboard().set_text(&text);
                Ok(json!({"ok":true}))
            })
            .await
        }
        "drag_files" => {
            let uris = uris(&core, &args)?;
            let (tx, rx) = mpsc::sync_channel(1);
            window
                .with_webview(move |webview| {
                    DRAG_URIS.with(|v| *v.borrow_mut() = uris);
                    let widget = webview.inner();
                    let list = gtk::TargetList::new(&drag_targets());
                    let result = widget.drag_begin_with_coordinates(
                        &list,
                        gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK,
                        1,
                        None,
                        -1,
                        -1,
                    );
                    if let Some(context) = result {
                        context.drag_set_icon_name("folder", 0, 0);
                        let _ = tx.send(Ok(json!({"ok":true})));
                    } else {
                        let _ = tx.send(Err("无法开始原生拖动，请按住鼠标后重试".into()));
                    }
                })
                .map_err(err)?;
            tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(err)?)
                .await
                .map_err(err)?
        }
        "shelf_toggle" => {
            let handle = app.clone();
            on_main(&app, move || {
                crate::shelf::toggle(&handle, true)?;
                Ok(json!({"ok":true}))
            })
            .await
        }
        "startup_warnings" => Ok(json!(std::mem::take(
            &mut *app
                .state::<NativeState>()
                .startup_warnings
                .lock()
                .map_err(err)?
        ))),
        "release_memory" => {
            let handle = app.clone();
            on_main(&app, move || {
                use webkit2gtk::{WebContextExt, WebViewExt};
                for window in handle.webview_windows().values() {
                    let _ = window.with_webview(|webview| {
                        if let Some(context) = webview.inner().context() {
                            context.clear_cache();
                        }
                    });
                }
                #[cfg(all(target_os = "linux", target_env = "gnu"))]
                unsafe {
                    libc::malloc_trim(0);
                }
                Ok(json!({"ok":true}))
            })
            .await
        }
        "navigate" => {
            crate::integration::route(&app, json!({"path":args["path"]}));
            Ok(json!({"ok":true}))
        }
        "configure" => {
            let a = app.clone();
            on_main(&app, move || {
                configure(&a, args["settings"].clone())?;
                Ok(json!({"ok":true}))
            })
            .await
        }
        _ => {
            let mut args = args;
            args["action"] = json!(action);
            tauri::async_runtime::spawn_blocking(move || process::desktop(&core, &args))
                .await
                .map_err(err)?
        }
    }
}
fn configure_shortcut(app: &tauri::AppHandle, settings: &Value) -> Result<()> {
    let state = app.state::<NativeState>();
    let mut registered = state.registered_shortcut.lock().map_err(err)?;
    let shortcut = settings["shelfShortcut"]
        .as_str()
        .unwrap_or("Control+Alt+Space");
    if registered.as_deref() == Some(shortcut) {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let _ = crate::shelf::toggle(&handle, true);
                });
            }
        })
        .map_err(|e| format!("Drop Shelf 全局快捷键注册失败，请在设置中更换：{e}"))?;
    if let Some(old) = registered.replace(shortcut.to_owned()) {
        let _ = app.global_shortcut().unregister(old.as_str());
    }
    Ok(())
}

fn configure_autostart(app: &tauri::AppHandle, settings: &Value) -> Result<()> {
    let core = app.state::<Core>();
    let config = if std::env::var_os("FILEMANAGER_TEST_HOME").is_none() {
        std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
    } else {
        None
    }
    .unwrap_or_else(|| core.home_dir.join(".config"));
    let autostart = config.join("autostart/filemanager.desktop");
    let legacy = config.join("autostart/fileos.desktop");
    if std::fs::read_to_string(&legacy)
        .is_ok_and(|s| s.lines().any(|l| l == "X-FileOS-Managed=true"))
    {
        std::fs::remove_file(legacy).map_err(err)?;
    }

    let managed = std::fs::read_to_string(&autostart)
        .is_ok_and(|s| s.lines().any(|line| line == "X-FileManager-Managed=true"));
    if settings["startAtLogin"] == true {
        if paths::existing(&autostart) && !managed {
            return Err("已有同名登录启动项，已保留其内容".into());
        }
        std::fs::create_dir_all(autostart.parent().unwrap()).map_err(err)?;
        let exe = std::env::var_os("APPIMAGE")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .map_err(err)?;
        let quoted = exe
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('\\', "\\\\")
            .replace('%', "%%");
        let temporary = autostart.with_extension(format!("desktop.{}", std::process::id()));
        std::fs::write(&temporary, format!("[Desktop Entry]\nType=Application\nName=FileManager\nExec=\"{quoted}\" --background\nIcon=io.filemanager.linux\nX-FileManager-Managed=true\nX-GNOME-Autostart-enabled=true\n")).map_err(err)?;
        std::fs::rename(&temporary, &autostart).map_err(err)?;
    } else if managed {
        std::fs::remove_file(&autostart).map_err(err)?;
    }
    Ok(())
}

fn configure_tray(app: &tauri::AppHandle, settings: &Value) -> Result<()> {
    if let Some(tray) = app.tray_by_id("filemanager") {
        tray.set_visible(settings["showTray"] != false)
            .map_err(err)?;
    }
    Ok(())
}

pub fn initialize(app: &tauri::AppHandle, settings: Value) -> Vec<String> {
    let mut warnings = Vec::new();
    for result in [
        configure_shortcut(app, &settings),
        configure_tray(app, &settings),
        configure_autostart(app, &settings),
    ] {
        if let Err(e) = result {
            warnings.push(e);
        }
    }
    if let Ok(mut current) = app.state::<NativeState>().settings.lock() {
        *current = settings;
    }
    if let Err(e) = refresh_tray(app) {
        warnings.push(e);
    }
    warnings
}

pub fn configure(app: &tauri::AppHandle, settings: Value) -> Result<()> {
    let state = app.state::<NativeState>();
    let before = state.settings.lock().map_err(err)?.clone();
    if before["shelfShortcut"] != settings["shelfShortcut"] {
        configure_shortcut(app, &settings)?;
    }
    if before["startAtLogin"] != settings["startAtLogin"] {
        configure_autostart(app, &settings)?;
    }
    if before["showTray"] != settings["showTray"] {
        configure_tray(app, &settings)?;
    }
    *state.settings.lock().map_err(err)? = settings;
    refresh_tray(app)?;
    Ok(())
}
pub fn create_tray(app: &tauri::AppHandle) -> Result<()> {
    let mut builder = TrayIconBuilder::with_id("filemanager")
        .tooltip("FileManager")
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if id == "quit" {
                app.exit(0);
                return;
            }
            if id == "shelf" {
                let _ = crate::shelf::toggle(app, true);
                return;
            }
            let route = app
                .state::<NativeState>()
                .tray_actions
                .lock()
                .ok()
                .and_then(|a| a.get(id).cloned());
            if let Some(route) = route {
                crate::integration::route(app, route);
            } else if id == "open" {
                crate::integration::route(app, json!({}));
            } else if id == "settings" {
                crate::integration::route(app, json!({"page":{"type":"settings"}}));
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map_err(err)?;
    Ok(())
}
pub fn refresh_tray(app: &tauri::AppHandle) -> Result<()> {
    let Some(tray) = app.tray_by_id("filemanager") else {
        return Ok(());
    };
    let menu = Menu::new(app).map_err(err)?;
    let item =
        |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>).map_err(err);
    menu.append(&item("open", "打开 FileManager")?)
        .map_err(err)?;
    menu.append(&item("shelf", "Drop Shelf")?).map_err(err)?;
    menu.append(&PredefinedMenuItem::separator(app).map_err(err)?)
        .map_err(err)?;
    let core = app.state::<Core>();
    let mut actions = std::collections::HashMap::new();
    if let Some(places) = filemanager_core::files::places(&core).as_array() {
        for (i, p) in places
            .iter()
            .filter(|p| p["id"] != "root")
            .take(6)
            .enumerate()
        {
            let id = format!("place-{i}");
            menu.append(&item(&id, p["name"].as_str().unwrap_or("文件夹"))?)
                .map_err(err)?;
            actions.insert(id, json!({"path":p["path"]}));
        }
    }
    let catalog = core.request(json!({"op":"catalog_get"}))?["data"].clone();
    if let Some(workspaces) = catalog["workspaces"].as_array() {
        for (i, w) in workspaces
            .iter()
            .filter(|w| w["archived"] != true)
            .take(4)
            .enumerate()
        {
            let id = format!("workspace-{i}");
            menu.append(&item(&id, w["name"].as_str().unwrap_or("工作空间"))?)
                .map_err(err)?;
            actions.insert(id, json!({"page":{"type":"workspace","id":w["id"]}}));
        }
    }
    let downloads = filemanager_core::files::user_directory(&core, "DOWNLOAD", "Downloads");
    if let Ok(list) =
        filemanager_core::files::list(&core, &json!({"path":paths::encode(&downloads)}))
    {
        let mut recent = list["entries"].as_array().cloned().unwrap_or_default();
        recent.sort_by(|a, b| {
            b["modified"]
                .as_f64()
                .partial_cmp(&a["modified"].as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, e) in recent
            .iter()
            .filter(|e| e["is_dir"] != true)
            .take(3)
            .enumerate()
        {
            let id = format!("recent-{i}");
            menu.append(&item(
                &id,
                &format!(
                    "最近：{}",
                    e["name"]
                        .as_str()
                        .unwrap_or("文件")
                        .chars()
                        .take(32)
                        .collect::<String>()
                ),
            )?)
            .map_err(err)?;
            actions.insert(
                id,
                json!({"path":paths::encode(&downloads),"select":e["path"]}),
            );
        }
    }
    menu.append(&PredefinedMenuItem::separator(app).map_err(err)?)
        .map_err(err)?;
    menu.append(&item("settings", "设置…")?).map_err(err)?;
    menu.append(&item("quit", "退出 FileManager")?)
        .map_err(err)?;
    *app.state::<NativeState>()
        .tray_actions
        .lock()
        .map_err(err)? = actions;
    tray.set_menu(Some(menu)).map_err(err)?;
    Ok(())
}

/// An opt-in development capture records the actual WebKit renderer for visual
/// regression inspection. Release builds contain no capture command or hook.
#[cfg(debug_assertions)]
pub fn capture_development_window(webview: &tauri::Webview) {
    use webkit2gtk::WebViewExt;
    let Some(path) = std::env::var_os("FILEMANAGER_CAPTURE_PATH") else {
        return;
    };
    let webview = webview.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(6), move || {
        let _ = webview.with_webview(move |w| {
            w.inner().snapshot(
                webkit2gtk::SnapshotRegion::Visible,
                webkit2gtk::SnapshotOptions::NONE,
                None::<&gtk::gio::Cancellable>,
                move |result| match result {
                    Ok(surface) => {
                        if let Ok(mut file) = std::fs::File::create(&path) {
                            if let Err(e) = surface.write_to_png(&mut file) {
                                eprintln!("Window capture: {e}");
                            }
                        }
                    }
                    Err(e) => eprintln!("Window capture: {e}"),
                },
            );
        });
    });
}

fn write_file_clipboard(cb: &gtk::Clipboard, uris: Vec<String>, cut: bool) -> Result<()> {
    let success = cb.set_with_data(&targets(), move |_, data, info| match info {
        1 => {
            let refs: Vec<_> = uris.iter().map(String::as_str).collect();
            data.set_uris(&refs);
        }
        2 => {
            let bytes = format!("{}\n{}", if cut { "cut" } else { "copy" }, uris.join("\n"));
            data.set(
                &gdk::Atom::intern("x-special/gnome-copied-files"),
                8,
                bytes.as_bytes(),
            );
        }
        3 => data.set(
            &gdk::Atom::intern("application/x-kde-cutselection"),
            8,
            if cut { b"1" } else { b"0" },
        ),
        _ => data.set(
            &gdk::Atom::intern("UTF8_STRING"),
            8,
            uris.join("\n").as_bytes(),
        ),
    });
    if !success {
        return Err("无法取得剪贴板".into());
    }
    Ok(())
}

fn read_file_clipboard(cb: &gtk::Clipboard) -> Result<Value> {
    let mut kind = "copy";
    let mut uris = Vec::new();
    if let Some(data) = cb.wait_for_contents(&gdk::Atom::intern("x-special/gnome-copied-files")) {
        let b = data.data();
        let text = String::from_utf8_lossy(&b);
        let mut lines = text.lines();
        if lines.next() == Some("cut") {
            kind = "move";
        }
        uris = lines.map(str::to_owned).collect();
    }
    if uris.is_empty() {
        if let Some(data) = cb.wait_for_contents(&gdk::Atom::intern("text/uri-list")) {
            uris = data.uris().iter().map(ToString::to_string).collect();
        }
        if let Some(data) =
            cb.wait_for_contents(&gdk::Atom::intern("application/x-kde-cutselection"))
        {
            if data.data() == b"1" {
                kind = "move";
            }
        }
    }
    let paths: Vec<_> = uris
        .iter()
        .filter_map(|u| url::Url::parse(u).ok())
        .filter_map(|u| u.to_file_path().ok())
        .map(|p| paths::encode(&p))
        .collect();
    Ok(json!({"kind":kind,"paths":paths}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_clipboard_formats_round_trip_without_touching_the_user_clipboard() {
        gtk::init().expect("GTK requires the desktop DISPLAY for this integration test");
        let cb = gtk::Clipboard::get(&gdk::Atom::intern("FILEMANAGER_ISOLATED_TEST_SELECTION"));
        let path = std::path::PathBuf::from("/tmp/中文 ' quote\n.txt");
        let uri = url::Url::from_file_path(&path).unwrap().to_string();
        for cut in [false, true] {
            write_file_clipboard(&cb, vec![uri.clone()], cut).unwrap();
            assert!(cb.wait_is_target_available(&gdk::Atom::intern("text/uri-list")));
            let kde = cb
                .wait_for_contents(&gdk::Atom::intern("application/x-kde-cutselection"))
                .unwrap();
            assert_eq!(kde.data(), if cut { b"1" } else { b"0" });
            let result = read_file_clipboard(&cb).unwrap();
            assert_eq!(
                result["paths"],
                json!([filemanager_core::paths::encode(&path)])
            );
            assert_eq!(result["kind"], if cut { "move" } else { "copy" });
        }
        let kde_uri = uri.clone();
        cb.set_with_data(
            &[targets()[0].clone(), targets()[2].clone()],
            move |_, data, info| {
                if info == 1 {
                    data.set_uris(&[kde_uri.as_str()]);
                } else {
                    data.set(
                        &gdk::Atom::intern("application/x-kde-cutselection"),
                        8,
                        b"1",
                    );
                }
            },
        );
        let result = read_file_clipboard(&cb).unwrap();
        assert_eq!(result["kind"], "move");
        assert_eq!(result["paths"][0], filemanager_core::paths::encode(&path));
        cb.clear();
    }
}
