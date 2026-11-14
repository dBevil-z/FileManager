use crate::{NativeState, native};
use filemanager_core::{Result, err};
use gdk::prelude::*;
use tauri::{Manager, PhysicalPosition, Position, WebviewUrl, WebviewWindowBuilder};
fn create(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        "shelf",
        WebviewUrl::App("index.html?window=shelf".into()),
    )
    .title("Drop Shelf")
    .inner_size(780.0, 340.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .build()
    .map_err(err)?;
    native::install_drag(&window).map_err(err)?;
    Ok(window)
}
pub fn show(app: &tauri::AppHandle, focus: bool) -> Result<()> {
    let window = match app.get_webview_window("shelf") {
        Some(window) => window,
        None => create(app)?,
    };
    if let Some(monitor) = window.primary_monitor().map_err(err)? {
        let size = window.outer_size().map_err(err)?;
        let pos = monitor.position();
        let x = pos.x + (monitor.size().width as i32 - size.width as i32) / 2;
        let y = pos.y + (16.0 * monitor.scale_factor()) as i32;
        window
            .set_position(Position::Physical(PhysicalPosition { x, y }))
            .map_err(err)?;
    }
    window.show().map_err(err)?;
    if focus {
        window.set_focus().map_err(err)?;
    }
    Ok(())
}
pub fn toggle(app: &tauri::AppHandle, focus: bool) -> Result<()> {
    let Some(window) = app.get_webview_window("shelf") else {
        return show(app, focus);
    };
    if window.is_visible().map_err(err)? {
        window.hide().map_err(err)
    } else {
        show(app, focus)
    }
}
pub fn watch(app: &tauri::AppHandle) {
    let app = app.clone();
    let mut dragging = false;
    let mut hovered = false;
    let mut hidden_since = None::<std::time::Instant>;
    glib::timeout_add_local(std::time::Duration::from_millis(180), move || {
        if let Some(window) = app.get_webview_window("shelf") {
            if window.is_visible().unwrap_or(false) {
                hidden_since = None;
            } else if hidden_since
                .get_or_insert_with(std::time::Instant::now)
                .elapsed()
                .as_secs()
                >= 20
            {
                // Holding is already persisted in the catalog. Release this second
                // WebKit process while the shelf is unused; reopen it on demand.
                let _ = window.destroy();
                hidden_since = None;
            }
        } else {
            hidden_since = None;
        }
        let (on_drag, on_hover) = app
            .state::<NativeState>()
            .settings
            .lock()
            .map(|s| (s["shelfOnDrag"] != false, s["shelfOnHover"] != false))
            .unwrap_or((false, false));
        if !on_drag && !on_hover {
            dragging = false;
            hovered = false;
            return glib::ControlFlow::Continue;
        }
        let Some(display) = gdk::Display::default() else {
            return glib::ControlFlow::Continue;
        };
        let x11 = display.type_().name().contains("X11");
        let drag = on_drag
            && x11
            && gdk::selection_owner_get(&gdk::Atom::intern("XdndSelection")).is_some();
        if drag && !dragging {
            let _ = show(&app, false);
        }
        dragging = drag;
        if let Some(pointer) = display.default_seat().and_then(|s| s.pointer()) {
            let (_, x, y) = pointer.position();
            if let Ok(Some(monitor)) = app.primary_monitor() {
                let p = monitor.position();
                let width = monitor.size().width as i32;
                let edge = y <= p.y + 2
                    && y >= p.y
                    && x > p.x + width / 2 - 130
                    && x < p.x + width / 2 + 130;
                if edge && !hovered && on_hover {
                    let _ = show(&app, false);
                }
                hovered = edge;
            }
        }
        glib::ControlFlow::Continue
    });
}
