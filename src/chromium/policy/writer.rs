use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tempfile::{Builder as TempFileBuilder, NamedTempFile};

use super::errors::PolicyWriteError;
#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(target_os = "macos")]
use super::macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use super::unsupported as platform;
#[cfg(target_os = "windows")]
use super::windows as platform;
use super::{PolicyLocation, PolicySet};
use crate::chromium::Browser;

pub type PolicyWriteResult = Result<PolicyWrite, PolicyWriteError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyWrite {
    pub target: PolicyLocation,
    pub policy_count: usize,
}

pub fn write(browser: Browser, policies: &PolicySet) -> PolicyWriteResult {
    platform::write(browser, policies)
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall(browser: Browser) -> PolicyWriteResult {
    platform::uninstall(browser)
}

pub fn managed_location(browser: Browser) -> PolicyLocation {
    platform::managed_location(browser)
}

pub(super) fn write_file_atomically(path: &Path, contents: &[u8]) -> Result<(), PolicyWriteError> {
    let (parent, file_name) = split_target_path(path)?;
    create_parent_dir(parent)?;

    let mut temp_file = create_temp_file(parent, file_name)?;
    write_and_sync_file(&mut temp_file, contents)?;
    let persisted_file = persist_temp_file(temp_file, path)?;
    drop(persisted_file);

    sync_directory(parent).map_err(|source| PolicyWriteError::Io {
        action: "sync policy directory",
        source,
    })
}

fn split_target_path(path: &Path) -> Result<(&Path, &OsStr), PolicyWriteError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or(PolicyWriteError::InvalidPath {
        reason: "path must include a file name",
    })?;

    Ok((parent, file_name))
}

fn create_parent_dir(parent: &Path) -> Result<(), PolicyWriteError> {
    fs::create_dir_all(parent).map_err(|source| PolicyWriteError::Io {
        action: "create policy directory",
        source,
    })
}

fn create_temp_file(parent: &Path, file_name: &OsStr) -> Result<NamedTempFile, PolicyWriteError> {
    let prefix = temporary_file_prefix(file_name);

    TempFileBuilder::new()
        .prefix(&prefix)
        .tempfile_in(parent)
        .map_err(|source| PolicyWriteError::Io {
            action: "create temporary policy file",
            source,
        })
}

fn temporary_file_prefix(file_name: &OsStr) -> OsString {
    let mut prefix = OsString::from(".");
    prefix.push(file_name);
    prefix.push(".");
    prefix
}

fn write_and_sync_file(file: &mut NamedTempFile, contents: &[u8]) -> Result<(), PolicyWriteError> {
    set_temp_file_permissions(file)?;

    file.write_all(contents)
        .and_then(|()| file.as_file_mut().sync_all())
        .map_err(|source| PolicyWriteError::Io {
            action: "write temporary policy file",
            source,
        })
}

#[cfg(target_os = "linux")]
fn set_temp_file_permissions(file: &mut NamedTempFile) -> Result<(), PolicyWriteError> {
    file.as_file_mut()
        .set_permissions(fs::Permissions::from_mode(0o644))
        .map_err(|source| PolicyWriteError::Io {
            action: "set temporary policy file permissions",
            source,
        })
}

#[cfg(not(target_os = "linux"))]
fn set_temp_file_permissions(_file: &mut NamedTempFile) -> Result<(), PolicyWriteError> {
    Ok(())
}

fn persist_temp_file(file: NamedTempFile, path: &Path) -> Result<File, PolicyWriteError> {
    file.persist(path).map_err(|error| PolicyWriteError::Io {
        action: "replace policy file",
        source: error.error,
    })
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_parent_and_replaces_file() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("nested").join("policy.json");

        write_file_atomically(&path, b"{\"first\":true}\n")?;
        write_file_atomically(&path, b"{\"second\":true}\n")?;

        let contents = fs::read_to_string(&path)?;
        assert_eq!(contents, "{\"second\":true}\n");

        Ok(())
    }

    #[test]
    fn atomic_write_rejects_path_without_file_name() {
        assert!(matches!(
            write_file_atomically(Path::new("/"), b"policy"),
            Err(PolicyWriteError::InvalidPath { .. })
        ));
    }
}
