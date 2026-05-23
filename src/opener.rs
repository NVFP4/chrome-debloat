use std::io;
use std::path::Path;
#[cfg(any(unix, target_os = "windows"))]
use std::process::Command;

#[cfg(any(unix, target_os = "windows"))]
pub(crate) fn locate_file(path: &Path) -> io::Result<()> {
    let status = locate_command(path).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("failed to locate file"))
    }
}

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
pub(crate) fn locate_file(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "locating files is not supported on this platform",
    ))
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn open_url(_url: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening URLs is not supported on this platform",
    ))
}

#[cfg(target_os = "macos")]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-R").arg(path);
    command
}

#[cfg(target_os = "macos")]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg(url);
    command
}

#[cfg(target_os = "windows")]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(format!("/select,{}", path.display()));
    command
}

#[cfg(target_os = "windows")]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("rundll32");
    command.args(["url.dll,FileProtocolHandler", url]);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_command(url: &str) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    command
}
