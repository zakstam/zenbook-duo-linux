//! External screens next to the Duo's panels.
//!
//! `gdctl set` replaces the whole monitor configuration, so a layout built for
//! the internal panels alone would turn every external screen off. The panel
//! rules (dock mode, the lower-screen key) therefore apply to the panels only
//! and carry the external screens over unchanged: same mode, scale, rotation
//! and, where it still fits, the same place next to the panels.

use crate::hardware::duo::{
    is_internal_connector, PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR,
};
use crate::models::{DisplayInfo, DisplayLayout};

/// Applies the panel rules again whenever an external screen is plugged in or
/// out: GNOME then picks its own layout for the new set of screens, which
/// turns both panels on, also the one under an attached keyboard.
pub(crate) async fn watch_hotplug() -> Result<(), String> {
    use futures_util::StreamExt;

    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("session bus: {e}"))?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
    )
    .await
    .map_err(|e| format!("Mutter DisplayConfig: {e}"))?;
    let mut changes = proxy
        .receive_signal("MonitorsChanged")
        .await
        .map_err(|e| format!("MonitorsChanged: {e}"))?;

    let mut known = connected_externals().await;
    while changes.next().await.is_some() {
        // Also emitted for the agent's own layout changes; only a different
        // set of connected screens needs the rules again.
        let now = connected_externals().await;
        if now == known {
            continue;
        }
        known = now;
        let attached = super::brightness_sync::keyboard_attached_from_runtime();
        let scale = crate::commands::settings::load_settings_local().default_scale;
        let result =
            tokio::task::spawn_blocking(move || super::apply_dock_layout(attached, scale, None))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
        match result {
            Ok(()) => super::apply_touch_mapping_best_effort(
                "external screen change",
                crate::runtime::session::detect_backend_from_env(),
            ),
            Err(err) => log::warn!("screen layout after external screen change failed: {err}"),
        }
    }
    Ok(())
}

/// Connectors of the external screens that are plugged in (on or off).
async fn connected_externals() -> Vec<String> {
    tokio::task::spawn_blocking(|| {
        let mut connectors: Vec<String> = crate::hardware::display_layout::get_display_layout()
            .map(|layout| layout.displays.into_iter().map(|d| d.connector).collect())
            .unwrap_or_default();
        connectors.retain(|c| !is_internal_connector(c));
        connectors.sort();
        connectors
    })
    .await
    .unwrap_or_default()
}

/// `panels` plus the external screens that are on now, read from GNOME. Falls
/// back to `panels` alone when GNOME's layout cannot be read.
pub(crate) fn gnome_with_active_externals(panels: DisplayLayout) -> DisplayLayout {
    let Ok(current) = crate::hardware::display_layout::get_display_layout() else {
        return panels;
    };
    let Ok(active) = super::screen_cycle::gnome_active_connectors() else {
        return panels;
    };
    with_externals(panels, &current, &active)
}

/// The external screens that are turned on, as they are now.
pub(crate) fn active_externals(current: &DisplayLayout, active: &[String]) -> Vec<DisplayInfo> {
    current
        .displays
        .iter()
        .filter(|d| !is_internal_connector(&d.connector) && active.contains(&d.connector))
        .cloned()
        .collect()
}

/// `panels` (a layout of internal panels only, possibly empty) plus the
/// external screens that are on now.
///
/// The external screens keep their place relative to the panels when the new
/// layout leaves them adjacent and not overlapping; otherwise they are put in
/// a row to the right of the panels. An external screen that was the primary
/// one stays primary.
pub(crate) fn with_externals(
    panels: DisplayLayout,
    current: &DisplayLayout,
    active: &[String],
) -> DisplayLayout {
    let mut externals = active_externals(current, active);
    if externals.is_empty() {
        return panels;
    }
    let mut displays = panels.displays;

    if externals.iter().any(|d| d.primary) {
        for panel in &mut displays {
            panel.primary = false;
        }
    } else if displays.is_empty() {
        externals[0].primary = true;
    }

    let kept = shift_to_panels(&externals, &displays, current, active);
    let placed = match kept {
        Some(kept) if is_valid(&displays, &kept) => kept,
        _ => in_a_row(&externals, &displays),
    };
    displays.extend(placed);
    DisplayLayout { displays }
}

