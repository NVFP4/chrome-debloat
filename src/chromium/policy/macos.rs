use std::path::{Path, PathBuf};

use plist::{Dictionary, Value as PlistValue};

use super::errors::{PolicyReadError, PolicyWriteError};
use super::writer::{PolicyWrite, PolicyWriteResult, write_file_atomically};
use super::{BrowserPolicy, PolicyLocation, PolicyReadResult, PolicySet, PolicyValue};
use crate::chromium::Browser;

const ROOT_MUST_BE_OBJECT: &str = "root must be a dictionary/object";

pub fn read(browser: Browser) -> PolicyReadResult {
    let path = policy_path(browser);
    let source = PolicyLocation::File(path.clone());
    read_policy(browser, path, source)
}

pub fn write(browser: Browser, policies: &PolicySet) -> PolicyWriteResult {
    let path = default_mobileconfig_path(browser);
    let contents = mobileconfig_bytes(browser, policies)?;
    write_file_atomically(&path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path),
        policy_count: policies.len(),
    })
}

pub fn export(browser: Browser, policies: &PolicySet, path: &Path) -> PolicyWriteResult {
    let contents = mobileconfig_bytes(browser, policies)?;
    write_file_atomically(path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path.to_path_buf()),
        policy_count: policies.len(),
    })
}

pub fn export_file_extension() -> &'static str {
    "mobileconfig"
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

    read_plist_policy(&path).map(|policies| {
        Some(BrowserPolicy {
            browser,
            source,
            policies,
        })
    })
}

fn read_plist_policy(path: &Path) -> Result<PolicySet, PolicyReadError> {
    let value = PlistValue::from_file(path).map_err(|source| PolicyReadError::Plist { source })?;
    read_plist(value)
}

fn read_plist(value: PlistValue) -> Result<PolicySet, PolicyReadError> {
    match value {
        PlistValue::Dictionary(values) => Ok(values
            .into_iter()
            .filter(|(key, _)| !is_macos_metadata_key(key))
            .map(|(key, value)| (key, read_value(value)))
            .collect()),
        PlistValue::Array(_)
        | PlistValue::Boolean(_)
        | PlistValue::Data(_)
        | PlistValue::Date(_)
        | PlistValue::Integer(_)
        | PlistValue::Real(_)
        | PlistValue::String(_)
        | PlistValue::Uid(_)
        | _ => Err(PolicyReadError::Invalid {
            reason: ROOT_MUST_BE_OBJECT,
        }),
    }
}

fn is_macos_metadata_key(key: &str) -> bool {
    matches!(key, "_manualProfile" | "PayloadUUID")
}

fn read_value(value: PlistValue) -> PolicyValue {
    match value {
        PlistValue::Boolean(value) => PolicyValue::Bool(value),
        PlistValue::Integer(value) => value
            .as_signed()
            .or_else(|| {
                value
                    .as_unsigned()
                    .and_then(|value| i64::try_from(value).ok())
            })
            .map(PolicyValue::Integer)
            .unwrap_or_else(|| PolicyValue::String(value.to_string())),
        PlistValue::String(value) => PolicyValue::String(value),
        PlistValue::Array(values) => {
            PolicyValue::List(values.into_iter().map(read_value).collect())
        }
        PlistValue::Dictionary(values) => PolicyValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, read_value(value)))
                .collect(),
        ),
        PlistValue::Data(value) => PolicyValue::String(format!("{} bytes", value.len())),
        PlistValue::Date(value) => PolicyValue::String(format!("{value:?}")),
        PlistValue::Real(value) => PolicyValue::String(value.to_string()),
        PlistValue::Uid(value) => PolicyValue::String(value.get().to_string()),
        other => PolicyValue::String(format!("{other:?}")),
    }
}

fn mobileconfig_bytes(browser: Browser, policies: &PolicySet) -> Result<Vec<u8>, PolicyWriteError> {
    let metadata = mobileconfig_metadata(browser);
    let mut payload = Dictionary::new();

    insert_string(&mut payload, "PayloadIdentifier", metadata.identifier);
    insert_string(&mut payload, "PayloadType", metadata.identifier);
    insert_string(&mut payload, "PayloadUUID", metadata.content_uuid);
    insert_integer(&mut payload, "PayloadVersion", 1);
    payload.insert("PayloadEnabled".to_owned(), PlistValue::Boolean(true));

    for (key, value) in policies {
        payload.insert(key.clone(), write_value(key, value)?);
    }

    let mut root = Dictionary::new();
    insert_integer(&mut root, "PayloadVersion", 1);
    insert_string(&mut root, "PayloadScope", "System");
    insert_string(&mut root, "PayloadType", "Configuration");
    root.insert(
        "PayloadRemovalDisallowed".to_owned(),
        PlistValue::Boolean(false),
    );
    insert_string(&mut root, "PayloadUUID", metadata.payload_uuid);
    insert_string(&mut root, "PayloadDisplayName", metadata.display_name);
    insert_string(&mut root, "PayloadDescription", metadata.description);
    insert_string(&mut root, "PayloadIdentifier", metadata.identifier);
    root.insert(
        "PayloadContent".to_owned(),
        PlistValue::Array(vec![PlistValue::Dictionary(payload)]),
    );

    let mut bytes = Vec::new();
    PlistValue::Dictionary(root)
        .to_writer_xml(&mut bytes)
        .map_err(|source| PolicyWriteError::Plist { source })?;

    Ok(bytes)
}

