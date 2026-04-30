use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonVersionInfo {
    pub version: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub app_version: String,
    pub app_protocol_version: u32,
    pub daemon_version: Option<String>,
    pub daemon_protocol_version: Option<u32>,
    pub service_available: bool,
}

impl VersionInfo {
    pub fn from_daemon(
        app_version: impl Into<String>,
        app_protocol_version: u32,
        daemon: DaemonVersionInfo,
    ) -> Self {
        Self {
            app_version: app_version.into(),
            app_protocol_version,
            daemon_version: Some(daemon.version),
            daemon_protocol_version: Some(daemon.protocol_version),
            service_available: true,
        }
    }

    pub fn protocol_mismatch(
        app_version: impl Into<String>,
        app_protocol_version: u32,
        daemon_protocol_version: u32,
    ) -> Self {
        Self {
            app_version: app_version.into(),
            app_protocol_version,
            daemon_version: None,
            daemon_protocol_version: Some(daemon_protocol_version),
            service_available: true,
        }
    }

    pub fn unavailable(app_version: impl Into<String>, app_protocol_version: u32) -> Self {
        Self {
            app_version: app_version.into(),
            app_protocol_version,
            daemon_version: None,
            daemon_protocol_version: None,
            service_available: false,
        }
    }
}
