use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};

use super::styles;
use super::ui_dialog::{self, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogState};
use crate::chromium::policy::CustomBrowserId;

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 80,
    min_width: 48,
    max_width: 92,
    height_percent: 35,
    min_height: 13,
    max_height: 14,
    border_style: styles::CYAN,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const INPUT: Style = styles::WHITE;
const ERROR: Style = styles::RED;
const LOAD: ButtonSpec = ("enter", "Load");
const CANCEL: ButtonSpec = ("esc", "Cancel");

pub fn render(frame: &mut Frame<'_>, area: Rect, _app: &App, dialog: &DialogState) {
    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(dialog),
            buttons: Some(ui_dialog::confirm_or_secondary_buttons_line(
                true,
                [LOAD, CANCEL],
                dialog.focused_button,
            )),
        },
    );
}

fn dialog_text(dialog: &DialogState) -> Text<'static> {
    let status = dialog
        .status
        .clone()
        .into_iter()
        .map(|status| Line::styled(status, ERROR));

    Text::from_iter(
        [
            ui_dialog::title_line("Any Chromium Browser"),
            Line::default(),
            Line::styled(CustomBrowserId::hint(), ui_dialog::BODY),
            Line::styled("Existing policies are loaded.", ui_dialog::BODY),
            Line::default(),
            Line::styled(format!("> {}▌", dialog.input), INPUT),
        ]
        .into_iter()
        .chain(status),
    )
}
