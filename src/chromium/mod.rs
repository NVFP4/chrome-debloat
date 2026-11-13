pub mod detection;
pub mod policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Browser {
    Brave,
    Chrome,
    Edge,
    Yandex,
    /// Any other Chromium browser, located by a user-supplied policy identifier.
    Chromium,
}

impl Browser {
    pub const COUNT: usize = 5;

    pub const fn all() -> [Self; Self::COUNT] {
        [
            Self::Brave,
            Self::Chrome,
            Self::Edge,
            Self::Yandex,
            Self::Chromium,
        ]
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Brave => "Brave",
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Yandex => "Yandex",
            Self::Chromium => "Any Chromium",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Yandex => "yandex",
            Self::Chromium => "chromium",
        }
    }
}
