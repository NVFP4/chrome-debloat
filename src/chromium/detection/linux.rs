use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::chromium::Browser;
use crate::chromium::detection::{BrowserDetectionError, BrowserDetectionResult, BrowserInstall};

pub fn detect_browser(browser: Browser) -> BrowserDetectionResult {
    for command in executable_candidates(browser) {
        if let Some(path) = command_path(command)? {
            return Ok(Some(BrowserInstall::new(browser, path)));
        }
    }

    Ok(None)
}

fn executable_candidates(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["brave-browser", "brave"],
        Browser::Chrome => &["google-chrome", "google-chrome-stable", "chrome"],
        Browser::Edge => &["microsoft-edge", "microsoft-edge-stable", "msedge"],
    }
}

fn command_path(command: &str) -> Result<Option<PathBuf>, BrowserDetectionError> {
    let Some(paths) = env::var_os("PATH") else {
        return Ok(None);
    };

    for path in env::split_paths(&paths).map(|directory| directory.join(command)) {
        if is_executable(&path)? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn is_executable(path: &Path) -> Result<bool, BrowserDetectionError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && metadata.permissions().mode() & 0o111 != 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(BrowserDetectionError::Io {
            action: "inspect executable candidate",
            path: path.to_path_buf(),
            source,
        }),
    }
}
