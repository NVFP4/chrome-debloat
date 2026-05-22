use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use super::errors::{PolicyReadError, PolicyWriteError};
use super::writer::{PolicyWrite, PolicyWriteResult, write_file_atomically};
use super::{BrowserPolicy, PolicyLocation, PolicyReadResult, PolicySet, json};
use crate::chromium::Browser;

pub fn read(browser: Browser) -> PolicyReadResult {
    let path = policy_path(browser);
    let source = PolicyLocation::File(path.clone());
    read_policy(browser, path, source)
}

pub fn write(browser: Browser, policies: &PolicySet) -> PolicyWriteResult {
    let path = policy_path(browser);
    let contents = json::policy_set_to_bytes(policies)?;
    write_file_atomically(&path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path),
        policy_count: policies.len(),
    })
}

pub fn uninstall(browser: Browser) -> PolicyWriteResult {
    let path = policy_path(browser);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(source) => {
            return Err(PolicyWriteError::Io {
                action: "remove policy file",
                source,
            });
        }
    }

    Ok(PolicyWrite {
        target: PolicyLocation::File(path),
        policy_count: 0,
    })
}

pub fn managed_location(browser: Browser) -> PolicyLocation {
    PolicyLocation::File(policy_path(browser))
}

fn read_policy(browser: Browser, path: PathBuf, source: PolicyLocation) -> PolicyReadResult {
    match path.try_exists() {
        Ok(false) => return Ok(None),
        Ok(true) => {}
        Err(source) => {
            return Err(PolicyReadError::Io {
                action: "check policy path",
                source,
            });
        }
    }

    json::policy_set_from_file(&path).map(|policies| {
        Some(BrowserPolicy {
            browser,
            source,
            policies,
        })
    })
}

fn policy_path(browser: Browser) -> PathBuf {
    match browser {
        Browser::Brave => PathBuf::from("/etc/brave/policies/managed/brave.json"),
        Browser::Chrome => PathBuf::from("/etc/opt/chrome/policies/managed/chrome.json"),
        Browser::Edge => PathBuf::from("/etc/opt/edge/policies/managed/edge.json"),
    }
}
