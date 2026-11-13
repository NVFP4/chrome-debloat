use super::errors::PolicyReadError;
#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(target_os = "macos")]
use super::macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use super::unsupported as platform;
#[cfg(target_os = "windows")]
use super::windows as platform;
use super::{BrowserPolicy, PolicyTarget};

pub type PolicyReadResult = Result<Option<BrowserPolicy>, PolicyReadError>;

pub fn read(target: &PolicyTarget) -> PolicyReadResult {
    platform::read(target)
}
