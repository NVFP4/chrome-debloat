use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::styles;
use super::ui_dialog::{self, ButtonHit, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogKind};

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 70,
    min_width: 44,
    max_width: 72,
    height_percent: 30,
    min_height: 11,
    max_height: 11,
    border_style: styles::YELLOW,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const IMPORTANT: Style = styles::YELLOW.add_modifier(Modifier::BOLD);
const OKAY: ButtonSpec = ("o", "Okay");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    if dialog.kind != DialogKind::ElevatedPermissionsRequired {
        return;
    }

    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(),
            buttons: Some(ui_dialog::buttons_line(ui_dialog::secondary_button(OKAY))),
        },
    );
}

pub(super) fn button_hit(area: Rect, column: u16, row: u16) -> Option<ButtonHit> {
    ui_dialog::button_hit(
        area,
        LAYOUT,
        ui_dialog::secondary_button(OKAY),
        (column, row),
    )
}

fn dialog_text() -> Text<'static> {
    Text::from_iter([
        ui_dialog::title_line("Elevated Permissions Required"),
        Line::default(),
        Line::from_iter([
            Span::styled("Chromium policies must be written at the ", ui_dialog::BODY),
            Span::styled("system level", IMPORTANT),
            Span::styled(" for them to take effect.", ui_dialog::BODY),
        ]),
        Line::default(),
        restart_line(),
    ])
}

#[cfg(target_os = "linux")]
fn restart_line() -> Line<'static> {
    Line::from_iter([
        Span::styled("Restart this app with ", ui_dialog::BODY),
        Span::styled("sudo", IMPORTANT),
        Span::styled(".", ui_dialog::BODY),
    ])
}

#[cfg(target_os = "windows")]
fn restart_line() -> Line<'static> {
    Line::from_iter([
        Span::styled("Restart and allow this app to ", ui_dialog::BODY),
        Span::styled("make changes to this device", IMPORTANT),
        Span::styled(".", ui_dialog::BODY),
    ])
}
