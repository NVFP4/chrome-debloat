use std::io;
use std::path::Path;
use std::process::Command;

pub fn open_mobileconfig(path: &Path) -> io::Result<()> {
    let status = Command::new("/usr/bin/open").arg(path).status()?;

    if !status.success() {
        return Err(io::Error::other("failed to open configuration profile"));
    }

    open_profiles_settings()?;

    Ok(())
}

pub fn open_profiles_settings() -> io::Result<()> {
    let status = Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Profiles-Settings.extension")
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("failed to open Profiles settings"))
    }
}
