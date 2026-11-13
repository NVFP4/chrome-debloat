use std::fmt;

use thiserror::Error;

#[cfg(target_os = "linux")]
use super::linux as platform;
#[cfg(target_os = "macos")]
use super::macos as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use super::unsupported as platform;
#[cfg(target_os = "windows")]
use super::windows as platform;
use crate::chromium::Browser;

/// The managed policy store a browser reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyTarget {
    Brave,
    Chrome,
    Edge,
    Yandex,
    Custom(CustomBrowserId),
}

/// A validated, platform-specific policy identifier for a Chromium browser the app doesn't know:
/// a registry key under `HKLM\SOFTWARE\Policies` on Windows, a bundle ID on macOS, and a
/// directory under `/etc` on Linux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomBrowserId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct InvalidCustomBrowserId(pub(super) &'static str);

impl PolicyTarget {
    /// `None` for [`Browser::Chromium`], whose store needs a [`CustomBrowserId`].
    pub const fn builtin(browser: Browser) -> Option<Self> {
        match browser {
            Browser::Brave => Some(Self::Brave),
            Browser::Chrome => Some(Self::Chrome),
            Browser::Edge => Some(Self::Edge),
            Browser::Yandex => Some(Self::Yandex),
            Browser::Chromium => None,
        }
    }

    pub const fn browser(&self) -> Browser {
        match self {
            Self::Brave => Browser::Brave,
            Self::Chrome => Browser::Chrome,
            Self::Edge => Browser::Edge,
            Self::Yandex => Browser::Yandex,
            Self::Custom(_) => Browser::Chromium,
        }
    }

    pub const fn custom_id(&self) -> Option<&CustomBrowserId> {
        match self {
            Self::Custom(id) => Some(id),
            Self::Brave | Self::Chrome | Self::Edge | Self::Yandex => None,
        }
    }
}

impl CustomBrowserId {
    pub fn parse(input: &str) -> Result<Self, InvalidCustomBrowserId> {
        let id = input.trim();
        if id.is_empty() {
            return Err(InvalidCustomBrowserId("enter an identifier"));
        }

        platform::validate_custom_id(id)?;
        Ok(Self(id.to_owned()))
    }

    /// What the identifier means on this platform, with an example.
    pub const fn hint() -> &'static str {
        platform::CUSTOM_ID_HINT
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CustomBrowserId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Splits `id` on `separator` and checks every segment is non-empty and made of `allowed` chars.
#[allow(dead_code)] // Each platform backend uses its own subset of the validators.
pub(super) fn validate_segments(
    id: &str,
    separator: char,
    allowed: impl Fn(char) -> bool,
) -> Result<(), InvalidCustomBrowserId> {
    for segment in id.split(separator) {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(InvalidCustomBrowserId(
                "identifier has an empty or relative part",
            ));
        }
        if !segment.chars().all(&allowed) {
            return Err(InvalidCustomBrowserId(
                "identifier contains unsupported characters",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_reject_empty_and_relative_parts() {
        let allowed = |character: char| character.is_ascii_alphanumeric() || character == '.';

        assert!(validate_segments("vendor/browser", '/', allowed).is_ok());
        assert!(validate_segments("vendor//browser", '/', allowed).is_err());
        assert!(validate_segments("vendor/../etc", '/', allowed).is_err());
        assert!(validate_segments("vendor/bro wser", '/', allowed).is_err());
    }
}
