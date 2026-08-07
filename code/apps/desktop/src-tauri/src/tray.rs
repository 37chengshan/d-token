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
                // Tauri normally reports tray rectangles in global physical
                // coordinates. Unknown logical rectangles fail closed rather
                // than mixing coordinate spaces across monitors.
                if let (tauri::Position::Physical(position), tauri::Size::Physical(size)) =
                    (rect.position, rect.size)
                {
                    toggle_panel(tray.app_handle(), position, size);
                }
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
    icon_position: tauri::PhysicalPosition<i32>,
    icon_size: tauri::PhysicalSize<u32>,
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

/// Placement decision in a single physical coordinate space. The tray rect,
/// panel size, and target monitor work area must all use this same space so
/// mixed-DPI and negative-origin displays remain unambiguous.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PanelPlacement {
    x: f64,
    y: f64,
    /// True when the panel was anchored below the icon; false means it was
    /// flipped above it because the work area ran out below.
    expanded_down: bool,
}

/// Pure placement: anchors the panel under the tray icon, centred on it.
///
/// The expansion edge follows the monitor's work area: downward when the panel
/// fits below the icon, otherwise upward (covers bottom taskbars, top menu
/// bars). Both axes are then clamped into the work area with `offset` margins,
/// which covers left/right taskbars and monitors whose origin is negative
/// (multi-display layouts).
///
/// `offset` is the icon-panel gap in physical pixels for the icon's monitor:
/// the caller scales the logical gap by that monitor's DPI so mixed-DPI
/// layouts stay proportionally identical.
#[allow(clippy::too_many_arguments)]
fn compute_panel_placement(
    icon_x: f64,
    icon_y: f64,
    icon_w: f64,
    icon_h: f64,
    panel_w: f64,
    panel_h: f64,
    work_x: f64,
    work_y: f64,
    work_w: f64,
    work_h: f64,
    offset: f64,
) -> PanelPlacement {
    let mut x = icon_x + icon_w / 2.0 - panel_w / 2.0;
    let mut y = icon_y + icon_h + offset;
    let mut expanded_down = true;

    // Downward would fall off the bottom edge: flip above the icon instead.
    if y + panel_h > work_y + work_h {
        y = icon_y - offset - panel_h;
        expanded_down = false;
    }

    // Clamp both axes into the work area; skip a clamp when the panel is
    // larger than the work area on that axis (nothing sensible to do).
    let min_x = work_x + offset;
    let max_x = work_x + work_w - panel_w - offset;
    let min_y = work_y + offset;
    let max_y = work_y + work_h - panel_h - offset;
    if max_x >= min_x {
        x = x.clamp(min_x, max_x);
    }
    if max_y >= min_y {
        y = y.clamp(min_y, max_y);
    }

    PanelPlacement {
        x,
        y,
        expanded_down,
    }
}

