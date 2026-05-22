use crate::chromium::Browser;
use crate::chromium::detection::BrowserInstall;

pub const fn detect_browser(_browser: Browser) -> Option<BrowserInstall> {
    None
}
