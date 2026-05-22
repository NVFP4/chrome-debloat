use super::BrowserPolicy;
use super::errors::PolicyReadError;
#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(target_os = "macos")]
use super::macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use super::unsupported as platform;
#[cfg(target_os = "windows")]
use super::windows as platform;
use crate::chromium::Browser;

pub type PolicyReadResult = Result<Option<BrowserPolicy>, PolicyReadError>;

pub fn read(browser: Browser) -> PolicyReadResult {
    platform::read(browser)
}

// pub fn read_all() -> HashMap<Browser, PolicyReadResult> {
//     Browser::all()
//         .into_iter()
//         .map(|browser| (browser, read(browser)))
//         .collect()
// }
