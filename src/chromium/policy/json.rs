#[cfg(target_os = "linux")]
use std::path::Path;

use serde_json::Value as JsonValue;

use super::errors::PolicyReadError;
#[cfg(target_os = "linux")]
use super::errors::PolicyWriteError;
use super::{PolicySet, PolicyValue};

const ROOT_MUST_BE_OBJECT: &str = "root must be a dictionary/object";

#[cfg(target_os = "linux")]
pub(super) fn policy_set_from_file(path: &Path) -> Result<PolicySet, PolicyReadError> {
    let contents = std::fs::read(path).map_err(|source| PolicyReadError::Io {
        action: "read policy file",
        source,
    })?;

    policy_set_from_bytes(&contents)
}

pub(super) fn policy_set_from_bytes(bytes: &[u8]) -> Result<PolicySet, PolicyReadError> {
    let value = serde_json::from_slice(bytes).map_err(|source| PolicyReadError::Json { source })?;
    read_json(value)
}

fn read_json(value: JsonValue) -> Result<PolicySet, PolicyReadError> {
    match value {
        JsonValue::Object(values) => Ok(values
            .into_iter()
            .map(|(key, value)| (key, read_value(value)))
            .collect()),
        JsonValue::Null
        | JsonValue::Bool(_)
        | JsonValue::Number(_)
        | JsonValue::String(_)
        | JsonValue::Array(_) => Err(PolicyReadError::Invalid {
            reason: ROOT_MUST_BE_OBJECT,
        }),
    }
}

fn read_value(value: JsonValue) -> PolicyValue {
    match value {
        JsonValue::Bool(value) => PolicyValue::Bool(value),
        JsonValue::Number(value) => value
            .as_i64()
            .map(PolicyValue::Integer)
            .or_else(|| {
                value
                    .as_u64()
                    .and_then(|value| i64::try_from(value).ok())
                    .map(PolicyValue::Integer)
            })
            .unwrap_or_else(|| PolicyValue::String(value.to_string())),
        JsonValue::String(value) => PolicyValue::String(value),
        JsonValue::Array(values) => PolicyValue::List(values.into_iter().map(read_value).collect()),
        JsonValue::Object(values) => PolicyValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, read_value(value)))
                .collect(),
        ),
        JsonValue::Null => PolicyValue::Null,
    }
}

#[cfg(target_os = "linux")]
pub(super) fn policy_set_to_bytes(policies: &PolicySet) -> Result<Vec<u8>, PolicyWriteError> {
    let value = JsonValue::Object(
        policies
            .iter()
            .map(|(key, value)| (key.clone(), write_value(value)))
            .collect(),
    );
    let mut bytes =
        serde_json::to_vec_pretty(&value).map_err(|source| PolicyWriteError::Json { source })?;
    bytes.push(b'\n');

    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn write_value(value: &PolicyValue) -> JsonValue {
    match value {
        PolicyValue::Bool(value) => JsonValue::Bool(*value),
        PolicyValue::Integer(value) => JsonValue::from(*value),
        PolicyValue::String(value) => JsonValue::String(value.clone()),
        PolicyValue::List(values) => JsonValue::Array(values.iter().map(write_value).collect()),
        PolicyValue::Object(values) => JsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), write_value(value)))
                .collect(),
        ),
        PolicyValue::Null => JsonValue::Null,
    }
}
