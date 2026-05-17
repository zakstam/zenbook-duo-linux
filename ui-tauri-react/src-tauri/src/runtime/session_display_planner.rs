use crate::models::DisplayLayout;

/// In-process Interface for planning and applying dock display modes.
///
/// The planner is intentionally not a wire or OS Adapter: it converts saved or
/// current layouts into desired display outcomes, then delegates compositor
/// execution back to the session-agent display helpers.
pub(crate) struct DockModePlanner;

impl DockModePlanner {
    pub(crate) fn apply(attached: bool, scale: f64, layout: Option<DisplayLayout>) -> Result<(), String> {
        super::session_agent::apply_dock_mode(attached, scale, layout)
    }

    #[cfg(test)]
    pub(crate) fn layout_from_base(
        layout: &DisplayLayout,
        attached: bool,
        scale: f64,
    ) -> Option<DisplayLayout> {
        super::session_agent::dock_layout_from_base(layout, attached, scale)
    }
}
