use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::chromium::Browser;

pub type PolicySet = BTreeMap<String, PolicyValue>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPolicy {
    pub browser: Browser,
    pub source: PolicyLocation,
    pub policies: PolicySet,
}

impl BrowserPolicy {
    pub fn policy_count(&self) -> usize {
        self.policies.len()
    }

    pub fn extension_count(&self) -> usize {
        self.policies
            .get("ExtensionInstallForcelist")
            .and_then(PolicyValue::list_len)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PolicyValue {
    #[allow(dead_code)] // JSON/plist policies can produce boolean values on other targets.
    Bool(bool),
    Integer(i64),
    String(String),
    List(Vec<PolicyValue>),
    #[allow(dead_code)] // JSON/plist policies can produce object values on other targets.
    Object(BTreeMap<String, PolicyValue>),
    #[allow(dead_code)] // Linux JSON policy files can contain null values.
    Null,
}

impl PolicyValue {
    pub(crate) const fn list_len(&self) -> Option<usize> {
        match self {
            Self::List(values) => Some(values.len()),
            Self::Bool(_) | Self::Integer(_) | Self::String(_) | Self::Object(_) | Self::Null => {
                None
            }
        }
    }
}

// Backends construct different source variants on different targets.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyLocation {
    File(PathBuf),
    #[cfg(target_os = "windows")]
    RegistryKey(String),
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    UnsupportedPlatform,
}

impl fmt::Display for PolicyLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(formatter, "{}", path.display()),
            #[cfg(target_os = "windows")]
            Self::RegistryKey(key) => formatter.write_str(key),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            Self::UnsupportedPlatform => formatter.write_str("unsupported platform"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn browser_policy_counts_top_level_policies_and_extensions() {
        let mut policies = PolicySet::new();
        policies.insert("HomepageLocation".to_owned(), string("https://example.com"));
        policies.insert(
            "ExtensionInstallForcelist".to_owned(),
            PolicyValue::List(vec![string("first"), string("second")]),
        );

        let policy = BrowserPolicy {
            browser: Browser::Chrome,
            source: PolicyLocation::File(PathBuf::from("chrome.json")),
            policies,
        };

        assert_eq!(policy.policy_count(), 2);
        assert_eq!(policy.extension_count(), 2);
    }

    #[test]
    fn extension_count_ignores_absent_or_non_list_policy() {
        let policy = BrowserPolicy {
            browser: Browser::Chrome,
            source: PolicyLocation::File(PathBuf::from("chrome.json")),
            policies: PolicySet::new(),
        };

        assert_eq!(policy.extension_count(), 0);

        let mut policies = PolicySet::new();
        policies.insert("ExtensionInstallForcelist".to_owned(), string("not-a-list"));

        let policy = BrowserPolicy {
            browser: Browser::Chrome,
            source: PolicyLocation::File(PathBuf::from("chrome.json")),
            policies,
        };

        assert_eq!(policy.extension_count(), 0);
    }

    #[test]
    fn file_policy_location_displays_path() {
        let location = PolicyLocation::File(PathBuf::from("managed/policy.json"));

        assert_eq!(location.to_string(), "managed/policy.json");
    }

    fn string(value: &str) -> PolicyValue {
        PolicyValue::String(value.to_owned())
    }
}
