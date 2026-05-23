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

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn locate_file(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "locating files is not supported on this platform",
    ))
}

#[cfg(target_os = "macos")]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-R").arg(path);
    command
}

#[cfg(target_os = "windows")]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("explorer");
    command.arg(format!("/select,{}", path.display()));
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn locate_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}
