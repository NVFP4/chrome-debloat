#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

mod app;
mod browser;
mod chromium;
mod diff;
mod editor;
mod history;
#[cfg(target_os = "macos")]
mod macos;
mod manifest;
mod opener;
mod policy_tree;
mod tui;
#[cfg(target_os = "macos")]
mod watcher;
#[cfg(target_os = "windows")]
mod windows;

use anyhow::Result;
use app::App;

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    if windows::relaunch_elevated_if_needed() {
        return Ok(());
    }

    tui::install_panic_hook();

    let mut app = App::new()?;
    let terminal = tui::init()?;

    tui::run(terminal, &mut app)
}
