use std::path::Path;

use super::errors::{PolicyReadError, PolicyWriteError};
use super::target::InvalidCustomBrowserId;
use super::writer::PolicyWriteResult;
use super::{PolicyLocation, PolicyReadResult, PolicySet, PolicyTarget};

pub(super) const CUSTOM_ID_HINT: &str = "Custom browsers are not supported on this platform";

pub fn read(_target: &PolicyTarget) -> PolicyReadResult {
    Err(PolicyReadError::UnsupportedPlatform)
}

pub fn write(_target: &PolicyTarget, _policies: &PolicySet) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn export(_target: &PolicyTarget, _policies: &PolicySet, _path: &Path) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn export_file_extension() -> &'static str {
    "policy"
}

pub fn uninstall(_target: &PolicyTarget) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn managed_location(_target: &PolicyTarget) -> PolicyLocation {
    PolicyLocation::UnsupportedPlatform
}

pub(super) fn validate_custom_id(_id: &str) -> Result<(), InvalidCustomBrowserId> {
    Err(InvalidCustomBrowserId(CUSTOM_ID_HINT))
}
