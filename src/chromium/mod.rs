pub mod detection;
pub mod policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Browser {
    Brave,
    Chrome,
    Edge,
    Yandex,
}

impl Browser {
    pub const COUNT: usize = 4;

    pub const fn all() -> [Self; Self::COUNT] {
        [Self::Brave, Self::Chrome, Self::Edge, Self::Yandex]
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Brave => "Brave",
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Yandex => "Yandex",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Yandex => "yandex",
        }
    }
}
