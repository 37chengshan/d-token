//! Tray icon and the always-available monitoring panel.
//!
//! The panel is a second, frameless window that reuses the existing desktop
//! summary commands. It owns no evidence of its own: whatever the panel shows,
//! the main window can show from the same source, so the two cannot disagree.
//!
//! Closing the main window hides it instead of exiting, because the tray is the
//! app's resident surface. Quitting stays an explicit tray action.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewWindow,
};

/// Whether a real system material sits behind the panel. The panel only thins
/// its own background when this holds, so platforms without blur stay opaque.
static PANEL_MATERIAL_APPLIED: AtomicBool = AtomicBool::new(false);

pub const MAIN_WINDOW: &str = "main";
pub const PANEL_WINDOW: &str = "tray-panel";

/// Gap between the tray icon and the panel's top edge.
const PANEL_OFFSET: f64 = 8.0;

const MENU_OPEN: &str = "tray.open";
const MENU_QUIT: &str = "tray.quit";

/// macOS ships a template icon so the glyph tracks light and dark menu bars.
/// Other platforms use the full-colour mark.
#[cfg(target_os = "macos")]
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/iconTemplate@2x.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, MENU_OPEN, "Open d-token", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, MENU_QUIT, "Quit d-token", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("dtoken-tray")
        .icon(Image::from_bytes(TRAY_ICON_PNG)?)
        .tooltip("d-token")
        // Left click toggles the panel, so the menu is reserved for right click.
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_OPEN => show_main_window(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_panel(
                    tray.app_handle(),
                    rect.position.to_logical(1.0),
                    rect.size.to_logical(1.0),
                );
            }
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }

    builder.build(app)?;
    Ok(())
}

/// Reveals the main control plane, restoring it if it was hidden or minimized.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Lets the tray panel hand off to the control plane, where confirmation and
/// rollback flows live. The panel hides itself so focus lands on one surface.
#[tauri::command]
pub fn show_control_plane(app: AppHandle) {
    if let Some(panel) = app.get_webview_window(PANEL_WINDOW) {
        let _ = panel.hide();
    }
    show_main_window(&app);
}

fn toggle_panel(
    app: &AppHandle,
    icon_position: tauri::LogicalPosition<f64>,
    icon_size: tauri::LogicalSize<f64>,
) {
    let Some(panel) = app.get_webview_window(PANEL_WINDOW) else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    position_panel(&panel, icon_position, icon_size);
    if PANEL_MATERIAL_APPLIED.load(Ordering::Relaxed) {
        // Set on show, when the document is guaranteed to exist.
        let _ = panel.eval("document.documentElement.dataset.panelMaterial = 'vibrancy';");
    }
    let _ = panel.show();
    let _ = panel.set_focus();
}

/// Anchors the panel under the tray icon, centred on it and kept on screen.
fn position_panel(
    panel: &WebviewWindow,
    icon_position: tauri::LogicalPosition<f64>,
    icon_size: tauri::LogicalSize<f64>,
) {
    let Ok(panel_size) = panel.outer_size() else {
        return;
    };
    let scale = panel.scale_factor().unwrap_or(1.0);
    let panel_width = panel_size.width as f64 / scale;

    let mut x = icon_position.x + icon_size.width / 2.0 - panel_width / 2.0;
    let y = icon_position.y + icon_size.height + PANEL_OFFSET;

    // Keep the panel inside the work area when the icon sits near a screen edge.
    if let Ok(Some(monitor)) = panel.current_monitor() {
        let monitor_position = monitor.position().to_logical::<f64>(monitor.scale_factor());
        let monitor_size = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let min_x = monitor_position.x + PANEL_OFFSET;
        let max_x = monitor_position.x + monitor_size.width - panel_width - PANEL_OFFSET;
        if max_x >= min_x {
            x = x.clamp(min_x, max_x);
        }
    }

    let _ = panel.set_position(tauri::LogicalPosition::new(x, y));
}

/// Applies the system blur behind the panel. macOS only; other platforms keep
/// the opaque surface the stylesheet already provides.
///
/// Success is reported to the webview so the panel only thins its own
/// background when a real material sits behind it.
pub fn apply_panel_material(app: &AppHandle) {
    let Some(panel) = app.get_webview_window(PANEL_WINDOW) else {
        return;
    };
    let _ = &panel;

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        // HudWindow matches the system's own menu-bar popovers.
        if apply_vibrancy(
            &panel,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(12.0),
        )
        .is_ok()
        {
            PANEL_MATERIAL_APPLIED.store(true, Ordering::Relaxed);
        }
    }
}

/// Hides the panel when focus moves elsewhere, matching menu-bar popover
/// behaviour, and keeps the main window's close button from exiting the app.
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } if window.label() == MAIN_WINDOW => {
            // The tray keeps running, so closing is a hide.
            api.prevent_close();
            let _ = window.hide();
        }
        tauri::WindowEvent::Focused(false) if window.label() == PANEL_WINDOW => {
            let _ = window.hide();
        }
        _ => {}
    }
}
