use std::collections::BTreeMap;
use std::path::Path;

use windows_registry::{Key, LOCAL_MACHINE, Type, Value as RegistryValue};

use super::errors::{PolicyReadError, PolicyWriteError};
use super::writer::{PolicyWrite, PolicyWriteResult, write_file_atomically};
use super::{BrowserPolicy, PolicyLocation, PolicyReadResult, PolicySet, PolicyValue};
use crate::chromium::Browser;

// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND/PATH_NOT_FOUND); see:
// https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-
// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/0c0bcf55-277e-4120-b5dc-f6115fc8dc38
const HRESULT_FROM_WIN32_FILE_NOT_FOUND: i32 = -2147024894;
const HRESULT_FROM_WIN32_PATH_NOT_FOUND: i32 = -2147024893;

pub fn read(browser: Browser) -> PolicyReadResult {
    let root_key = policy_root_path(browser);
    let source = registry_location(root_key);
    read_policy(browser, root_key, source)
}

pub fn write(browser: Browser, policies: &PolicySet) -> PolicyWriteResult {
    let root_key = policy_root_path(browser);
    replace_registry_policy(root_key, policies)?;

    Ok(PolicyWrite {
        target: registry_location(root_key),
        policy_count: policies.len(),
    })
}

pub fn export(browser: Browser, policies: &PolicySet, path: &Path) -> PolicyWriteResult {
    let contents = registry_file_contents(browser, policies)?;
    let contents = registry_file_bytes(&contents);
    write_file_atomically(path, &contents)?;

    Ok(PolicyWrite {
        target: PolicyLocation::File(path.to_path_buf()),
        policy_count: policies.len(),
    })
}

pub fn export_file_extension() -> &'static str {
    "reg"
}

pub fn uninstall(browser: Browser) -> PolicyWriteResult {
    let root_key = policy_root_path(browser);
    remove_registry_policy(root_key)?;

    Ok(PolicyWrite {
        target: registry_location(root_key),
        policy_count: 0,
    })
}

pub fn managed_location(browser: Browser) -> PolicyLocation {
    registry_location(policy_root_path(browser))
}

fn read_policy(browser: Browser, root_key: &str, source: PolicyLocation) -> PolicyReadResult {
    let policies = match query_registry_policy(root_key) {
        Ok(Some(policies)) => policies,
        Ok(None) => return Ok(None),
        Err(error) => return Err(error),
    };

    if policies.is_empty() {
        Ok(None)
    } else {
        Ok(Some(BrowserPolicy {
            browser,
            source,
            policies,
        }))
    }
}

fn replace_registry_policy(root_key: &str, policies: &PolicySet) -> Result<(), PolicyWriteError> {
    validate_registry_map(policies, "")?;
    remove_registry_policy(root_key)?;

    if policies.is_empty() {
        return Ok(());
    }

    let key = LOCAL_MACHINE.create(root_key).map_err(|error| {
        PolicyWriteError::registry(
            "create Windows registry policy key",
            error.code().0,
            error.to_string(),
        )
    })?;

    write_registry_map(&key, policies, "")
}

fn remove_registry_policy(root_key: &str) -> Result<(), PolicyWriteError> {
    match LOCAL_MACHINE.remove_tree(root_key) {
        Ok(()) => {}
        Err(error) if is_missing_key(error.code().0) => {}
        Err(error) => {
            return Err(PolicyWriteError::registry(
                "remove Windows registry policy key",
                error.code().0,
                error.to_string(),
            ));
        }
    }

    Ok(())
}

fn validate_registry_map(
    policies: &BTreeMap<String, PolicyValue>,
    parent_path: &str,
) -> Result<(), PolicyWriteError> {
    for (name, value) in policies {
        let policy = policy_name(parent_path, name);
        validate_registry_value(&policy, value)?;
    }

    Ok(())
}

fn validate_registry_value(policy: &str, value: &PolicyValue) -> Result<(), PolicyWriteError> {
    match value {
        PolicyValue::Bool(_) | PolicyValue::String(_) => Ok(()),
        PolicyValue::Integer(value) => {
            u32::try_from(*value).map_err(|_| PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "Windows registry policy integers must fit in a DWORD",
            })?;
            Ok(())
        }
        PolicyValue::List(values) => {
            for value in values {
                validate_registry_list_item(policy, value)?;
            }

            Ok(())
        }
        PolicyValue::Object(values) => validate_registry_map(values, policy),
        PolicyValue::Null => Err(PolicyWriteError::UnsupportedValue {
            policy: policy.to_owned(),
            reason: "Windows registry policies do not support null values",
        }),
    }
}