/// The external screens moved along with the panels: a panel that is on now and
/// stays on keeps the same offset to them.
fn shift_to_panels(
    externals: &[DisplayInfo],
    panels: &[DisplayInfo],
    current: &DisplayLayout,
    active: &[String],
) -> Option<Vec<DisplayInfo>> {
    let (dx, dy) = if panels.is_empty() {
        // Only external screens remain: they already fit together.
        let min_x = externals.iter().map(|d| d.x).min()?;
        let min_y = externals.iter().map(|d| d.y).min()?;
        (-min_x, -min_y)
    } else {
        [PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR]
            .iter()
            .find_map(|connector| {
                let old = current
                    .displays
                    .iter()
                    .find(|d| d.connector == *connector && active.iter().any(|a| a == connector))?;
                let new = panels.iter().find(|d| d.connector == *connector)?;
                Some((new.x - old.x, new.y - old.y))
            })?
    };
    Some(
        externals
            .iter()
            .map(|d| DisplayInfo {
                x: d.x + dx,
                y: d.y + dy,
                ..d.clone()
            })
            .collect(),
    )
}

/// The external screens side by side, to the right of the panels, top-aligned
/// with them, in their current left-to-right order.
fn in_a_row(externals: &[DisplayInfo], panels: &[DisplayInfo]) -> Vec<DisplayInfo> {
    let mut x = panels
        .iter()
        .map(|d| d.x + logical_size(d).0)
        .max()
        .unwrap_or(0);
    let y = panels.iter().map(|d| d.y).min().unwrap_or(0);
    let mut ordered: Vec<&DisplayInfo> = externals.iter().collect();
    ordered.sort_by_key(|d| (d.x, d.y));
    ordered
        .into_iter()
        .map(|d| {
            let placed = DisplayInfo { x, y, ..d.clone() };
            x += logical_size(d).0;
            placed
        })
        .collect()
}

/// Mutter accepts a configuration whose screens do not overlap and are all
/// connected through shared edges.
fn is_valid(panels: &[DisplayInfo], externals: &[DisplayInfo]) -> bool {
    let mut rects: Vec<Rect> = panels.iter().map(rect).collect();
    snap_stacked_panels(&mut rects);
    rects.extend(externals.iter().map(rect));
    let overlaps = rects.iter().enumerate().any(|(i, a)| {
        rects[i + 1..].iter().any(|b| {
            a.x < b.x + b.w - TOLERANCE
                && b.x < a.x + a.w - TOLERANCE
                && a.y < b.y + b.h - TOLERANCE
                && b.y < a.y + a.h - TOLERANCE
        })
    });
    if overlaps {
        return false;
    }
    // Every screen reachable from the first one through touching edges.
    let mut reached = vec![false; rects.len()];
    let mut queue = vec![0];
    reached[0] = true;
    while let Some(i) = queue.pop() {
        for j in 0..rects.len() {
            if !reached[j] && touch(&rects[i], &rects[j]) {
                reached[j] = true;
                queue.push(j);
            }
        }
    }
    reached.into_iter().all(|r| r)
}

/// The bottom panel can sit a few px off the top one's edge (a scale off
/// Mutter's grid, like a legacy saved 1.66, or rounding) and is stacked with
/// gdctl's `--below`, which uses the edge Mutter really applies.
fn snap_stacked_panels(rects: &mut [Rect]) {
    if let [top, bottom] = rects {
        let edge = top.y + top.h;
        if bottom.x == top.x && (bottom.y - edge).abs() <= 8 {
            bottom.y = edge;
        }
    }
}

/// How far edges may be apart (logical px) and still count as touching.
const TOLERANCE: i32 = 2;

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn rect(display: &DisplayInfo) -> Rect {
    let (w, h) = logical_size(display);
    Rect {
        x: display.x,
        y: display.y,
        w,
        h,
    }
}

fn touch(a: &Rect, b: &Rect) -> bool {
    let near = |p: i32, q: i32| (p - q).abs() <= TOLERANCE;
    let overlap_x = a.x < b.x + b.w && b.x < a.x + a.w;
    let overlap_y = a.y < b.y + b.h && b.y < a.y + a.h;
    ((near(a.x + a.w, b.x) || near(b.x + b.w, a.x)) && overlap_y)
        || ((near(a.y + a.h, b.y) || near(b.y + b.h, a.y)) && overlap_x)
}

