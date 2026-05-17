use crate::ipc::protocol::SessionBackend;
use crate::models::{DisplayLayout, Orientation};

/// Interface implemented by each compositor Adapter.
///
/// Callers use the public display-layout functions in the parent Module; only
/// this seam needs to know which compositor command syntax or parser shape is
/// active.
pub(super) trait CompositorDisplayAdapter {
    fn layout(&self) -> Result<DisplayLayout, String>;
    fn apply_layout(&self, layout: &DisplayLayout) -> Result<(), String>;
    fn set_orientation(&self, orientation: &Orientation) -> Result<(), String>;
}

struct GnomeAdapter;
struct KdeAdapter;
struct NiriAdapter;

impl CompositorDisplayAdapter for GnomeAdapter {
    fn layout(&self) -> Result<DisplayLayout, String> {
        super::gnome::get_gnome_display_layout()
    }

    fn apply_layout(&self, layout: &DisplayLayout) -> Result<(), String> {
        super::gnome::apply_gnome_display_layout(layout)
    }

    fn set_orientation(&self, orientation: &Orientation) -> Result<(), String> {
        super::gnome::set_gnome_orientation(orientation)
    }
}

impl CompositorDisplayAdapter for KdeAdapter {
    fn layout(&self) -> Result<DisplayLayout, String> {
        super::kde::get_kde_display_layout()
    }

    fn apply_layout(&self, layout: &DisplayLayout) -> Result<(), String> {
        super::kde::apply_kde_display_layout(layout)
    }

    fn set_orientation(&self, orientation: &Orientation) -> Result<(), String> {
        super::kde::set_kde_orientation(orientation)
    }
}

impl CompositorDisplayAdapter for NiriAdapter {
    fn layout(&self) -> Result<DisplayLayout, String> {
        super::niri::get_niri_display_layout()
    }

    fn apply_layout(&self, layout: &DisplayLayout) -> Result<(), String> {
        super::niri::apply_niri_display_layout(layout)
    }

    fn set_orientation(&self, orientation: &Orientation) -> Result<(), String> {
        super::niri::set_niri_orientation(orientation)
    }
}

pub(super) fn with_display_adapter<T>(
    backend: SessionBackend,
    f: impl FnOnce(&dyn CompositorDisplayAdapter) -> Result<T, String>,
) -> Result<T, String> {
    match backend {
        SessionBackend::Gnome => f(&GnomeAdapter),
        SessionBackend::Kde => f(&KdeAdapter),
        SessionBackend::Niri => f(&NiriAdapter),
        SessionBackend::Unknown => Err("Unsupported session backend for display layout".into()),
    }
}
