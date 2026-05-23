use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};

use super::styles;
use super::ui_dialog::{self, ButtonSpec, DialogLayout, DialogRender};
use crate::app::{App, DialogKind};
use crate::diff::DiffCounts;

const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 70,
    min_width: 44,
    max_width: 72,
    height_percent: 45,
    min_height: 14,
    max_height: 16,
    border_style: styles::GREEN,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const STATUS: Style = styles::RED;
const ADDED: Style = styles::GREEN;
const EDITED: Style = styles::YELLOW;
const DELETED: Style = styles::RED;
const IMPORTANT: Style = styles::CYAN.add_modifier(ratatui::style::Modifier::BOLD);
const APPLY: ButtonSpec = ("a", "Apply");
const CANCEL: ButtonSpec = ("esc", "Cancel");

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(dialog) = app.dialog() else {
        return;
    };
    if dialog.kind != DialogKind::ConfirmApply {
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
                !app.active_browser_state().diff_counts().is_empty(),
                dialog.focused_button,
            )),
        },
    );
}

fn dialog_text(app: &App) -> Text<'static> {
    let diff = app.active_browser_state().diff_counts();

    if diff.is_empty() {
        Text::from_iter(no_changes_lines(app).into_iter().chain(status_lines(app)))
    } else {
        Text::from_iter(apply_lines(app, diff).into_iter().chain(status_lines(app)))
    }
}

fn status_lines(app: &App) -> impl Iterator<Item = Line<'static>> {
    app.dialog()
        .and_then(|dialog| dialog.status.clone())
        .into_iter()
        .flat_map(|status| [Line::default(), Line::styled(status, STATUS)])
}

#[cfg(not(target_os = "macos"))]
fn apply_lines(app: &App, diff: DiffCounts) -> [Line<'static>; 6] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line("Confirm Apply"),
        Line::default(),
        Line::styled(
            format!("Apply pending policy changes for {}?", browser),
            ui_dialog::BODY,
        ),
        diff_summary_line(diff),
        Line::default(),
        Line::from_iter([
            Span::styled("Choosing ", ui_dialog::BODY),
            Span::styled("Apply", IMPORTANT),
            Span::styled(
                " will write changes directly to the system policy path.",
                ui_dialog::BODY,
            ),
        ]),
    ]
}

#[cfg(target_os = "macos")]
fn apply_lines(app: &App, diff: DiffCounts) -> [Line<'static>; 9] {
    let browser = app.active_browser().name();
    [
        ui_dialog::title_line("Confirm Apply"),
        Line::default(),
        Line::styled(
            format!("Apply pending policy changes for {}?", browser),
            ui_dialog::BODY,
        ),
        diff_summary_line(diff),
        Line::default(),
        Line::from_iter([
            Span::styled(
                "macOS requires new profiles to be installed and ",
                ui_dialog::BODY,
            ),
            Span::styled("approved manually", IMPORTANT),
            Span::styled(".", ui_dialog::BODY),
        ]),
        Line::default(),
        Line::from_iter([
            Span::styled("Choosing ", ui_dialog::BODY),
            Span::styled("Apply", IMPORTANT),
            Span::styled(" will open ", ui_dialog::BODY),
            Span::styled("System Settings", IMPORTANT),
            Span::styled(",", ui_dialog::BODY),
        ]),
        Line::from_iter([
            Span::styled("where you must approve the ", ui_dialog::BODY),
            Span::styled(format!("'{} Policy'", browser), IMPORTANT),
            Span::styled(" profile.", ui_dialog::BODY),
        ]),
    ]
}

fn no_changes_lines(app: &App) -> [Line<'static>; 5] {
    [
        ui_dialog::title_line("Apply"),
        Line::default(),
        Line::styled(
            format!("No pending changes for {}.", app.active_browser().name()),
            ui_dialog::BODY,
        ),
        Line::default(),
        Line::styled("Make a policy change before applying.", ui_dialog::BODY),
    ]
}

fn diff_summary_line(diff: DiffCounts) -> Line<'static> {
    Line::from_iter([
        Span::styled(format!("+{}", diff.added), ADDED),
        Span::raw("  "),
        Span::styled(format!("~{}", diff.edited), EDITED),
        Span::raw("  "),
        Span::styled(format!("-{}", diff.deleted), DELETED),
    ])
}

fn buttons_line(has_changes: bool, focused_button: usize) -> Line<'static> {
    ui_dialog::confirm_or_secondary_buttons_line(has_changes, [APPLY, CANCEL], focused_button)
}