/// A screen's size in the layout, after its scale and rotation.
fn logical_size(display: &DisplayInfo) -> (i32, i32) {
    let (w, h) = if display.transform == 90 || display.transform == 270 {
        (display.height, display.width)
    } else {
        (display.width, display.height)
    };
    let scale = snapped_scale(display.scale);
    (
        (w as f64 / scale).round() as i32,
        (h as f64 / scale).round() as i32,
    )
}

/// The scale Mutter applies: it snaps a requested scale to the nearest one it
/// supports, a multiple of 1/4 or 1/3. The presets (1, 1.25, 4/3, 1.5, 5/3, 2)
/// are already on that grid; a legacy saved 1.66 becomes 5/3.
fn snapped_scale(scale: f64) -> f64 {
    let scale = scale.max(0.1);
    let quarter = (scale * 4.0).round() / 4.0;
    let third = (scale * 3.0).round() / 3.0;
    let best = if (quarter - scale).abs() <= (third - scale).abs() {
        quarter
    } else {
        third
    };
    if best > 0.0 {
        best
    } else {
        scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisplayMode, RefreshPolicy};

    fn screen(connector: &str, width: u32, height: u32, scale: f64, x: i32, y: i32) -> DisplayInfo {
        let mode = DisplayMode {
            width,
            height,
            refresh_rate: 60.0,
            ..Default::default()
        };
        DisplayInfo {
            connector: connector.into(),
            width,
            height,
            refresh_rate: 60.0,
            scale,
            x,
            y,
            transform: 0,
            primary: false,
            current_mode: mode.clone(),
            available_modes: vec![mode],
            refresh_policy: RefreshPolicy::default(),
            supports_dynamic_refresh: false,
        }
    }

    fn panel(connector: &str, y: i32) -> DisplayInfo {
        // The default scale, 5/3 (1728x1080 logical).
        let mut p = screen(connector, 2880, 1800, 5.0 / 3.0, 0, y);
        p.primary = connector == PRIMARY_INTERNAL_CONNECTOR;
        p
    }

    fn places(layout: &DisplayLayout) -> Vec<(String, i32, i32, bool)> {
        layout
            .displays
            .iter()
            .map(|d| (d.connector.clone(), d.x, d.y, d.primary))
            .collect()
    }

    fn active(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn layout(displays: Vec<DisplayInfo>) -> DisplayLayout {
        DisplayLayout { displays }
    }

    #[test]
    fn without_external_screens_the_panel_layout_is_unchanged() {
        let current = layout(vec![panel("eDP-1", 0), panel("eDP-2", 1080)]);
        let panels = layout(vec![panel("eDP-1", 0)]);
        let result = with_externals(panels, &current, &active(&["eDP-1", "eDP-2"]));
        assert_eq!(places(&result), vec![("eDP-1".into(), 0, 0, true)]);
    }

    #[test]
    fn an_external_screen_beside_the_top_panel_stays_there_when_the_bottom_panel_goes_off() {
        let current = layout(vec![
            panel("eDP-1", 0),
            panel("eDP-2", 1080),
            screen("HDMI-1", 1920, 1080, 1.0, 1728, 0),
        ]);
        let panels = layout(vec![panel("eDP-1", 0)]);
        let result = with_externals(panels, &current, &active(&["eDP-1", "eDP-2", "HDMI-1"]));
        assert_eq!(
            places(&result),
            vec![("eDP-1".into(), 0, 0, true), ("HDMI-1".into(), 1728, 0, false)]
        );
    }

    #[test]
    fn an_external_screen_left_of_the_panels_moves_along_with_them() {
        // The panels sit right of the external screen; the new panel layout
        // starts at 0,0, so the external screen ends up at a negative x
        // (gdctl's argument builder shifts the whole layout back).
        let current = layout(vec![
            screen("DP-3", 2560, 1440, 1.0, 0, 0),
            panel("eDP-1", 0),
            panel("eDP-2", 1080),
        ]);
        let mut current = current;
        current.displays[1].x = 2560;
        current.displays[2].x = 2560;
        let panels = layout(vec![panel("eDP-1", 0), panel("eDP-2", 1080)]);
        let result = with_externals(panels, &current, &active(&["DP-3", "eDP-1", "eDP-2"]));
        assert_eq!(
            places(&result)[2],
            ("DP-3".into(), -2560, 0, false)
        );
    }

    #[test]
    fn an_external_screen_below_the_bottom_panel_moves_beside_the_top_panel_when_the_bottom_goes_off() {
        let current = layout(vec![
            panel("eDP-1", 0),
            panel("eDP-2", 1080),
            screen("HDMI-1", 1920, 1080, 1.0, 0, 2160),
        ]);
        let panels = layout(vec![panel("eDP-1", 0)]);
        let result = with_externals(panels, &current, &active(&["eDP-1", "eDP-2", "HDMI-1"]));
        assert_eq!(places(&result)[1], ("HDMI-1".into(), 1728, 0, false));
    }

    #[test]
    fn an_external_screen_beside_the_bottom_panel_stays_there() {
        let current = layout(vec![
            panel("eDP-1", 0),
            panel("eDP-2", 1080),
            screen("HDMI-1", 1920, 1080, 1.0, 1728, 1080),
        ]);
        // As dock_layout_from_base builds it: y from the requested scale.
        let panels = layout(vec![panel("eDP-1", 0), panel("eDP-2", 1085)]);
        let result = with_externals(panels, &current, &active(&["eDP-1", "eDP-2", "HDMI-1"]));
        assert_eq!(places(&result)[2], ("HDMI-1".into(), 1728, 1080, false));
    }

    #[test]
    fn a_turned_off_external_screen_stays_off() {
        let current = layout(vec![
            panel("eDP-1", 0),
            screen("HDMI-1", 1920, 1080, 1.0, 1728, 0),
        ]);
        let panels = layout(vec![panel("eDP-1", 0), panel("eDP-2", 1080)]);
        let result = with_externals(panels, &current, &active(&["eDP-1"]));
        assert_eq!(result.displays.len(), 2);
    }

    #[test]
    fn a_primary_external_screen_stays_primary() {
        let mut external = screen("DP-3", 2560, 1440, 1.0, 1728, 0);
        external.primary = true;
        let mut top = panel("eDP-1", 0);
        top.primary = false;
        let current = layout(vec![top, external]);
        let panels = layout(vec![panel("eDP-1", 0), panel("eDP-2", 1080)]);
        let result = with_externals(panels, &current, &active(&["eDP-1", "DP-3"]));
        let primaries: Vec<_> = result.displays.iter().filter(|d| d.primary).map(|d| d.connector.as_str()).collect();
        assert_eq!(primaries, vec!["DP-3"]);
    }

    #[test]
    fn with_both_panels_off_the_external_screens_keep_their_arrangement_from_the_origin() {
        let current = layout(vec![
            panel("eDP-1", 0),
            screen("HDMI-1", 1920, 1080, 1.0, 1728, 0),
            screen("DP-3", 1920, 1080, 1.0, 3648, 0),
        ]);
        let result = with_externals(layout(vec![]), &current, &active(&["eDP-1", "HDMI-1", "DP-3"]));
        assert_eq!(
            places(&result),
            vec![("HDMI-1".into(), 0, 0, true), ("DP-3".into(), 1920, 0, false)]
        );
    }

    #[test]
    fn mutter_snaps_the_requested_panel_scale() {
        assert_eq!(logical_size(&panel("eDP-1", 0)), (1728, 1080));
        assert_eq!(snapped_scale(1.25), 1.25);
        assert_eq!(snapped_scale(2.0), 2.0);
        // The default is stored as 5/3 now; a legacy saved 1.66 snaps to it.
        assert!((snapped_scale(1.66) - 5.0 / 3.0).abs() < 1e-9);
        assert!((snapped_scale(5.0 / 3.0) - 5.0 / 3.0).abs() < 1e-9);
        let mut legacy = panel("eDP-1", 0);
        legacy.scale = 1.66;
        assert_eq!(logical_size(&legacy), (1728, 1080));
    }
}