fn validate_registry_list_item(policy: &str, value: &PolicyValue) -> Result<(), PolicyWriteError> {
    match value {
        PolicyValue::Bool(_) | PolicyValue::Integer(_) | PolicyValue::String(_) => Ok(()),
        PolicyValue::List(_) | PolicyValue::Object(_) | PolicyValue::Null => {
            Err(PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "registry list policies only support scalar values",
            })
        }
    }
}

fn write_registry_map(
    key: &Key,
    policies: &BTreeMap<String, PolicyValue>,
    parent_path: &str,
) -> Result<(), PolicyWriteError> {
    for (name, value) in policies {
        let policy = policy_name(parent_path, name);
        write_registry_value(key, name, &policy, value)?;
    }

    Ok(())
}

fn write_registry_value(
    key: &Key,
    name: &str,
    policy: &str,
    value: &PolicyValue,
) -> Result<(), PolicyWriteError> {
    match value {
        PolicyValue::Bool(value) => set_registry_u32(key, name, u32::from(*value)),
        PolicyValue::Integer(value) => {
            let value = u32::try_from(*value).map_err(|_| PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "Windows registry policy integers must fit in a DWORD",
            })?;
            set_registry_u32(key, name, value)
        }
        PolicyValue::String(value) => key.set_string(name, value).map_err(|error| {
            PolicyWriteError::registry(
                "write Windows registry value",
                error.code().0,
                error.to_string(),
            )
        }),
        PolicyValue::List(values) => {
            let child = create_registry_child(key, name)?;
            write_registry_list(&child, policy, values)
        }
        PolicyValue::Object(values) => {
            let child = create_registry_child(key, name)?;
            write_registry_map(&child, values, policy)
        }
        PolicyValue::Null => Err(PolicyWriteError::UnsupportedValue {
            policy: policy.to_owned(),
            reason: "Windows registry policies do not support null values",
        }),
    }
}

fn create_registry_child(parent: &Key, name: &str) -> Result<Key, PolicyWriteError> {
    parent.create(name).map_err(|error| {
        PolicyWriteError::registry(
            "create Windows registry policy subkey",
            error.code().0,
            error.to_string(),
        )
    })
}

fn write_registry_list(
    key: &Key,
    policy: &str,
    values: &[PolicyValue],
) -> Result<(), PolicyWriteError> {
    for (index, value) in values.iter().enumerate() {
        let name = (index + 1).to_string();
        let value = registry_list_item(policy, value)?;
        key.set_string(&name, value).map_err(|error| {
            PolicyWriteError::registry(
                "write Windows registry list value",
                error.code().0,
                error.to_string(),
            )
        })?;
    }

    Ok(())
}

fn set_registry_u32(key: &Key, name: &str, value: u32) -> Result<(), PolicyWriteError> {
    key.set_u32(name, value).map_err(|error| {
        PolicyWriteError::registry(
            "write Windows registry value",
            error.code().0,
            error.to_string(),
        )
    })
}

fn registry_file_contents(
    browser: Browser,
    policies: &PolicySet,
) -> Result<String, PolicyWriteError> {
    let root_key = registry_file_root_path(browser);
    let mut lines = vec![
        "Windows Registry Editor Version 5.00".to_owned(),
        String::new(),
        format!("[-{root_key}]"),
    ];

    if !policies.is_empty() {
        lines.push(String::new());
        append_registry_file_key(&mut lines, &root_key, policies, "")?;
    }

    lines.push(String::new());
    Ok(lines.join("\r\n"))
}

fn registry_file_bytes(contents: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(2 + contents.len() * 2);
    bytes.extend_from_slice(&[0xff, 0xfe]);
    for unit in contents.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    bytes
}

fn append_registry_file_key(
    lines: &mut Vec<String>,
    key_path: &str,
    policies: &BTreeMap<String, PolicyValue>,
    parent_path: &str,
) -> Result<(), PolicyWriteError> {
    lines.push(format!("[{key_path}]"));

    for (name, value) in policies {
        let policy = policy_name(parent_path, name);

        match value {
            PolicyValue::Bool(_) | PolicyValue::Integer(_) | PolicyValue::String(_) => {
                lines.push(format!(
                    "\"{}\"={}",
                    escape_registry_string(name),
                    registry_file_scalar_value(&policy, value)?
                ));
            }
            PolicyValue::List(values) => {
                lines.push(String::new());
                append_registry_file_list(lines, &format!("{key_path}\\{name}"), &policy, values)?;
            }
            PolicyValue::Object(values) => {
                lines.push(String::new());
                append_registry_file_key(lines, &format!("{key_path}\\{name}"), values, &policy)?;
            }
            PolicyValue::Null => {
                return Err(PolicyWriteError::UnsupportedValue {
                    policy,
                    reason: "Windows registry policies do not support null values",
                });
            }
        }
    }

    Ok(())
}