fn write_value(policy: &str, value: &PolicyValue) -> Result<PlistValue, PolicyWriteError> {
    match value {
        PolicyValue::Bool(value) => Ok(PlistValue::Boolean(*value)),
        PolicyValue::Integer(value) => Ok(PlistValue::Integer((*value).into())),
        PolicyValue::String(value) => Ok(PlistValue::String(value.clone())),
        PolicyValue::List(values) => values
            .iter()
            .map(|value| write_value(policy, value))
            .collect::<Result<Vec<_>, _>>()
            .map(PlistValue::Array),
        PolicyValue::Object(values) => {
            let mut dictionary = Dictionary::new();

            for (key, value) in values {
                dictionary.insert(key.clone(), write_value(policy, value)?);
            }

            Ok(PlistValue::Dictionary(dictionary))
        }
        PolicyValue::Null => Err(PolicyWriteError::UnsupportedValue {
            policy: policy.to_owned(),
            reason: "configuration profiles do not support null values",
        }),
    }
}

fn insert_string(dictionary: &mut Dictionary, key: &str, value: &str) {
    dictionary.insert(key.to_owned(), PlistValue::String(value.to_owned()));
}

fn insert_integer(dictionary: &mut Dictionary, key: &str, value: i64) {
    dictionary.insert(key.to_owned(), PlistValue::Integer(value.into()));
}

struct MobileConfigMetadata {
    display_name: &'static str,
    description: &'static str,
    identifier: &'static str,
    payload_uuid: &'static str,
    content_uuid: &'static str,
}

fn mobileconfig_metadata(browser: Browser) -> MobileConfigMetadata {
    match browser {
        Browser::Brave => MobileConfigMetadata {
            display_name: "Brave Policies",
            description: "Brave Browser system-level policies",
            identifier: "com.brave.Browser",
            payload_uuid: "e143b891-3398-48f9-bee1-54d3b6db44b3",
            content_uuid: "88032831-5301-41ad-8231-10efa9d67ab3",
        },
        Browser::Chrome => MobileConfigMetadata {
            display_name: "Google Chrome Policies",
            description: "Google Chrome Browser system-level policies",
            identifier: "com.google.Chrome",
            payload_uuid: "8568e67e-21ba-4bdc-a944-a30fb301ba02",
            content_uuid: "3eb9eb1f-412c-4f8b-b425-f95f1a67072d",
        },
        Browser::Edge => MobileConfigMetadata {
            display_name: "Microsoft Edge Policies",
            description: "Microsoft Edge Browser system-level policies",
            identifier: "com.microsoft.Edge",
            payload_uuid: "778fb3c3-2e58-4337-86dc-1a8044793d2d",
            content_uuid: "65ffbe44-b556-4c33-88ea-ab684dab69bc",
        },
    }
}

fn default_mobileconfig_path(browser: Browser) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chrome-debloat-{}.mobileconfig",
        browser_slug(browser)
    ))
}

fn browser_slug(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => "brave",
        Browser::Chrome => "chrome",
        Browser::Edge => "edge",
    }
}

fn policy_path(browser: Browser) -> PathBuf {
    match browser {
        Browser::Brave => PathBuf::from("/Library/Managed Preferences/com.brave.Browser.plist"),
        Browser::Chrome => PathBuf::from("/Library/Managed Preferences/com.google.Chrome.plist"),
        Browser::Edge => PathBuf::from("/Library/Managed Preferences/com.microsoft.Edge.plist"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor};

    use super::*;

    #[test]
    fn mobileconfig_wraps_browser_payload() -> Result<(), Box<dyn std::error::Error>> {
        let mut policies = PolicySet::new();
        policies.insert("HomepageLocation".to_owned(), string("https://example.com"));
        policies.insert(
            "ExtensionInstallForcelist".to_owned(),
            PolicyValue::List(vec![string("abc;https://example.com/update.xml")]),
        );

        let bytes = mobileconfig_bytes(Browser::Chrome, &policies)?;
        let profile = PlistValue::from_reader(Cursor::new(bytes))?;
        let root = profile
            .as_dictionary()
            .ok_or_else(|| io::Error::other("profile root should be a dictionary"))?;
        let payloads = root
            .get("PayloadContent")
            .and_then(PlistValue::as_array)
            .ok_or_else(|| io::Error::other("profile should contain payloads"))?;
        let payload = payloads
            .first()
            .and_then(PlistValue::as_dictionary)
            .ok_or_else(|| io::Error::other("profile should contain browser payload"))?;

        assert_eq!(
            root.get("PayloadType").and_then(PlistValue::as_string),
            Some("Configuration")
        );
        assert_eq!(
            payload.get("PayloadType").and_then(PlistValue::as_string),
            Some("com.google.Chrome")
        );
        assert_eq!(
            payload
                .get("HomepageLocation")
                .and_then(PlistValue::as_string),
            Some("https://example.com")
        );
        assert_eq!(
            payload
                .get("ExtensionInstallForcelist")
                .and_then(PlistValue::as_array)
                .map(Vec::len),
            Some(1)
        );

        Ok(())
    }

    #[test]
    fn mobileconfig_rejects_null_policy_values() {
        let mut policies = PolicySet::new();
        policies.insert("HomepageLocation".to_owned(), PolicyValue::Null);

        let result = mobileconfig_bytes(Browser::Chrome, &policies);

        assert!(matches!(
            result,
            Err(PolicyWriteError::UnsupportedValue { policy, .. })
                if policy == "HomepageLocation"
        ));
    }

    fn string(value: &str) -> PolicyValue {
        PolicyValue::String(value.to_owned())
    }
}
