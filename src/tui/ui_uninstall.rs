use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::styles;
use super::ui_dialog::{self, ButtonHit, ButtonSpec, DialogLayout, DialogRender};
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
const UNINSTALL_HEIGHT: u16 = 11;
#[cfg(not(target_os = "macos"))]
const UNINSTALL_HEIGHT: u16 = 10;
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

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };

    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(dialog, app),
            buttons: Some(ui_dialog::buttons_line(ui_dialog::confirm_buttons(
                UNINSTALL,
                CANCEL,
                dialog.focused_button,
            ))),
        },
    );
}

pub(super) fn button_hit(area: Rect, column: u16, row: u16) -> Option<ButtonHit> {
    ui_dialog::button_hit(
        area,
        LAYOUT,
        ui_dialog::confirm_buttons(UNINSTALL, CANCEL, 0),
        (column, row),
    )
}

fn dialog_text(dialog: &DialogState, app: &App) -> Text<'static> {
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

#[cfg(not(target_os = "macos"))]
fn confirm_uninstall_text(app: &App, _focused_button: usize) -> [Line<'static>; 5] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line("Confirm Uninstall"),
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

#[cfg(target_os = "macos")]
fn confirm_uninstall_text(app: &App, _focused_button: usize) -> [Line<'static>; 6] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line("Uninstall Policies"),
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
