#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::io;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserDetectionError {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[error("{action} at {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[cfg(target_os = "windows")]
    #[error("{action}: {message} (HRESULT 0x{hresult:08X})")]
    Registry {
        action: &'static str,
        hresult: u32,
        message: String,
    },
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    #[error("browser detection is only supported on Linux, macOS, and Windows")]
    UnsupportedPlatform,
}

impl BrowserDetectionError {
    #[cfg(target_os = "windows")]
    pub(crate) fn registry(action: &'static str, hresult: i32, message: String) -> Self {
        let message = if message.is_empty() {
            "Windows registry error".to_owned()
        } else {
            message
        };

        Self::Registry {
            action,
            hresult: hresult as u32,
            message,
        }
    }
}
