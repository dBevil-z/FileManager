use crate::{NativeState, native};
use filemanager_core::{Result, err};
use gdk::prelude::*;
use tauri::{
    LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

const DEFAULT_SIZE: LogicalSize<f64> = LogicalSize::new(780.0, 560.0);
const MIN_SIZE: LogicalSize<f64> = LogicalSize::new(520.0, 340.0);
const SHELF_MARGIN: f64 = 16.0;

fn create(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow> {
    let preferred_size = app
        .state::<NativeState>()
        .shelf_size
        .lock()
        .ok()
        .and_then(|size| *size)
        .unwrap_or(DEFAULT_SIZE);
    let monitor = app.primary_monitor().map_err(err)?;
    let (size, min_size) = monitor
        .as_ref()
        .map_or((preferred_size, MIN_SIZE), |monitor| {
            let area = monitor
                .work_area()
                .size
                .to_logical::<f64>(monitor.scale_factor());
            let width = (area.width - 2.0 * SHELF_MARGIN).max(1.0);
            let height = (area.height - 2.0 * SHELF_MARGIN).max(1.0);
            (
                LogicalSize::new(
                    preferred_size.width.min(width),
                    preferred_size.height.min(height),
                ),
                LogicalSize::new(MIN_SIZE.width.min(width), MIN_SIZE.height.min(height)),
            )
        });
    let window = WebviewWindowBuilder::new(
        app,
        "shelf",
        WebviewUrl::App("index.html?window=shelf".into()),
    )
    .title("Drop Shelf")
    .inner_size(size.width, size.height)
    .min_inner_size(min_size.width, min_size.height)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .visible(false)
    .build()
    .map_err(err)?;
    if let Some(monitor) = monitor {
        // GTK has not reported the window geometry before the first show.
        position(&window, &monitor, size.to_physical(monitor.scale_factor()))?;
    }
    native::install_drag(&window).map_err(err)?;
    Ok(window)
}

fn position(
    window: &tauri::WebviewWindow,
    monitor: &Monitor,
    size: PhysicalSize<u32>,
) -> Result<()> {
    let area = monitor.work_area();
    let margin = (SHELF_MARGIN * monitor.scale_factor()) as u32;
    let x = area.position.x + (area.size.width.saturating_sub(size.width) / 2) as i32;
    let y = area.position.y + area.size.height.saturating_sub(size.height).min(margin) as i32;
    window.set_position(PhysicalPosition { x, y }).map_err(err)
}

pub fn show(app: &tauri::AppHandle, focus: bool) -> Result<()> {
    let window = match app.get_webview_window("shelf") {
        Some(window) => {
            if !window.is_visible().map_err(err)?
                && let Some(monitor) = window.primary_monitor().map_err(err)?
            {
                position(&window, &monitor, window.outer_size().map_err(err)?)?;
            }
            window
        }
        None => create(app)?,
    };
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
                if let (Ok(size), Ok(scale), Ok(mut preferred_size)) = (
                    window.inner_size(),
                    window.scale_factor(),
                    app.state::<NativeState>().shelf_size.lock(),
                ) {
                    *preferred_size = Some(size.to_logical(scale));
                }
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
