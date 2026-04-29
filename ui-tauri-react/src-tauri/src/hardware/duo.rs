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
