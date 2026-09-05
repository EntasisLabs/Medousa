use std::collections::BTreeSet;

use medousa_world::{WorldDriverId, WorldResourceId};
use serde::{Deserialize, Serialize};

pub const COMPUTER_DRIVER_PROTOCOL_VERSION: u16 = 1;
pub const COMPUTER_OBSERVATION_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT: u32 = 2_048;
pub const MAX_COMPUTER_OBSERVATION_NODES: u32 = 4_096;
pub const MAX_COMPUTER_OBSERVATION_DISPLAYS: usize = 32;
pub const MAX_COMPUTER_OBSERVATION_APPLICATIONS: usize = 256;
pub const MAX_COMPUTER_OBSERVATION_WINDOWS: usize = 1_024;
pub const MAX_COMPUTER_OBSERVATION_TEXT_BYTES: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerPermissionKind {
    Accessibility,
    ScreenCapture,
    InputControl,
}

impl ComputerPermissionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ScreenCapture => "screen_capture",
            Self::InputControl => "input_control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerPermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Restricted,
    Unsupported,
}

impl ComputerPermissionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::NotDetermined => "not_determined",
            Self::Restricted => "restricted",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerPermissionReport {
    pub permission: ComputerPermissionKind,
    pub status: ComputerPermissionStatus,
    pub can_request: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerDriverPreflight {
    pub protocol_version: u16,
    pub driver_id: WorldDriverId,
    pub platform: String,
    pub session_id: String,
    pub permissions: Vec<ComputerPermissionReport>,
    pub checked_at_ms: u64,
}

impl ComputerDriverPreflight {
    pub fn permission_status(
        &self,
        permission: ComputerPermissionKind,
    ) -> ComputerPermissionStatus {
        self.permissions
            .iter()
            .find(|report| report.permission == permission)
            .map(|report| report.status)
            .unwrap_or(ComputerPermissionStatus::Unsupported)
    }

    pub fn semantic_observation_ready(&self) -> bool {
        self.permission_status(ComputerPermissionKind::Accessibility)
            == ComputerPermissionStatus::Granted
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerDisplay {
    pub resource_id: WorldResourceId,
    pub name: String,
    pub frame: ComputerRect,
    pub scale_factor: f64,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerApplication {
    pub resource_id: WorldResourceId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerWindow {
    pub resource_id: WorldResourceId,
    pub application_resource_id: WorldResourceId,
    pub title: String,
    pub frame: ComputerRect,
    pub minimized: bool,
    pub focused: bool,
}

/// Bounded accessibility projection produced by a colocated driver.
///
/// Element references are opaque and scoped to one observation generation.
/// A sensitive node must never include its current value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerSemanticNode {
    pub element_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_ref: Option<String>,
    pub window_resource_id: WorldResourceId,
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ComputerRect>,
    pub enabled: bool,
    pub focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default)]
    pub sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerObservationRequest {
    pub resource_id: WorldResourceId,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_revision: Option<u64>,
    #[serde(default = "default_node_limit")]
    pub max_nodes: u32,
}

impl ComputerObservationRequest {
    pub fn new(resource_id: impl Into<WorldResourceId>, session_id: impl Into<String>) -> Self {
        Self {
            resource_id: resource_id.into(),
            session_id: session_id.into(),
            after_revision: None,
            max_nodes: DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("computer resource", self.resource_id.as_str())?;
        validate_identifier("computer session", &self.session_id)?;
        if self.max_nodes == 0 || self.max_nodes > MAX_COMPUTER_OBSERVATION_NODES {
            return Err(format!(
                "computer observation max_nodes must be between 1 and {MAX_COMPUTER_OBSERVATION_NODES}"
            ));
        }
        Ok(())
    }
}

fn default_node_limit() -> u32 {
    DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT
}

/// Full semantic state or a delta after the requested revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerObservation {
    pub schema_version: u16,
    pub driver_id: WorldDriverId,
    pub resource_id: WorldResourceId,
    pub session_id: String,
    pub observation_generation: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
    pub full: bool,
    #[serde(default)]
    pub unchanged: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_application_resource_id: Option<WorldResourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_window_resource_id: Option<WorldResourceId>,
    #[serde(default)]
    pub displays: Vec<ComputerDisplay>,
    #[serde(default)]
    pub applications: Vec<ComputerApplication>,
    #[serde(default)]
    pub windows: Vec<ComputerWindow>,
    #[serde(default)]
    pub nodes: Vec<ComputerSemanticNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_refs: Vec<String>,
    #[serde(default)]
    pub truncated: bool,
    pub captured_at_ms: u64,
    /// Accessibility text is application-provided data, never runtime policy.
    pub untrusted_content: bool,
}

impl ComputerObservation {
    pub fn validate_for(
        &self,
        driver_id: &WorldDriverId,
        request: &ComputerObservationRequest,
    ) -> Result<(), String> {
        if self.schema_version != COMPUTER_OBSERVATION_SCHEMA_VERSION {
            return Err(format!(
                "unsupported computer observation schema {}",
                self.schema_version
            ));
        }
        if &self.driver_id != driver_id {
            return Err("computer observation came from the wrong driver".to_string());
        }
        if self.resource_id != request.resource_id {
            return Err("computer observation came from the wrong resource".to_string());
        }
        validate_identifier("computer session", &self.session_id)?;
        if self.session_id != request.session_id {
            return Err("computer observation came from the wrong desktop session".to_string());
        }
        validate_identifier("observation generation", &self.observation_generation)?;
        if self.nodes.len() > request.max_nodes as usize {
            return Err("computer observation exceeded its requested node bound".to_string());
        }
        if self.displays.len() > MAX_COMPUTER_OBSERVATION_DISPLAYS
            || self.applications.len() > MAX_COMPUTER_OBSERVATION_APPLICATIONS
            || self.windows.len() > MAX_COMPUTER_OBSERVATION_WINDOWS
        {
            return Err("computer observation exceeded its resource bounds".to_string());
        }
        if !self.untrusted_content {
            return Err("computer observation must mark application content untrusted".to_string());
        }
        if self.full {
            if self.base_revision.is_some() || self.unchanged {
                return Err("full computer observation cannot be a delta or unchanged".to_string());
            }
            if !self.removed_refs.is_empty() {
                return Err(
                    "full computer observation cannot remove element references".to_string()
                );
            }
        } else {
            let base_revision = self
                .base_revision
                .ok_or_else(|| "computer observation delta is missing base_revision".to_string())?;
            if request.after_revision != Some(base_revision) {
                return Err(
                    "computer observation delta does not extend the requested revision".to_string(),
                );
            }
            if (self.unchanged && self.revision != base_revision)
                || (!self.unchanged && self.revision <= base_revision)
            {
                return Err("computer observation delta has an invalid revision".to_string());
            }
            if self.unchanged
                && (!self.displays.is_empty()
                    || !self.applications.is_empty()
                    || !self.windows.is_empty()
                    || !self.nodes.is_empty()
                    || !self.removed_refs.is_empty())
            {
                return Err("unchanged computer observation must not contain a delta".to_string());
            }
        }

        let mut resources = BTreeSet::new();
        for display in &self.displays {
            validate_identifier("computer display resource", display.resource_id.as_str())?;
            validate_text("computer display name", &display.name)?;
            if !display.scale_factor.is_finite()
                || display.scale_factor <= 0.0
                || display.scale_factor > 16.0
            {
                return Err("computer display scale factor is invalid".to_string());
            }
            if !resources.insert(display.resource_id.as_str()) {
                return Err("computer observation contains duplicate resources".to_string());
            }
        }
        for application in &self.applications {
            validate_identifier(
                "computer application resource",
                application.resource_id.as_str(),
            )?;
            validate_text("computer application name", &application.name)?;
            if let Some(application_id) = application.application_id.as_deref() {
                validate_text("computer application identity", application_id)?;
            }
            if !resources.insert(application.resource_id.as_str()) {
                return Err("computer observation contains duplicate resources".to_string());
            }
        }
        for window in &self.windows {
            validate_identifier("computer window resource", window.resource_id.as_str())?;
            validate_identifier(
                "computer window application resource",
                window.application_resource_id.as_str(),
            )?;
            validate_text("computer window title", &window.title)?;
            if !resources.insert(window.resource_id.as_str()) {
                return Err("computer observation contains duplicate resources".to_string());
            }
        }
        if self.full {
            if let Some(active) = self.active_application_resource_id.as_ref()
                && !self
                    .applications
                    .iter()
                    .any(|application| &application.resource_id == active && application.active)
            {
                return Err("active computer application is missing from the snapshot".to_string());
            }
            if let Some(focused) = self.focused_window_resource_id.as_ref()
                && !self
                    .windows
                    .iter()
                    .any(|window| &window.resource_id == focused && window.focused)
            {
                return Err("focused computer window is missing from the snapshot".to_string());
            }
            if self.windows.iter().any(|window| {
                !self
                    .applications
                    .iter()
                    .any(|application| application.resource_id == window.application_resource_id)
            }) {
                return Err("computer window references an unknown application".to_string());
            }
        }

        let mut refs = BTreeSet::new();
        for node in &self.nodes {
            validate_identifier("computer element reference", &node.element_ref)?;
            validate_identifier(
                "computer element window resource",
                node.window_resource_id.as_str(),
            )?;
            validate_text("computer element role", &node.role)?;
            validate_text("computer element name", &node.name)?;
            if let Some(value) = node.value.as_deref() {
                validate_text("computer element value", value)?;
            }
            if !refs.insert(node.element_ref.as_str()) {
                return Err(
                    "computer observation contains duplicate element references".to_string()
                );
            }
            if node.sensitive && node.value.is_some() {
                return Err("sensitive computer node exposed its value".to_string());
            }
            if self.full
                && !self
                    .windows
                    .iter()
                    .any(|window| window.resource_id == node.window_resource_id)
            {
                return Err("computer element references an unknown window".to_string());
            }
        }
        let mut removed_refs = BTreeSet::new();
        for element_ref in &self.removed_refs {
            validate_identifier("removed computer element reference", element_ref)?;
            if !removed_refs.insert(element_ref.as_str()) || refs.contains(element_ref.as_str()) {
                return Err(
                    "computer observation contains conflicting element references".to_string(),
                );
            }
        }
        Ok(())
    }
}

fn validate_identifier(label: &str, value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 256 || trimmed.chars().any(char::is_control) {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str) -> Result<(), String> {
    if value.len() > MAX_COMPUTER_OBSERVATION_TEXT_BYTES {
        return Err(format!("{label} exceeded its byte bound"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> ComputerObservation {
        ComputerObservation {
            schema_version: COMPUTER_OBSERVATION_SCHEMA_VERSION,
            driver_id: WorldDriverId::new("driver:computer:test"),
            resource_id: WorldResourceId::new("desktop:session"),
            session_id: "login-session:test".to_string(),
            observation_generation: "generation:one".to_string(),
            revision: 1,
            base_revision: None,
            full: true,
            unchanged: false,
            active_application_resource_id: None,
            focused_window_resource_id: None,
            displays: Vec::new(),
            applications: vec![ComputerApplication {
                resource_id: WorldResourceId::new("application:one"),
                name: "Test application".to_string(),
                application_id: None,
                process_id: Some(42),
                active: false,
            }],
            windows: vec![ComputerWindow {
                resource_id: WorldResourceId::new("window:one"),
                application_resource_id: WorldResourceId::new("application:one"),
                title: "Test window".to_string(),
                frame: ComputerRect::default(),
                minimized: false,
                focused: false,
            }],
            nodes: vec![ComputerSemanticNode {
                element_ref: "ax:button:one".to_string(),
                parent_ref: None,
                window_resource_id: WorldResourceId::new("window:one"),
                role: "button".to_string(),
                name: "Continue".to_string(),
                value: None,
                bounds: None,
                enabled: true,
                focused: false,
                selected: None,
                sensitive: false,
            }],
            removed_refs: Vec::new(),
            truncated: false,
            captured_at_ms: 1,
            untrusted_content: true,
        }
    }

    #[test]
    fn observation_contract_round_trips() {
        let observation = observation();
        let encoded = serde_json::to_vec(&observation).expect("encode observation");
        let decoded: ComputerObservation =
            serde_json::from_slice(&encoded).expect("decode observation");
        assert_eq!(decoded, observation);
        decoded
            .validate_for(
                &WorldDriverId::new("driver:computer:test"),
                &ComputerObservationRequest::new("desktop:session", "login-session:test"),
            )
            .expect("valid observation");
    }

    #[test]
    fn sensitive_values_never_cross_the_bridge() {
        let mut observation = observation();
        observation.nodes[0].sensitive = true;
        observation.nodes[0].value = Some("secret".to_string());
        let error = observation
            .validate_for(
                &WorldDriverId::new("driver:computer:test"),
                &ComputerObservationRequest::new("desktop:session", "login-session:test"),
            )
            .expect_err("sensitive values must fail");
        assert!(error.contains("exposed its value"));
    }

    #[test]
    fn deltas_must_extend_the_requested_revision() {
        let mut observation = observation();
        observation.full = false;
        observation.base_revision = Some(3);
        observation.revision = 4;
        let mut request = ComputerObservationRequest::new("desktop:session", "login-session:test");
        request.after_revision = Some(2);
        let error = observation
            .validate_for(&WorldDriverId::new("driver:computer:test"), &request)
            .expect_err("mismatched delta must fail");
        assert!(error.contains("does not extend"));
    }

    #[test]
    fn observation_cannot_cross_desktop_sessions() {
        let observation = observation();
        let error = observation
            .validate_for(
                &WorldDriverId::new("driver:computer:test"),
                &ComputerObservationRequest::new("desktop:session", "login-session:other"),
            )
            .expect_err("desktop session mismatch must fail");
        assert!(error.contains("wrong desktop session"));
    }
}
