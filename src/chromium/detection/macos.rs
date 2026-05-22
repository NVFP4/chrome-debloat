use std::path::PathBuf;
use std::{env, fs};

use crate::chromium::Browser;
use crate::chromium::detection::BrowserInstall;

pub fn detect_browser(browser: Browser) -> Option<BrowserInstall> {
    application_roots().into_iter().find_map(|root| {
        app_bundle_names(browser)
            .iter()
            .map(|name| root.join(name))
            .find(|path| fs::metadata(path).is_ok())
            .map(|path| BrowserInstall::new(browser, path))
    })
}

fn application_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];

    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    roots
}

fn app_bundle_names(browser: Browser) -> &'static [&'static str] {
    match browser {
        Browser::Brave => &["Brave Browser.app"],
        Browser::Chrome => &["Google Chrome.app"],
        Browser::Edge => &["Microsoft Edge.app"],
    }
}
