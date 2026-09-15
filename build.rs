use std::io::{self, Write};
use std::path::PathBuf;
use std::{env, fs};

use flate2::Compression;
use flate2::write::GzEncoder;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=src/manifest.yaml");

    let manifest = fs::read("src/manifest.yaml")?;
    validate_manifest(&manifest)?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&manifest)?;
    let compressed = encoder.finish()?;

    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    fs::write(out_dir.join("manifest.yaml.gz"), compressed)
}

fn validate_manifest(contents: &[u8]) -> io::Result<()> {
    let manifest: serde_yaml::Value = serde_yaml::from_slice(contents).map_err(|source| {
        invalid_manifest(format!("could not parse src/manifest.yaml: {source}"))
    })?;
    let Some(presets) = manifest
        .get("presets")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Ok(());
    };
    let extensions = manifest
        .get("extensions")
        .and_then(serde_yaml::Value::as_mapping);

    for (browser, browser_presets) in presets {
        let Some(browser) = browser.as_str() else {
            return Err(invalid_manifest("preset browser names must be strings"));
        };
        let Some(browser_presets) = browser_presets.as_mapping() else {
            return Err(invalid_manifest(format!(
                "presets for browser '{browser}' must be a mapping"
            )));
        };

        for (preset_name, preset) in browser_presets {
            let Some(preset_name) = preset_name.as_str() else {
                return Err(invalid_manifest("preset names must be strings"));
            };
            let Some(preset) = preset.as_mapping() else {
                return Err(invalid_manifest(format!(
                    "preset '{preset_name}' for browser '{browser}' must be a mapping"
                )));
            };
            let Some(extension_values) = preset
                .get("extensions")
                .and_then(serde_yaml::Value::as_sequence)
            else {
                continue;
            };

            for extension_value in extension_values {
                let Some(extension_id) = extension_value.as_str() else {
                    return Err(invalid_manifest(format!(
                        "preset '{preset_name}' for browser '{browser}' must reference extensions by name"
                    )));
                };
                let Some(extension) = extensions.and_then(|extensions| {
                    extensions.get(serde_yaml::Value::String(extension_id.to_owned()))
                }) else {
                    return Err(invalid_manifest(format!(
                        "preset '{preset_name}' for browser '{browser}' references unknown extension '{extension_id}'"
                    )));
                };
                let targeted = extension
                    .get("targets")
                    .and_then(serde_yaml::Value::as_sequence)
                    .is_some_and(|targets| {
                        targets
                            .iter()
                            .any(|target| target.as_str() == Some(browser))
                    });
                if !targeted {
                    return Err(invalid_manifest(format!(
                        "preset '{preset_name}' for browser '{browser}' references extension '{extension_id}' that does not target that browser"
                    )));
                }
            }
        }
    }

    Ok(())
}

fn invalid_manifest(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
