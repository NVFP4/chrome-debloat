use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Text};

use super::styles;
use super::ui_dialog::{self, ButtonHit, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogKind};

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 70,
    min_width: 44,
    max_width: 72,
    height_percent: 30,
    min_height: 9,
    max_height: 9,
    border_style: styles::YELLOW,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const QUIT: ButtonSpec = ("q", "Quit");
const CANCEL: ButtonSpec = ("esc", "Cancel");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    if dialog.kind != DialogKind::ConfirmQuit {
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
            buttons: Some(ui_dialog::buttons_line(ui_dialog::confirm_buttons(
                QUIT,
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
        ui_dialog::confirm_buttons(QUIT, CANCEL, 0),
        (column, row),
    )
}

fn dialog_text() -> Text<'static> {
    Text::from_iter([
        ui_dialog::title_line("Confirm Quit"),
        Line::default(),
        Line::styled(
            "You have pending changes. Are you sure you want to quit?",
            ui_dialog::BODY,
        ),
        Line::styled("Any unapplied policy edits will be lost.", ui_dialog::BODY),
    ])
}
