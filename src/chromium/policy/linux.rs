use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::errors::{PolicyReadError, PolicyWriteError};
use super::target::{InvalidCustomBrowserId, validate_segments};
use super::writer::{PolicyWrite, PolicyWriteResult, write_file_atomically};
use super::{BrowserPolicy, PolicyLocation, PolicyReadResult, PolicySet, PolicyTarget, json};

pub(super) const CUSTOM_ID_HINT: &str = "Directory under /etc holding policies/managed, e.g. chromium for /etc/chromium/policies/managed";
const BUILTIN_IDS: [&str; 4] = ["brave", "opt/chrome", "opt/edge", "opt/yandex/browser"];

pub fn read(target: &PolicyTarget) -> PolicyReadResult {
    let path = policy_path(target);
    let source = PolicyLocation::File(path.clone());
    read_policy(target, path, source)
}

pub fn write(target: &PolicyTarget, policies: &PolicySet) -> PolicyWriteResult {
    let path = policy_path(target);
    let contents = json::policy_set_to_bytes(policies)?;
    write_file_atomically(&path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path),
        policy_count: policies.len(),
    })
}

pub fn export(_target: &PolicyTarget, policies: &PolicySet, path: &Path) -> PolicyWriteResult {
    let contents = json::policy_set_to_bytes(policies)?;
    write_file_atomically(path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path.to_path_buf()),
        policy_count: policies.len(),
    })
}

pub fn uninstall(target: &PolicyTarget) -> PolicyWriteResult {
    let path = policy_path(target);
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

pub fn export_file_extension() -> &'static str {
    "json"
}

pub fn managed_location(target: &PolicyTarget) -> PolicyLocation {
    PolicyLocation::File(policy_path(target))
}

pub(super) fn validate_custom_id(id: &str) -> Result<(), InvalidCustomBrowserId> {
    validate_segments(id, '/', |character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
    })?;
    if BUILTIN_IDS.contains(&id) {
        return Err(InvalidCustomBrowserId(
            "use the browser's own tab for this identifier",
        ));
    }

    Ok(())
}

fn read_policy(target: &PolicyTarget, path: PathBuf, source: PolicyLocation) -> PolicyReadResult {
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
            browser: target.browser(),
            source,
            policies,
        })
    })
}

fn policy_path(target: &PolicyTarget) -> PathBuf {
    match target {
        PolicyTarget::Brave => PathBuf::from("/etc/brave/policies/managed/brave.json"),
        PolicyTarget::Chrome => PathBuf::from("/etc/opt/chrome/policies/managed/chrome.json"),
        PolicyTarget::Edge => PathBuf::from("/etc/opt/edge/policies/managed/edge.json"),
        PolicyTarget::Yandex => {
            PathBuf::from("/etc/opt/yandex/browser/policies/managed/yandex.json")
        }
        PolicyTarget::Custom(id) => {
            let id = id.as_str();
            let file_name = id.rsplit('/').next().unwrap_or(id);
            Path::new("/etc")
                .join(id)
                .join("policies")
                .join("managed")
                .join(format!("{file_name}.json"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chromium::policy::CustomBrowserId;

    #[test]
    fn custom_ids_map_to_their_managed_policy_directory() -> Result<(), InvalidCustomBrowserId> {
        let target = PolicyTarget::Custom(CustomBrowserId::parse("opt/vivaldi")?);

        assert_eq!(
            policy_path(&target),
            PathBuf::from("/etc/opt/vivaldi/policies/managed/vivaldi.json")
        );
        assert!(CustomBrowserId::parse("/etc/chromium").is_err());
        assert!(CustomBrowserId::parse("opt/chrome").is_err());

        Ok(())
    }
}
