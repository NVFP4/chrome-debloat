use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyReadError {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[error("{action}: {source}")]
    Io {
        action: &'static str,
        #[source]
        source: io::Error,
    },
    #[cfg(target_os = "linux")]
    #[error("policy data is not valid JSON: {source}")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[error("invalid policy data: {reason}")]
    Invalid { reason: &'static str },
    #[cfg(target_os = "macos")]
    #[error("policy plist could not be read: {source}")]
    Plist {
        #[source]
        source: plist::Error,
    },
    #[cfg(target_os = "windows")]
    #[error("{action}: {message} (HRESULT 0x{hresult:08X})")]
    Registry {
        action: &'static str,
        hresult: u32,
        message: String,
    },
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    #[error("policy can only be read on Linux, macOS, and Windows")]
    UnsupportedPlatform,
}

impl PolicyReadError {
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

#[derive(Debug, Error)]
pub enum PolicyWriteError {
    #[error("{action}: {source}")]
    Io {
        action: &'static str,
        #[source]
        source: io::Error,
    },
    #[cfg(target_os = "linux")]
    #[error("policy data could not be serialized as JSON: {source}")]
    Json {
        #[source]
        source: serde_json::Error,
    },
    #[cfg(target_os = "macos")]
    #[error("policy profile could not be serialized: {source}")]
    Plist {
        #[source]
        source: plist::Error,
    },
    #[cfg(target_os = "windows")]
    #[error("{action}: {message} (HRESULT 0x{hresult:08X})")]
    Registry {
        action: &'static str,
        hresult: u32,
        message: String,
    },
    #[error("invalid policy write path: {reason}")]
    InvalidPath { reason: &'static str },
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[error("policy {policy} cannot be written: {reason}")]
    UnsupportedValue {
        policy: String,
        reason: &'static str,
    },
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    #[error("policy can only be written on Linux, macOS, and Windows")]
    UnsupportedPlatform,
}

impl PolicyWriteError {
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
