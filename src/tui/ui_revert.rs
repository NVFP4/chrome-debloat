use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::text::{Line, Text};

use super::styles;
use super::ui_dialog::{self, ButtonSpec, DialogLayout, DialogRender};
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
const REVERT: ButtonSpec = ("r", "Revert");
const CANCEL: ButtonSpec = ("esc", "Cancel");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    if dialog.kind != DialogKind::ConfirmRevert {
        return;
    }

    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(app),
            buttons: Some(buttons_line(
                app.active_browser_state().is_dirty(),
                dialog.focused_button,
            )),
        },
    );
}

fn dialog_text(app: &App) -> Text<'static> {
    if !app.active_browser_state().is_dirty() {
        return Text::from_iter(no_changes_lines(app));
    }

    let browser = app.active_browser().name();
    Text::from_iter([
        ui_dialog::title_line("Confirm Revert"),
        Line::default(),
        Line::styled(
            format!("Discard pending changes for {}?", browser),
            ui_dialog::BODY,
        ),
        Line::styled(
            "Policies will revert to the last applied state on your system.",
            ui_dialog::BODY,
        ),
    ])
}

fn no_changes_lines(app: &App) -> [Line<'static>; 4] {
    [
        ui_dialog::title_line("Revert"),
        Line::default(),
        Line::styled(
            format!("No pending changes for {}.", app.active_browser().name()),
            ui_dialog::BODY,
        ),
        Line::styled("There is nothing to revert.", ui_dialog::BODY),
    ]
}

fn buttons_line(has_changes: bool, focused_button: usize) -> Line<'static> {
    ui_dialog::confirm_or_secondary_buttons_line(has_changes, [REVERT, CANCEL], focused_button)
}
