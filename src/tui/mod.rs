pub mod action;
pub mod event;
pub mod term;
pub mod ui;

mod styles;
mod ui_apply;
mod ui_content;
mod ui_dialog;
mod ui_filter;
mod ui_footer;
mod ui_header;
mod ui_help;
mod ui_quit;
mod ui_revert;
#[cfg(target_os = "linux")]
mod ui_sudo;
mod ui_summary;
mod ui_text;
mod ui_uninstall;

pub use term::{init, install_panic_hook, run};
