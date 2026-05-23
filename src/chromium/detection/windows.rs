use std::path::{Path, PathBuf};

use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};

use crate::chromium::Browser;
use crate::chromium::detection::{BrowserDetectionError, BrowserDetectionResult, BrowserInstall};

const HRESULT_FROM_WIN32_FILE_NOT_FOUND: i32 = -2147024894;
const HRESULT_FROM_WIN32_PATH_NOT_FOUND: i32 = -2147024893;

pub fn detect_browser(browser: Browser) -> BrowserDetectionResult {
    let Some(path) = app_path(browser)? else {
        return Ok(None);
    };

    if path_exists(&path)? {
        Ok(Some(BrowserInstall::new(browser, path)))
    } else {
        Ok(None)
    }
}

fn app_path(browser: Browser) -> Result<Option<PathBuf>, BrowserDetectionError> {
    let exe_name = browser_exe_name(browser);
    for root in [LOCAL_MACHINE, CURRENT_USER] {
        if let Some(path) = app_path_registry_value(root, exe_name)? {
            return Ok(Some(PathBuf::from(path)));
        }
    }

    Ok(None)
}

fn app_path_registry_value(
    root: &Key,
    exe_name: &str,
) -> Result<Option<String>, BrowserDetectionError> {
    let key_path = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe_name}");
    let key = match root.open(&key_path) {
        Ok(key) => key,
        Err(error) if is_missing_key(error.code().0) => return Ok(None),
        Err(error) => {
            return Err(BrowserDetectionError::registry(
                "open Windows app path registry key",
                error.code().0,
                error.to_string(),
            ));
        }
    };

    match key.get_string("") {
        Ok(path) => Ok(Some(path)),
        Err(error) if is_missing_key(error.code().0) => Ok(None),
        Err(error) => Err(BrowserDetectionError::registry(
            "read Windows app path registry value",
            error.code().0,
            error.to_string(),
        )),
    }
}

fn browser_exe_name(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => "brave.exe",
        Browser::Chrome => "chrome.exe",
        Browser::Edge => "msedge.exe",
    }
}

fn path_exists(path: &Path) -> Result<bool, BrowserDetectionError> {
    path.try_exists()
        .map_err(|source| BrowserDetectionError::Io {
            action: "check Windows app path executable",
            path: path.to_path_buf(),
            source,
        })
}

fn is_missing_key(code: i32) -> bool {
    matches!(
        code,
        HRESULT_FROM_WIN32_FILE_NOT_FOUND | HRESULT_FROM_WIN32_PATH_NOT_FOUND
    )
}
