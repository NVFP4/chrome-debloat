use std::borrow::Cow;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value as PlistValue};

use super::errors::{PolicyReadError, PolicyWriteError};
use super::target::{InvalidCustomBrowserId, validate_segments};
use super::writer::{PolicyWrite, PolicyWriteResult, write_file_atomically};
use super::{
    BrowserPolicy,
    PolicyLocation,
    PolicyReadResult,
    PolicySet,
    PolicyTarget,
    PolicyValue,
};

const ROOT_MUST_BE_OBJECT: &str = "root must be a dictionary/object";
const MANAGED_PREFERENCES: &str = "/Library/Managed Preferences";
const BUILTIN_IDS: [&str; 4] = [
    "com.brave.Browser",
    "com.google.Chrome",
    "com.microsoft.Edge",
    "ru.yandex.desktop.yandex-browser",
];

pub(super) const CUSTOM_ID_HINT: &str = "Bundle ID of the browser app, e.g. org.chromium.Chromium";

pub fn read(target: &PolicyTarget) -> PolicyReadResult {
    let path = policy_path(target);
    let source = PolicyLocation::File(path.clone());
    read_policy(target, path, source)
}

pub fn write(target: &PolicyTarget, policies: &PolicySet) -> PolicyWriteResult {
    let path = default_mobileconfig_path(target);
    let contents = mobileconfig_bytes(target, policies)?;
    write_file_atomically(&path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path),
        policy_count: policies.len(),
    })
}

pub fn export(target: &PolicyTarget, policies: &PolicySet, path: &Path) -> PolicyWriteResult {
    let contents = mobileconfig_bytes(target, policies)?;
    write_file_atomically(path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path.to_path_buf()),
        policy_count: policies.len(),
    })
}

pub fn export_file_extension() -> &'static str {
    "mobileconfig"
}

pub fn managed_location(target: &PolicyTarget) -> PolicyLocation {
    PolicyLocation::File(policy_path(target))
}

