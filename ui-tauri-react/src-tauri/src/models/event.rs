use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareEvent {
    pub timestamp: DateTime<Utc>,
    pub category: EventCategory,
    pub severity: EventSeverity,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventCategory {
    Usb,
    Display,
    Keyboard,
    Network,
    Rotation,
    Bluetooth,
    Service,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
}

impl HardwareEvent {
    pub fn info(
        category: EventCategory,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            category,
            severity: EventSeverity::Info,
            message: message.into(),
            source: source.into(),
        }
    }

    #[allow(dead_code)]
    pub fn warning(
        category: EventCategory,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            category,
            severity: EventSeverity::Warning,
            message: message.into(),
            source: source.into(),
        }
    }

    #[allow(dead_code)]
    pub fn error(
        category: EventCategory,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            category,
            severity: EventSeverity::Error,
            message: message.into(),
            source: source.into(),
        }
    }
}
