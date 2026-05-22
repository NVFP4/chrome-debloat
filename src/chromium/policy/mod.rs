mod errors;
#[allow(clippy::module_inception)] // Domain policy types live in policy.rs inside this module.
mod policy;
mod reader;
#[allow(dead_code)] // Writer API is exposed before the TUI starts calling it.
mod writer;

#[cfg(target_os = "linux")]
mod json;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[allow(unused_imports)] // Public error surface for read/write callers.
pub use errors::{PolicyReadError, PolicyWriteError};
pub use policy::{BrowserPolicy, PolicyLocation, PolicySet, PolicyValue};
pub use reader::{PolicyReadResult, read};
#[cfg(not(target_os = "macos"))]
pub use writer::uninstall;
pub use writer::{PolicyWrite, export, export_file_name, managed_location, write};
