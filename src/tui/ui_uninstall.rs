use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::styles;
use super::ui_dialog::{self, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogState};

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 70,
    min_width: 44,
    max_width: 72,
    height_percent: 30,
    min_height: UNINSTALL_HEIGHT,
    max_height: UNINSTALL_HEIGHT,
    border_style: styles::RED,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
#[cfg(target_os = "macos")]
const UNINSTALL_HEIGHT: u16 = 14;
#[cfg(not(target_os = "macos"))]
const UNINSTALL_HEIGHT: u16 = 13;
const ERROR: Style = styles::RED;
const IMPORTANT: Style = styles::YELLOW.add_modifier(Modifier::BOLD);
#[cfg(not(target_os = "macos"))]
const UNINSTALL: ButtonSpec = ("u", "Uninstall");
#[cfg(target_os = "macos")]
const UNINSTALL: ButtonSpec = ("o", "Open Settings");
#[cfg(not(target_os = "macos"))]
const CANCEL: ButtonSpec = ("esc", "Cancel");
#[cfg(target_os = "macos")]
const CANCEL: ButtonSpec = ("esc", "Cancel");
const CLOSE: ButtonSpec = ("esc", "Close");

const DIALOG_TITLE: &str = "Uninstall Policies";

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    let has_policy = app.active_browser_state().managed_policy_exists();

    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(dialog, app, has_policy),
            buttons: Some(buttons_line(has_policy, dialog.focused_button)),
        },
    );
}

fn dialog_text(dialog: &DialogState, app: &App, has_policy: bool) -> Text<'static> {
    if !has_policy {
        return Text::from_iter(no_policy_text());
    }

    Text::from_iter(
        confirm_uninstall_text(app, dialog.focused_button)
            .into_iter()
            .chain(error_lines(dialog)),
    )
}

fn error_lines(dialog: &DialogState) -> impl Iterator<Item = Line<'static>> {
    dialog
        .status
        .clone()
        .into_iter()
        .flat_map(|status| [Line::default(), Line::styled(status, ERROR)])
}

fn no_policy_text() -> [Line<'static>; 5] {
    [
        ui_dialog::title_line(DIALOG_TITLE),
        Line::default(),
        Line::styled("Nothing to uninstall", ui_dialog::BODY),
        Line::default(),
        Line::default(),
    ]
}

#[cfg(not(target_os = "macos"))]
fn confirm_uninstall_text(app: &App, _focused_button: usize) -> [Line<'static>; 5] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line(DIALOG_TITLE),
        Line::default(),
        Line::from_iter([
            Span::styled("This will ", ui_dialog::BODY),
            Span::styled("permanently delete", IMPORTANT),
            Span::styled(
                format!(" all {} system policies.", browser),
                ui_dialog::BODY,
            ),
        ]),
        Line::default(),
        Line::styled(
            "Any unsaved or pending edits will be discarded.",
            ui_dialog::BODY,
        ),
    ]
}

fn buttons_line(has_policy: bool, focused_button: usize) -> Line<'static> {
    ui_dialog::confirm_or_secondary_buttons_line(
        has_policy,
        [UNINSTALL, secondary_button(has_policy)],
        focused_button,
    )
}

fn secondary_button(has_policy: bool) -> ButtonSpec {
    if has_policy { CANCEL } else { CLOSE }
}

#[cfg(target_os = "macos")]
fn confirm_uninstall_text(app: &App, _focused_button: usize) -> [Line<'static>; 6] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line(DIALOG_TITLE),
        Line::default(),
        Line::from_iter([
            Span::styled("macOS requires policy profiles to be ", ui_dialog::BODY),
            Span::styled("removed manually", IMPORTANT),
            Span::styled(".", ui_dialog::BODY),
        ]),
        Line::default(),
        Line::from_iter([
            Span::styled("Select ", ui_dialog::BODY),
            Span::styled("Open Settings", IMPORTANT),
            Span::styled(" to launch macOS System Settings,", ui_dialog::BODY),
        ]),
        Line::from_iter([
            Span::styled("then delete the ", ui_dialog::BODY),
            Span::styled(format!("'{} Policy'", browser), IMPORTANT),
            Span::styled(" profile.", ui_dialog::BODY),
        ]),
    ]
}
