use std::collections::BTreeMap;
use std::io::{self, Read};

use flate2::read::GzDecoder;
use indexmap::IndexMap;
use serde::Deserialize;
use thiserror::Error;

use crate::chromium::Browser;
use crate::chromium::policy::{PolicySet, PolicyValue};

const BALANCED_PRESET: &str = "balanced";
pub(crate) const EXTENSION_INSTALL_FORCELIST: &str = "ExtensionInstallForcelist";
const MANIFEST_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/manifest.yaml.gz"));

#[derive(Debug)]
pub(crate) struct Manifest {
    policy_groups: Vec<PolicyGroup>,
    extensions: IndexMap<String, Extension>,
    presets: IndexMap<Browser, IndexMap<String, Preset>>,
}

#[derive(Debug)]
pub(crate) struct PolicyGroup {
    pub id: String,
    pub name: String,
    pub targets: Vec<Browser>,
    pub settings: Vec<PolicySetting>,
}

#[derive(Debug)]
pub(crate) struct PolicySetting {
    pub key: String,
    pub value: PolicyValue,
}

#[derive(Debug)]
struct Extension {
    name: String,
    id: String,
    targets: Vec<Browser>,
    settings: Vec<PolicySetting>,
}

#[derive(Debug)]
struct Preset {
    policy_groups: Vec<String>,
    extensions: Vec<String>,
}

#[derive(Debug, Error)]
pub(crate) enum ManifestError {
    #[error("read embedded manifest: {0}")]
    Io(#[from] io::Error),
    #[error("parse embedded manifest: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    policy_groups: IndexMap<String, RawPolicyGroup>,
    #[serde(default)]
    extensions: IndexMap<String, RawExtension>,
    #[serde(default)]
    presets: IndexMap<String, IndexMap<String, RawPreset>>,
}

#[derive(Debug, Deserialize)]
struct RawPolicyGroup {
    name: Option<String>,
    label: Option<String>,
    targets: Vec<String>,
    settings: IndexMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
struct RawExtension {
    name: Option<String>,
    id: String,
    targets: Vec<String>,
    #[serde(default)]
    settings: IndexMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPreset {
    #[serde(default)]
    policy_groups: Vec<String>,
    #[serde(default)]
    extensions: Vec<String>,
}

impl Manifest {
    pub(crate) fn load() -> Result<Self, ManifestError> {
        let mut yaml = String::new();
        GzDecoder::new(MANIFEST_GZ).read_to_string(&mut yaml)?;
        Self::from_yaml(&yaml)
    }

    pub(crate) fn policy_groups(&self, browser: Browser) -> impl Iterator<Item = &PolicyGroup> {
        self.policy_groups
            .iter()
            .filter(move |group| group.targets.contains(&browser))
    }

    pub(crate) fn policy_group(&self, id: &str) -> Option<&PolicyGroup> {
        self.policy_groups.iter().find(|group| group.id == id)
    }

    pub(crate) fn balanced_preset(&self, browser: Browser) -> PolicySet {
        let Some(preset) = self
            .presets
            .get(&browser)
            .and_then(|presets| presets.get(BALANCED_PRESET))
        else {
            return PolicySet::new();
        };

        let mut policies = PolicySet::new();
        for group_id in &preset.policy_groups {
            let Some(group) = self.policy_group(group_id) else {
                continue;
            };
            if !group.targets.contains(&browser) {
                continue;
            }

            for setting in &group.settings {
                policies.insert(setting.key.clone(), setting.value.clone());
            }
        }

        let mut extension_ids = Vec::new();
        for extension_id in &preset.extensions {
            let Some(extension) = self.extensions.get(extension_id) else {
                continue;
            };
            if !extension.targets.contains(&browser) {
                continue;
            }

            extension_ids.push(PolicyValue::String(extension.id.clone()));
            for setting in &extension.settings {
                policies.insert(setting.key.clone(), setting.value.clone());
            }
        }

        if !extension_ids.is_empty() {
            policies.insert(
                EXTENSION_INSTALL_FORCELIST.to_owned(),
                PolicyValue::List(extension_ids),
            );
        }

        policies
    }

    pub(crate) fn extension_name(&self, browser: Browser, extension_id: &str) -> Option<&str> {
        let extension_id = extension_id
            .split_once(';')
            .map_or(extension_id, |(id, _)| id);

        self.extensions
            .values()
            .find(|extension| extension.id == extension_id && extension.targets.contains(&browser))
            .or_else(|| {
                self.extensions
                    .values()
                    .find(|extension| extension.id == extension_id)
            })
            .map(|extension| extension.name.as_str())
    }

