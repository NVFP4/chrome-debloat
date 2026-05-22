#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Browser {
    Brave,
    Chrome,
    Edge,
}

impl Browser {
    pub const fn all() -> [Self; 3] {
        [Self::Brave, Self::Chrome, Self::Edge]
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Brave => "Brave",
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Chrome => "chrome",
            Self::Edge => "edge",
        }
    }
}
