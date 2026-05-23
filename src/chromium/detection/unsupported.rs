use crate::chromium::Browser;
use crate::chromium::detection::{BrowserDetectionError, BrowserDetectionResult};

pub const fn detect_browser(_browser: Browser) -> BrowserDetectionResult {
    Err(BrowserDetectionError::UnsupportedPlatform)
}
