#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
pub use macos::NativeComputerDriver;
#[cfg(not(target_os = "macos"))]
pub use unsupported::NativeComputerDriver;

#[derive(Debug)]
pub struct PlatformDriverError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}
