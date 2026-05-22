use std::env;
use std::path::PathBuf;

use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};

use crate::chromium::Browser;
use crate::chromium::detection::BrowserInstall;

pub fn detect_browser(browser: Browser) -> Option<BrowserInstall> {
    app_path(browser)
        .or_else(|| {
            common_install_paths(browser)
                .into_iter()
                .find(|path| path.exists())
        })
        .map(|path| BrowserInstall::new(browser, path))
}

fn app_path(browser: Browser) -> Option<PathBuf> {
    let exe_name = browser_exe_name(browser);
    [LOCAL_MACHINE, CURRENT_USER]
        .into_iter()
        .find_map(|root| app_path_registry_value(root, exe_name))
        .map(PathBuf::from)
}

fn app_path_registry_value(root: &Key, exe_name: &str) -> Option<String> {
    let key = root
        .open(format!(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe_name}"
        ))
        .ok()?;

    key.get_string("").ok()
}

fn browser_exe_name(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => "brave.exe",
        Browser::Chrome => "chrome.exe",
        Browser::Edge => "msedge.exe",
    }
}

fn common_install_paths(browser: Browser) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    match browser {
        Browser::Brave => {
            append_env_path(
                &mut paths,
                "ProgramFiles",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            );
            append_env_path(
                &mut paths,
                "ProgramFiles(x86)",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            );
            append_env_path(
                &mut paths,
                "LocalAppData",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            );
        }
        Browser::Chrome => {
            append_env_path(
                &mut paths,
                "ProgramFiles",
                r"Google\Chrome\Application\chrome.exe",
            );
            append_env_path(
                &mut paths,
                "ProgramFiles(x86)",
                r"Google\Chrome\Application\chrome.exe",
            );
            append_env_path(
                &mut paths,
                "LocalAppData",
                r"Google\Chrome\Application\chrome.exe",
            );
        }
        Browser::Edge => {
            append_env_path(
                &mut paths,
                "ProgramFiles",
                r"Microsoft\Edge\Application\msedge.exe",
            );
            append_env_path(
                &mut paths,
                "ProgramFiles(x86)",
                r"Microsoft\Edge\Application\msedge.exe",
            );
            append_env_path(
                &mut paths,
                "LocalAppData",
                r"Microsoft\Edge\Application\msedge.exe",
            );
        }
    }

    paths
}

fn append_env_path(paths: &mut Vec<PathBuf>, variable: &str, suffix: &str) {
    if let Some(root) = env::var_os(variable) {
        paths.push(PathBuf::from(root).join(suffix));
    }
}
