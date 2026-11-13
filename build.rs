use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use flate2::Compression;
use flate2::write::GzEncoder;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=src/manifest.yaml");
    println!("cargo:rustc-env=APP_VERSION={}", app_version()?);

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

/// `X.Y.Z` for release builds, else `X.Y.(Z+1)-dev+<hash>`.
fn app_version() -> io::Result<String> {
    println!("cargo:rerun-if-env-changed=APP_RELEASE");
    let package_version = env!("CARGO_PKG_VERSION");
    if env::var_os("APP_RELEASE").is_some() {
        return Ok(package_version.to_owned());
    }

    watch_git_head();
    let Some(hash) = git(&["rev-parse", "--short=7", "HEAD"]) else {
        return Ok(package_version.to_owned());
    };

    let patch: u64 = env!("CARGO_PKG_VERSION_PATCH")
        .parse()
        .map_err(|_| io::Error::other("package patch version is not a number"))?;
    Ok(format!(
        "{}.{}.{}-dev+{hash}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        patch + 1
    ))
}

fn watch_git_head() {
    let head_ref = git(&["symbolic-ref", "-q", "HEAD"]);
    let watched = ["HEAD", "packed-refs"]
        .into_iter()
        .chain(head_ref.as_deref());

    let mut args = vec!["rev-parse"];
    for path in watched {
        args.extend(["--git-path", path]);
    }
    let Some(resolved) = git(&args) else {
        return;
    };
    // Cargo always reruns for missing paths.
    for path in resolved.lines().filter(|path| Path::new(path).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|stdout| stdout.trim_end().to_owned())
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