fn append_registry_file_list(
    lines: &mut Vec<String>,
    key_path: &str,
    policy: &str,
    values: &[PolicyValue],
) -> Result<(), PolicyWriteError> {
    lines.push(format!("[{key_path}]"));

    for (index, value) in values.iter().enumerate() {
        lines.push(format!(
            "\"{}\"=\"{}\"",
            index + 1,
            escape_registry_string(&registry_list_item(policy, value)?)
        ));
    }

    Ok(())
}

fn registry_file_scalar_value(
    policy: &str,
    value: &PolicyValue,
) -> Result<String, PolicyWriteError> {
    match value {
        PolicyValue::Bool(value) => Ok(format!("dword:{:08x}", u32::from(*value))),
        PolicyValue::Integer(value) => {
            let value = u32::try_from(*value).map_err(|_| PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "Windows registry policy integers must fit in a DWORD",
            })?;

            Ok(format!("dword:{value:08x}"))
        }
        PolicyValue::String(value) => Ok(format!("\"{}\"", escape_registry_string(value))),
        PolicyValue::List(_) | PolicyValue::Object(_) | PolicyValue::Null => {
            Err(PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "value cannot be written as a registry scalar",
            })
        }
    }
}

fn registry_list_item(policy: &str, value: &PolicyValue) -> Result<String, PolicyWriteError> {
    match value {
        PolicyValue::Bool(value) => Ok(value.to_string()),
        PolicyValue::Integer(value) => Ok(value.to_string()),
        PolicyValue::String(value) => Ok(value.clone()),
        PolicyValue::List(_) | PolicyValue::Object(_) | PolicyValue::Null => {
            Err(PolicyWriteError::UnsupportedValue {
                policy: policy.to_owned(),
                reason: "registry list policies only support scalar values",
            })
        }
    }
}

fn registry_file_root_path(browser: Browser) -> String {
    format!(r"HKEY_LOCAL_MACHINE\{}", policy_root_path(browser))
}

fn policy_name(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}\\{name}")
    }
}

fn escape_registry_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '\\' => escaped.push_str(r"\\"),
            '"' => escaped.push_str(r#"\""#),
            _ => escaped.push(character),
        }
    }

    escaped
}

fn policy_root_path(browser: Browser) -> &'static str {
    match browser {
        Browser::Brave => r"SOFTWARE\Policies\BraveSoftware\Brave",
        Browser::Chrome => r"SOFTWARE\Policies\Google\Chrome",
        Browser::Edge => r"SOFTWARE\Policies\Microsoft\Edge",
    }
}

fn registry_location(path: &str) -> PolicyLocation {
    PolicyLocation::RegistryKey(format!(r"HKLM\{path}"))
}

fn query_registry_policy(path: &str) -> Result<Option<PolicySet>, PolicyReadError> {
    let key = match LOCAL_MACHINE.open(path) {
        Ok(key) => key,
        Err(error) if is_missing_key(error.code().0) => return Ok(None),
        Err(error) => {
            return Err(PolicyReadError::registry(
                "open Windows registry key",
                error.code().0,
                error.to_string(),
            ));
        }
    };

    registry_map(&key).map(Some)
}

fn registry_map(key: &Key) -> Result<PolicySet, PolicyReadError> {
    let mut policies = registry_values(key)?;

    for name in registry_subkey_names(key)? {
        let child = open_registry_subkey(key, &name)?;
        policies.insert(name, registry_subkey_value(&child)?);
    }

    Ok(policies)
}

fn registry_subkey_value(key: &Key) -> Result<PolicyValue, PolicyReadError> {
    let values = registry_values(key)?;
    let subkeys = registry_subkey_names(key)?;

    if subkeys.is_empty() && is_numbered_list(&values) {
        return Ok(PolicyValue::List(registry_numbered_list(values)));
    }

    let mut object = values;

    for name in subkeys {
        let child = open_registry_subkey(key, &name)?;
        object.insert(name, registry_subkey_value(&child)?);
    }

    Ok(PolicyValue::Object(object))
}

fn registry_subkey_names(key: &Key) -> Result<Vec<String>, PolicyReadError> {
    key.keys()
        .map_err(|error| {
            PolicyReadError::registry(
                "enumerate Windows registry subkeys",
                error.code().0,
                error.to_string(),
            )
        })
        .map(|keys| keys.collect())
}