pub(super) fn validate_custom_id(id: &str) -> Result<(), InvalidCustomBrowserId> {
    validate_segments(id, '.', |character| {
        character.is_ascii_alphanumeric() || character == '-'
    })?;
    if !id.contains('.') {
        return Err(InvalidCustomBrowserId(
            "bundle ID needs at least two parts, e.g. org.chromium.Chromium",
        ));
    }
    if BUILTIN_IDS
        .iter()
        .any(|builtin| builtin.eq_ignore_ascii_case(id))
    {
        return Err(InvalidCustomBrowserId(
            "use the browser's own tab for this bundle ID",
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

    read_plist_policy(&path).map(|policies| {
        Some(BrowserPolicy {
            browser: target.browser(),
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

fn mobileconfig_bytes(
    target: &PolicyTarget,
    policies: &PolicySet,
) -> Result<Vec<u8>, PolicyWriteError> {
    let metadata = mobileconfig_metadata(target);
    let mut payload = Dictionary::new();

    insert_string(&mut payload, "PayloadIdentifier", &metadata.identifier);
    insert_string(&mut payload, "PayloadType", &metadata.identifier);
    insert_string(&mut payload, "PayloadUUID", &metadata.content_uuid);
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
    insert_string(&mut root, "PayloadUUID", &metadata.payload_uuid);
    insert_string(&mut root, "PayloadDisplayName", &metadata.display_name);
    insert_string(&mut root, "PayloadDescription", &metadata.description);
    insert_string(&mut root, "PayloadIdentifier", &metadata.identifier);
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
    display_name: Cow<'static, str>,
    description: Cow<'static, str>,
    identifier: Cow<'static, str>,
    payload_uuid: Cow<'static, str>,
    content_uuid: Cow<'static, str>,
}

fn mobileconfig_metadata(target: &PolicyTarget) -> MobileConfigMetadata {
    match target {
        PolicyTarget::Brave => MobileConfigMetadata::builtin(
            "Brave Policies",
            "Brave Browser system-level policies",
            "com.brave.Browser",
            "e143b891-3398-48f9-bee1-54d3b6db44b3",
            "88032831-5301-41ad-8231-10efa9d67ab3",
        ),
        PolicyTarget::Chrome => MobileConfigMetadata::builtin(
            "Google Chrome Policies",
            "Google Chrome Browser system-level policies",
            "com.google.Chrome",
            "8568e67e-21ba-4bdc-a944-a30fb301ba02",
            "3eb9eb1f-412c-4f8b-b425-f95f1a67072d",
        ),
        PolicyTarget::Edge => MobileConfigMetadata::builtin(
            "Microsoft Edge Policies",
            "Microsoft Edge Browser system-level policies",
            "com.microsoft.Edge",
            "778fb3c3-2e58-4337-86dc-1a8044793d2d",
            "65ffbe44-b556-4c33-88ea-ab684dab69bc",
        ),
        PolicyTarget::Yandex => MobileConfigMetadata::builtin(
            "Yandex Browser Policies",
            "Yandex Browser system-level policies",
            "ru.yandex.desktop.yandex-browser",
            "61496cb6-d284-4a04-805e-b5334f887b39",
            "395e405c-5300-45f7-a770-61bdb3f94165",
        ),
        PolicyTarget::Custom(id) => MobileConfigMetadata {
            display_name: Cow::Owned(format!("{id} Policies")),
            description: Cow::Owned(format!("{id} system-level policies")),
            identifier: Cow::Owned(id.as_str().to_owned()),
            payload_uuid: Cow::Owned(stable_uuid(id.as_str(), "payload")),
            content_uuid: Cow::Owned(stable_uuid(id.as_str(), "content")),
        },
    }
}

impl MobileConfigMetadata {
    const fn builtin(
        display_name: &'static str,
        description: &'static str,
        identifier: &'static str,
        payload_uuid: &'static str,
        content_uuid: &'static str,
    ) -> Self {
        Self {
            display_name: Cow::Borrowed(display_name),
            description: Cow::Borrowed(description),
            identifier: Cow::Borrowed(identifier),
            payload_uuid: Cow::Borrowed(payload_uuid),
            content_uuid: Cow::Borrowed(content_uuid),
        }
    }
}

/// A UUID derived from `id` and `salt`, so re-exporting a profile for the same bundle ID replaces
/// the installed one instead of adding a second profile.
fn stable_uuid(id: &str, salt: &str) -> String {
    let high = fnv1a64(0xcbf2_9ce4_8422_2325, [salt, id]);
    let low = fnv1a64(high, [id, salt]);
    // RFC 9562 version 8 (custom) with the RFC variant bits.
    let high = (high & !0xf000) | 0x8000;
    let low = (low & !(0b11 << 62)) | (0b10 << 62);

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        high >> 32,
        (high >> 16) & 0xffff,
        high & 0xffff,
        low >> 48,
        low & 0xffff_ffff_ffff,
    )
}

fn fnv1a64<const N: usize>(seed: u64, parts: [&str; N]) -> u64 {
    parts
        .iter()
        .flat_map(|part| part.bytes())
        .fold(seed, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

fn default_mobileconfig_path(target: &PolicyTarget) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chrome-debloat-{}.mobileconfig",
        target.browser().slug()
    ))
}

fn policy_path(target: &PolicyTarget) -> PathBuf {
    let id = match target {
        PolicyTarget::Brave => "com.brave.Browser",
        PolicyTarget::Chrome => "com.google.Chrome",
        PolicyTarget::Edge => "com.microsoft.Edge",
        PolicyTarget::Yandex => "ru.yandex.desktop.yandex-browser",
        PolicyTarget::Custom(id) => id.as_str(),
    };

    Path::new(MANAGED_PREFERENCES).join(format!("{id}.plist"))
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

        let bytes = mobileconfig_bytes(&PolicyTarget::Chrome, &policies)?;
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

        let result = mobileconfig_bytes(&PolicyTarget::Chrome, &policies);

        assert!(matches!(
            result,
            Err(PolicyWriteError::UnsupportedValue { policy, .. })
                if policy == "HomepageLocation"
        ));
    }

    #[test]
    fn custom_ids_must_be_bundle_ids_of_unknown_browsers() {
        assert!(validate_custom_id("org.chromium.Chromium").is_ok());
        assert!(validate_custom_id("Chromium").is_err());
        assert!(validate_custom_id("com.google.chrome").is_err());
    }

    #[test]
    fn custom_profile_uuids_are_stable_per_bundle_id() {
        let first = stable_uuid("org.chromium.Chromium", "payload");

        assert_eq!(first, stable_uuid("org.chromium.Chromium", "payload"));
        assert_ne!(first, stable_uuid("org.chromium.Chromium", "content"));
        assert_ne!(first, stable_uuid("com.vivaldi.Vivaldi", "payload"));
    }

    fn string(value: &str) -> PolicyValue {
        PolicyValue::String(value.to_owned())
    }
}
