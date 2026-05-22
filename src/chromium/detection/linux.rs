use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::chromium::Browser;
use crate::chromium::detection::BrowserInstall;

pub fn detect_browser(browser: Browser) -> Option<BrowserInstall> {
    executable_candidates(browser)
        .iter()
        .find_map(|command| command_path(command))
        .map(|path| BrowserInstall::new(browser, path))
}

fn executable_candidates(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["brave-browser", "brave"],
        Browser::Chrome => &["google-chrome", "google-chrome-stable", "chrome"],
        Browser::Edge => &["microsoft-edge", "microsoft-edge-stable", "msedge"],
    }
}

fn command_path(command: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(command))
            .find(|path| is_executable(path))
    })
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}