fn open_registry_subkey(parent: &Key, name: &str) -> Result<Key, PolicyReadError> {
    parent.open(name).map_err(|error| {
        PolicyReadError::registry(
            "open Windows registry subkey",
            error.code().0,
            error.to_string(),
        )
    })
}

fn registry_values(key: &Key) -> Result<PolicySet, PolicyReadError> {
    let values = key.values().map_err(|error| {
        PolicyReadError::registry(
            "enumerate Windows registry values",
            error.code().0,
            error.to_string(),
        )
    })?;

    Ok(values
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name, registry_value(value)))
        .collect())
}

fn registry_value(value: RegistryValue) -> PolicyValue {
    match value.ty() {
        Type::U32 | Type::U64 => registry_integer(&value)
            .map(PolicyValue::Integer)
            .unwrap_or_else(|| registry_bytes_value(&value)),
        Type::String | Type::ExpandString => PolicyValue::String(registry_string(&value)),
        Type::MultiString => PolicyValue::List(
            value
                .as_wide()
                .split(|character| *character == 0)
                .filter(|characters| !characters.is_empty())
                .map(|characters| PolicyValue::String(String::from_utf16_lossy(characters)))
                .collect(),
        ),
        Type::Bytes | Type::Other(_) => registry_bytes_value(&value),
    }
}

fn registry_integer(value: &RegistryValue) -> Option<i64> {
    match value.ty() {
        Type::U32 => value
            .as_ref()
            .try_into()
            .ok()
            .map(u32::from_le_bytes)
            .map(i64::from),
        Type::U64 => value
            .as_ref()
            .try_into()
            .ok()
            .map(u64::from_le_bytes)
            .and_then(|value| i64::try_from(value).ok()),
        Type::String | Type::ExpandString | Type::MultiString | Type::Bytes | Type::Other(_) => {
            None
        }
    }
}

fn registry_string(value: &RegistryValue) -> String {
    String::from_utf16_lossy(trim_wide(value.as_wide()))
}

fn trim_wide(mut characters: &[u16]) -> &[u16] {
    while characters.last() == Some(&0) {
        characters = &characters[..characters.len() - 1];
    }

    characters
}

fn registry_bytes_value(value: &RegistryValue) -> PolicyValue {
    PolicyValue::String(format!("{} bytes", value.len()))
}

fn is_numbered_list(values: &PolicySet) -> bool {
    values.keys().all(|name| registry_order(name).is_some())
}

fn registry_numbered_list(values: PolicySet) -> Vec<PolicyValue> {
    let mut values = values
        .into_iter()
        .filter_map(|(name, value)| registry_order(&name).map(|order| (order, name, value)))
        .collect::<Vec<_>>();

    values.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    values.into_iter().map(|(_, _, value)| value).collect()
}

fn registry_order(name: &str) -> Option<u32> {
    name.parse().ok()
}

fn is_missing_key(code: i32) -> bool {
    matches!(
        code,
        HRESULT_FROM_WIN32_FILE_NOT_FOUND | HRESULT_FROM_WIN32_PATH_NOT_FOUND
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_numbered_list_sorts_by_numeric_value_name() {
        let mut values = PolicySet::new();
        values.insert("10".to_owned(), string("ten"));
        values.insert("2".to_owned(), string("two"));
        values.insert("1".to_owned(), string("one"));

        assert_eq!(
            registry_numbered_list(values),
            vec![string("one"), string("two"), string("ten")]
        );
    }

    #[test]
    fn registry_validation_rejects_invalid_values_before_write() {
        let mut policies = PolicySet::new();
        policies.insert("HomepageIsNewTabPage".to_owned(), PolicyValue::Integer(-1));

        let result = validate_registry_map(&policies, "");

        assert!(matches!(
            result,
            Err(PolicyWriteError::UnsupportedValue { policy, .. })
                if policy == "HomepageIsNewTabPage"
        ));
    }

    #[test]
    fn registry_file_export_writes_numbered_list_subkeys() -> Result<(), PolicyWriteError> {
        let mut policies = PolicySet::new();
        policies.insert(
            "ExtensionInstallForcelist".to_owned(),
            PolicyValue::List(vec![string("abc;https://example.com/update.xml")]),
        );

        let contents = registry_file_contents(Browser::Chrome, &policies)?;

        assert!(contents.contains(
            "[HKEY_LOCAL_MACHINE\\SOFTWARE\\Policies\\Google\\Chrome\\ExtensionInstallForcelist]"
        ));
        assert!(contents.contains("\"1\"=\"abc;https://example.com/update.xml\""));

        Ok(())
    }

    fn string(value: &str) -> PolicyValue {
        PolicyValue::String(value.to_owned())
    }
}
