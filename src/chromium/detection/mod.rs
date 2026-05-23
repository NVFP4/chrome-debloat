mod errors;
mod install;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

pub use errors::BrowserDetectionError;
pub use install::BrowserInstall;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as platform;
#[cfg(target_os = "windows")]
use windows as platform;

use crate::chromium::Browser;

pub type BrowserDetectionResult = Result<Option<BrowserInstall>, BrowserDetectionError>;

pub fn detect(browser: Browser) -> BrowserDetectionResult {
    platform::detect_browser(browser)
}