/// Anchors the panel under the tray icon, centred on it and kept on screen.
fn position_panel(
    panel: &WebviewWindow,
    icon_position: tauri::PhysicalPosition<i32>,
    icon_size: tauri::PhysicalSize<u32>,
) {
    let Ok(panel_size) = panel.outer_size() else {
        return;
    };
    let panel_width = panel_size.width as f64;
    let panel_height = panel_size.height as f64;

    // Resolve the monitor from the physical centre of the tray icon. The
    // panel may be hidden on another display, so current_monitor is not a safe
    // fallback for a tray click.
    let icon_center_x = icon_position.x as f64 + icon_size.width as f64 / 2.0;
    let icon_center_y = icon_position.y as f64 + icon_size.height as f64 / 2.0;
    let Some(monitor) = panel
        .monitor_from_point(icon_center_x, icon_center_y)
        .ok()
        .flatten()
    else {
        return;
    };
    let work = monitor.work_area();
    // The icon-panel gap is a logical design constant; scale it to physical
    // pixels for this monitor so mixed-DPI layouts place the panel identically.
    let scale = monitor.scale_factor();
    let placement = compute_panel_placement(
        icon_position.x as f64,
        icon_position.y as f64,
        icon_size.width as f64,
        icon_size.height as f64,
        panel_width,
        panel_height,
        work.position.x as f64,
        work.position.y as f64,
        work.size.width as f64,
        work.size.height as f64,
        PANEL_OFFSET * scale,
    );

    let _ = panel.set_position(tauri::PhysicalPosition::new(
        placement.x.round() as i32,
        placement.y.round() as i32,
    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Panel size used by the real panel: 480 wide, 640 tall.
    const PANEL_W: f64 = 480.0;
    const PANEL_H: f64 = 640.0;
    /// Default tray icon size on Windows.
    const ICON_W: f64 = 24.0;
    const ICON_H: f64 = 24.0;

    fn place(
        icon_x: f64,
        icon_y: f64,
        work_x: f64,
        work_y: f64,
        work_w: f64,
        work_h: f64,
    ) -> PanelPlacement {
        compute_panel_placement(
            icon_x,
            icon_y,
            ICON_W,
            ICON_H,
            PANEL_W,
            PANEL_H,
            work_x,
            work_y,
            work_w,
            work_h,
            PANEL_OFFSET,
        )
    }

    #[test]
    fn expands_down_when_bottom_has_room() {
        // Top menu bar: work area starts below it, icon high on screen.
        let p = place(960.0, 200.0, 0.0, 0.0, 1920.0, 1000.0);
        assert!(p.expanded_down);
        assert_eq!(p.y, 200.0 + ICON_H + PANEL_OFFSET);
        assert_eq!(p.x, 960.0 + ICON_W / 2.0 - PANEL_W / 2.0);
    }

    #[test]
    fn expands_up_when_bottom_taskbar_blocks_down() {
        // Windows default: bottom taskbar eats the last 40px; icon inside it.
        let p = place(960.0, 1024.0, 0.0, 0.0, 1920.0, 1040.0);
        assert!(!p.expanded_down);
        assert_eq!(p.y, 1024.0 - PANEL_OFFSET - PANEL_H);
        assert_eq!(p.x, 960.0 + ICON_W / 2.0 - PANEL_W / 2.0);
    }

    #[test]
    fn clamps_x_against_left_and_right_taskbars() {
        // Icon flush against the right edge: x is pulled back inside.
        let right = place(1920.0 - ICON_W - 2.0, 500.0, 0.0, 0.0, 1920.0, 1000.0);
        assert_eq!(right.x, 1920.0 - PANEL_W - PANEL_OFFSET);
        // Icon flush against the left edge.
        let left = place(2.0, 500.0, 0.0, 0.0, 1920.0, 1000.0);
        assert_eq!(left.x, PANEL_OFFSET);
    }

    #[test]
    fn clamps_y_after_up_flip_when_icon_below_work_area() {
        // Taskbar icon sits below the work area bottom; flipped up still gets
        // clamped so the panel never pokes past the top.
        let p = place(960.0, 1080.0, 0.0, 0.0, 1920.0, 1000.0);
        assert!(!p.expanded_down);
        assert_eq!(p.y, 1000.0 - PANEL_H - PANEL_OFFSET);
    }

    #[test]
    fn handles_negative_multi_monitor_origin() {
        // Secondary monitor to the left of the primary has a negative origin.
        // Icon sits high on that monitor, so the panel expands down and the
        // centred x stays inside the negative-origin work area.
        let p = place(-960.0, 200.0, -1920.0, 0.0, 1920.0, 1000.0);
        assert!(p.expanded_down);
        assert_eq!(p.x, -960.0 + ICON_W / 2.0 - PANEL_W / 2.0);
        assert!(p.x >= -1920.0 + PANEL_OFFSET);
        assert!(p.x + PANEL_W <= -1920.0 + 1920.0 - PANEL_OFFSET);
    }

    #[test]
    fn clamps_into_negative_origin_monitor() {
        // Icon near the left edge of the left monitor: x clamped to its origin.
        let p = place(-1918.0, 500.0, -1920.0, 0.0, 1920.0, 1000.0);
        assert_eq!(p.x, -1920.0 + PANEL_OFFSET);
    }

    #[test]
    fn physical_placement_scales_with_each_monitor() {
        // Equivalent logical layouts at 1.5x and 2x produce proportionally
        // scaled physical placements, without crossing coordinate spaces.
        let s15 = compute_panel_placement(
            1440.0,
            1536.0,
            ICON_W * 1.5,
            ICON_H * 1.5,
            PANEL_W * 1.5,
            PANEL_H * 1.5,
            0.0,
            0.0,
            2880.0,
            1560.0,
            PANEL_OFFSET * 1.5,
        );
        let s20 = compute_panel_placement(
            1920.0,
            2048.0,
            ICON_W * 2.0,
            ICON_H * 2.0,
            PANEL_W * 2.0,
            PANEL_H * 2.0,
            0.0,
            0.0,
            3840.0,
            2080.0,
            PANEL_OFFSET * 2.0,
        );
        assert_eq!(s15.x / 1.5, s20.x / 2.0);
        assert_eq!(s15.y / 1.5, s20.y / 2.0);
        assert!(!s15.expanded_down);
    }

    #[test]
    fn panel_taller_than_work_area_never_panics() {
        let p = place(960.0, 100.0, 0.0, 0.0, 1920.0, 200.0);
        // No clamp possible; the up-flip still wins and numbers stay finite.
        assert!(!p.expanded_down);
        assert!(p.y.is_finite() && p.x.is_finite());
    }
}