    pub(crate) fn has_policy_key(&self, browser: Browser, key: &str) -> bool {
        self.policy_groups(browser)
            .any(|group| group.settings.iter().any(|setting| setting.key == key))
    }

    fn from_yaml(yaml: &str) -> Result<Self, ManifestError> {
        let raw = serde_yaml::from_str::<RawManifest>(yaml)?;
        let policy_groups = raw
            .policy_groups
            .into_iter()
            .map(|(id, group)| group.try_into_group(id))
            .collect::<Result<Vec<_>, _>>()?;
        let extensions = raw
            .extensions
            .into_iter()
            .map(|(id, extension)| Ok((id, extension.try_into_extension()?)))
            .collect::<Result<IndexMap<_, _>, ManifestError>>()?;
        let presets = raw
            .presets
            .into_iter()
            .map(|(browser, presets)| {
                Ok((
                    parse_browser(&browser)?,
                    presets
                        .into_iter()
                        .map(|(name, preset)| {
                            (
                                name,
                                Preset {
                                    policy_groups: preset.policy_groups,
                                    extensions: preset.extensions,
                                },
                            )
                        })
                        .collect(),
                ))
            })
            .collect::<Result<IndexMap<_, _>, ManifestError>>()?;

        Ok(Self {
            policy_groups,
            extensions,
            presets,
        })
    }
}

impl RawPolicyGroup {
    fn try_into_group(self, id: String) -> Result<PolicyGroup, ManifestError> {
        let name = self.label.or(self.name).unwrap_or_else(|| id.clone());
        let targets = parse_targets(&self.targets)?;
        let settings = parse_settings(self.settings)?;

        Ok(PolicyGroup {
            id,
            name,
            targets,
            settings,
        })
    }
}

impl RawExtension {
    fn try_into_extension(self) -> Result<Extension, ManifestError> {
        let name = self.name.unwrap_or_else(|| self.id.clone());

        Ok(Extension {
            name,
            id: self.id,
            targets: parse_targets(&self.targets)?,
            settings: parse_settings(self.settings)?,
        })
    }
}

fn parse_targets(targets: &[String]) -> Result<Vec<Browser>, ManifestError> {
    targets.iter().map(|target| parse_browser(target)).collect()
}

fn parse_browser(value: &str) -> Result<Browser, ManifestError> {
    match value {
        "brave" => Ok(Browser::Brave),
        "chrome" => Ok(Browser::Chrome),
        "edge" => Ok(Browser::Edge),
        _ => Err(ManifestError::Invalid(format!(
            "unknown browser target '{value}'"
        ))),
    }
}

fn parse_settings(
    settings: IndexMap<String, serde_yaml::Value>,
) -> Result<Vec<PolicySetting>, ManifestError> {
    settings
        .into_iter()
        .map(|(key, value)| {
            Ok(PolicySetting {
                key,
                value: parse_policy_value(value)?,
            })
        })
        .collect()
}

fn parse_policy_value(value: serde_yaml::Value) -> Result<PolicyValue, ManifestError> {
    match value {
        serde_yaml::Value::Bool(value) => Ok(PolicyValue::Bool(value)),
        serde_yaml::Value::Number(value) => value
            .as_i64()
            .map(PolicyValue::Integer)
            .ok_or_else(|| ManifestError::Invalid("policy numbers must be integers".to_owned())),
        serde_yaml::Value::String(value) => Ok(PolicyValue::String(value)),
        serde_yaml::Value::Sequence(values) => values
            .into_iter()
            .map(parse_policy_value)
            .collect::<Result<Vec<_>, _>>()
            .map(PolicyValue::List),
        serde_yaml::Value::Mapping(values) => parse_policy_object(values),
        serde_yaml::Value::Null => Ok(PolicyValue::Null),
        serde_yaml::Value::Tagged(value) => parse_policy_value(value.value),
    }
}

fn parse_policy_object(values: serde_yaml::Mapping) -> Result<PolicyValue, ManifestError> {
    values
        .into_iter()
        .map(|(key, value)| {
            let Some(key) = key.as_str() else {
                return Err(ManifestError::Invalid(
                    "policy object keys must be strings".to_owned(),
                ));
            };

            Ok((key.to_owned(), parse_policy_value(value)?))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map(PolicyValue::Object)
}
