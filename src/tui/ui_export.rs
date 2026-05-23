use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};

use super::styles;
use super::ui_dialog::{self, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogKind, DialogState};

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 80,
    min_width: 48,
    max_width: 92,
    height_percent: 35,
    min_height: 12,
    max_height: 14,
    border_style: styles::CYAN,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const SUCCESS: Style = styles::GREEN;
const ERROR: Style = styles::RED;
const LOCATE: ButtonSpec = ("l", "Locate");
const CANCEL: ButtonSpec = ("esc", "Cancel");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    if dialog.kind != DialogKind::ExportFile {
        return;
    }

    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (0, 0),
            wrap: true,
            content: dialog_text(dialog),
            buttons: Some(buttons_line(
                dialog.export_path.is_some(),
                dialog.focused_button,
            )),
        },
    );
}

fn dialog_text(dialog: &DialogState) -> Text<'static> {
    if let Some(path) = &dialog.export_path {
        return Text::from_iter(success_lines(path).into_iter().chain(error_lines(dialog)));
    }

    Text::from_iter(failure_lines().into_iter().chain(error_lines(dialog)))
}

fn success_lines(path: &Path) -> [Line<'static>; 4] {
    [
        ui_dialog::title_line("Saved"),
        Line::default(),
        Line::styled("Policy file saved to:", ui_dialog::BODY),
        Line::from(Span::styled(path.display().to_string(), SUCCESS)),
    ]
}

fn failure_lines() -> [Line<'static>; 3] {
    [
        ui_dialog::title_line("Save Failed"),
        Line::default(),
        Line::styled("Could not save the policy file.", ui_dialog::BODY),
    ]
}

fn error_lines(dialog: &DialogState) -> impl Iterator<Item = Line<'static>> {
    dialog
        .status
        .clone()
        .into_iter()
        .flat_map(|status| [Line::default(), Line::from(Span::styled(status, ERROR))])
}

fn buttons_line(has_export_path: bool, focused_button: usize) -> Line<'static> {
    ui_dialog::confirm_or_secondary_buttons_line(has_export_path, [LOCATE, CANCEL], focused_button)
}
