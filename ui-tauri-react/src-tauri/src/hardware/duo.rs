pub const PRIMARY_INTERNAL_CONNECTOR: &str = "eDP-1";
pub const SECONDARY_INTERNAL_CONNECTOR: &str = "eDP-2";
pub const INTERNAL_CONNECTORS: [&str; 2] =
    [PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR];

pub fn is_internal_connector(connector: &str) -> bool {
    INTERNAL_CONNECTORS.contains(&connector)
}

pub fn is_primary_internal_connector(connector: &str) -> bool {
    connector == PRIMARY_INTERNAL_CONNECTOR
}

pub fn is_secondary_internal_connector(connector: &str) -> bool {
    connector == SECONDARY_INTERNAL_CONNECTOR
}

fn board_is_duo_2026(board_name: &str) -> bool {
    board_name.trim() == "UX8407AA"
}

/// Whether this is the 2026 Zenbook Duo (UX8407AA), detected via DMI.
/// Workarounds that have only been verified on that model are gated on this
/// so earlier models keep their existing behavior.
pub fn is_duo_2026() -> bool {
    static DUO_2026: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DUO_2026.get_or_init(|| {
        std::fs::read_to_string("/sys/class/dmi/id/board_name")
            .map(|name| board_is_duo_2026(&name))
            .unwrap_or(false)
    })
}

/// The UX8407AA's primary panel (eDP-1) is physically mounted 180° rotated.
/// KWin does not compensate via the DRM panel-orientation property, so the
/// primary's physical rotation is offset by 180° from the logical orientation,
/// and the two panels swap sides in portrait.
pub fn primary_panel_mounted_inverted() -> bool {
    is_duo_2026()
}

pub fn connector_for_elan_name(name: &str) -> Option<&'static str> {
    if name.contains("ELAN9008") {
        Some(PRIMARY_INTERNAL_CONNECTOR)
    } else if name.contains("ELAN9009") {
        Some(SECONDARY_INTERNAL_CONNECTOR)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_internal_connectors() {
        assert!(is_internal_connector(PRIMARY_INTERNAL_CONNECTOR));
        assert!(is_internal_connector(SECONDARY_INTERNAL_CONNECTOR));
        assert!(!is_internal_connector("HDMI-A-1"));
    }

    #[test]
    fn only_ux8407aa_is_detected_as_the_2026_model() {
        assert!(board_is_duo_2026("UX8407AA"));
        assert!(board_is_duo_2026("UX8407AA\n"));
        assert!(!board_is_duo_2026("UX8406MA"));
        assert!(!board_is_duo_2026(""));
    }

    #[test]
    fn maps_elan_touchscreens_to_internal_connectors() {
        assert_eq!(
            connector_for_elan_name("ELAN9008:00"),
            Some(PRIMARY_INTERNAL_CONNECTOR)
        );
        assert_eq!(
            connector_for_elan_name("ELAN9009:00"),
            Some(SECONDARY_INTERNAL_CONNECTOR)
        );
        assert_eq!(connector_for_elan_name("ELAN0000:00"), None);
    }
}
