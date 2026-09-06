use medousa_computer_bridge::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerActionReceipt, ComputerActionRequest,
    ComputerDriverPreflight, ComputerObservation, ComputerObservationRequest,
    ComputerPermissionKind, ComputerPermissionReport, ComputerPermissionStatus,
    ComputerScreenshotCapture, ComputerScreenshotRequest,
};
use medousa_world::WorldDriverId;

use super::PlatformDriverError;

pub struct NativeComputerDriver;

impl NativeComputerDriver {
    pub fn new() -> Self {
        Self
    }

    pub fn preflight(&self) -> ComputerDriverPreflight {
        ComputerDriverPreflight {
            protocol_version: COMPUTER_DRIVER_PROTOCOL_VERSION,
            driver_id: WorldDriverId::new("driver:computer:unsupported"),
            platform: std::env::consts::OS.to_string(),
            session_id: format!("unsupported:{}", std::process::id()),
            permissions: [
                ComputerPermissionKind::Accessibility,
                ComputerPermissionKind::ScreenCapture,
                ComputerPermissionKind::InputControl,
            ]
            .into_iter()
            .map(|permission| ComputerPermissionReport {
                permission,
                status: ComputerPermissionStatus::Unsupported,
                can_request: false,
                guidance: Some(
                    "This medousa-computer build does not include a driver for the host platform."
                        .to_string(),
                ),
            })
            .collect(),
            checked_at_ms: now_ms(),
        }
    }

    pub fn observe(
        &self,
        _request: ComputerObservationRequest,
    ) -> Result<ComputerObservation, PlatformDriverError> {
        Err(PlatformDriverError {
            code: "platform_unsupported",
            message: "native computer observation is not supported on this platform".to_string(),
            retryable: false,
        })
    }

    pub fn act(
        &self,
        _request: ComputerActionRequest,
    ) -> Result<ComputerActionReceipt, PlatformDriverError> {
        Err(PlatformDriverError {
            code: "platform_unsupported",
            message: "native computer actions are not supported on this platform".to_string(),
            retryable: false,
        })
    }

    pub fn screenshot(
        &self,
        _request: ComputerScreenshotRequest,
    ) -> Result<ComputerScreenshotCapture, PlatformDriverError> {
        Err(PlatformDriverError {
            code: "platform_unsupported",
            message: "native computer screenshots are not supported on this platform".to_string(),
            retryable: false,
        })
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
