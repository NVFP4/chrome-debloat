use std::io;
#[cfg(any(unix, target_os = "windows"))]
use std::process::Command;

#[cfg(any(unix, target_os = "windows"))]
pub(crate) fn open_url(url: &str) -> io::Result<()> {
    let status = open_command(url).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("failed to open URL"))
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn open_url(_url: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening URLs is not supported on this platform",
    ))
}

#[cfg(target_os = "macos")]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg(url);
    command
}

#[cfg(target_os = "windows")]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", url]);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    command
}
