mod integration;
mod native;
mod shelf;
mod updates;
use filemanager_core::{Core, Result};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct NativeState {
    settings: Mutex<Value>,
    tray_actions: Mutex<std::collections::HashMap<String, Value>>,
    pending_route: Mutex<Option<Value>>,
    registered_shortcut: Mutex<Option<String>>,
    startup_warnings: Mutex<Vec<String>>,
}

#[tauri::command]
async fn request(
    input: Value,
    core: tauri::State<'_, Core>,
    app: tauri::AppHandle,
) -> Result<Value> {
    let core = core.inner().clone();
    let op = input["op"].as_str();
    let catalog_changed = op == Some("catalog_save")
        && matches!(
            input["args"]["namespace"].as_str(),
            None | Some("catalog" | "settings")
        );
    let update = catalog_changed || matches!(op, Some("undo" | "write_text" | "rename_apply"));
    let result = tauri::async_runtime::spawn_blocking(move || core.request(input))
        .await
        .map_err(|e| e.to_string())??;
    if update {
        let _ = app.emit("filemanager-data-changed", ());
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = native::refresh_tray(&handle);
        });
    }
    Ok(result)
}
#[tauri::command]
async fn desktop_action(
    action: String,
    args: Value,
    core: tauri::State<'_, Core>,
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Value> {
    native::action(app, window, core.inner().clone(), action, args).await
}
fn data_dir() -> PathBuf {
    std::env::var_os("FILEMANAGER_DATA_DIR")
        .or_else(|| std::env::var_os("FILEOS_DATA_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share")
                });
            let legacy = base.join("fileos");
            if legacy.join("fileos.sqlite").is_file() {
                legacy
            } else {
                base.join("filemanager")
            }
        })
}
pub fn run() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    let args = integration::arguments();
    if integration::static_cli(&args) {
        return;
    }
    let core = if let Some(home) = std::env::var_os("FILEMANAGER_TEST_HOME") {
        let home = PathBuf::from(home);
        Core::with_paths(data_dir(), home.clone(), home.join(".local/share/Trash"))
    } else {
        Core::new(data_dir())
    }
    .expect("FileManager local database");
    if integration::cli(&core, &args) {
        return;
    }
    let media_core = core.clone();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let app = app.clone();
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if !integration::route_arguments(&handle, &args, std::path::Path::new(&cwd))
                    && !args.iter().any(|a| a == "--background")
                {
                    integration::route(&handle, json!({}));
                }
            });
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(core)
        .manage(NativeState::default())
        .manage(updates::UpdateState::default())
        .register_asynchronous_uri_scheme_protocol("filemanager-media", move |_ctx, req, responder| {
            let core = media_core.clone();
            std::thread::spawn(move || {
                let id = req.uri().path().trim_start_matches('/');
                let response = match filemanager_core::media::response(&core, id, req.headers().get("Range").and_then(|s|s.to_str().ok())) {
                    Ok(r) => {
                        let mut b = tauri::http::Response::builder().status(r.status)
                            .header("Content-Type", r.mime).header("Accept-Ranges", "bytes")
                            .header("Access-Control-Allow-Origin", "*").header("X-Content-Type-Options", "nosniff")
                            .header("Content-Security-Policy", "default-src 'none'; sandbox;");
                        if r.status==206 { b=b.header("Content-Range",r.range); }
                        b.body(r.body).unwrap()
                    }
                    Err(e)=>tauri::http::Response::builder().status(404).body(e.into_bytes()).unwrap(),
                };
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![request,desktop_action])
        .on_page_load(|webview, payload| {
            #[cfg(debug_assertions)]
            if payload.event()==tauri::webview::PageLoadEvent::Finished && webview.label()=="main" {
                native::capture_development_window(webview);
            }
            #[cfg(not(debug_assertions))]
            let _ = (webview,payload);
        })
        .setup(move |app| {
            let main = app.get_webview_window("main").unwrap();
            let core = app.state::<Core>();
            let stored = core.request(json!({"op":"catalog_get","args":{"namespace":"settings"}}))?["data"].clone();
            let settings = if stored.is_object() { stored } else { json!({"showTray":true,"startAtLogin":false,"shelfShortcut":"Control+Alt+Space","shelfOnDrag":true,"shelfOnHover":true}) };
            let mut warnings = Vec::new();
            if let Err(e) = native::install_drag(&main) { warnings.push(e.to_string()); }
            if let Err(e) = native::create_tray(app.handle()) { warnings.push(e); }
            warnings.extend(native::initialize(app.handle(), settings));
            for warning in &warnings { eprintln!("FileManager: {warning}"); }
            if let Ok(mut current) = app.state::<NativeState>().startup_warnings.lock() {
                *current = warnings;
            }
            shelf::watch(app.handle());
            integration::start_dbus(app.handle());
            let cwd = std::env::current_dir().unwrap_or_else(|_| core.home_dir.clone());
            if !integration::route_arguments(app.handle(), &args, &cwd)
                && (!args.iter().any(|a| a == "--background") || app.tray_by_id("filemanager").is_none())
            {
                main.show()?;
            }
            Ok(())
        })
        .on_window_event(|w,event| {
            if let tauri::WindowEvent::CloseRequested{api,..}=event {
                if w.label()=="main" {
                    let background=w.app_handle().tray_by_id("filemanager").is_some() && w.state::<NativeState>().settings.lock().is_ok_and(|s|s["showTray"]!=false);
                    if background {api.prevent_close();let _=w.hide();}else{w.app_handle().exit(0);}
                } else if w.label()=="shelf" {api.prevent_close();let _=w.hide();}
            }
        });
    builder
        .run(tauri::generate_context!())
        .expect("FileManager desktop runtime");
}
