use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::chromium::Browser;
use crate::chromium::detection::{BrowserDetectionError, BrowserDetectionResult, BrowserInstall};

pub fn detect_browser(browser: Browser) -> BrowserDetectionResult {
    for root in application_roots() {
        for path in app_bundle_names(browser).iter().map(|name| root.join(name)) {
            if app_bundle_exists(&path)? {
                return Ok(Some(BrowserInstall::new(browser, path)));
            }
        }
    }

    Ok(None)
}

fn application_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];

    if let Some(home) = env::home_dir() {
        roots.push(home.join("Applications"));
    }

    roots
}

fn app_bundle_exists(path: &Path) -> Result<bool, BrowserDetectionError> {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(BrowserDetectionError::Io {
                    action: "inspect application bundle",
                    path: path.to_path_buf(),
                    source: error,
                })
            }
        })
}

fn app_bundle_names(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["Brave Browser.app"],
        Browser::Chrome => &["Google Chrome.app"],
        Browser::Edge => &["Microsoft Edge.app"],
    }
}
