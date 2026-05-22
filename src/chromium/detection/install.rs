use std::path::PathBuf;

use crate::chromium::Browser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserInstall {
    pub browser: Browser,
    pub executable_path: PathBuf,
    pub version: Option<String>,
}

impl BrowserInstall {
    pub fn new(browser: Browser, executable_path: PathBuf) -> Self {
        Self {
            browser,
            executable_path,
            version: None,
        }
    }
}
