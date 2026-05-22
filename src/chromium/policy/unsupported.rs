use std::path::Path;

use super::errors::{PolicyReadError, PolicyWriteError};
use super::writer::PolicyWriteResult;
use super::{PolicyLocation, PolicyReadResult, PolicySet};
use crate::chromium::Browser;

pub fn read(_browser: Browser) -> PolicyReadResult {
    Err(PolicyReadError::UnsupportedPlatform)
}

pub fn write(_browser: Browser, _policies: &PolicySet) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn export(_browser: Browser, _policies: &PolicySet, _path: &Path) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn export_file_extension() -> &'static str {
    "policy"
}

pub fn uninstall(_browser: Browser) -> PolicyWriteResult {
    Err(PolicyWriteError::UnsupportedPlatform)
}

pub fn managed_location(_browser: Browser) -> PolicyLocation {
    PolicyLocation::UnsupportedPlatform
}
