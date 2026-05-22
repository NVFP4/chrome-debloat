use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

#[cfg(target_os = "linux")]
use super::ui_sudo;
use super::{
    ui_apply,
    ui_content,
    ui_export,
    ui_filter,
    ui_footer,
    ui_header,
    ui_help,
    ui_quit,
    ui_revert,
    ui_summary,
    ui_uninstall,
};
use crate::app::{App, DialogKind};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let summary_height = if app.filter_visible() {
        ui_filter::HEIGHT
    } else {
        ui_summary::height(app, area.width.saturating_sub(4))
    };

    let header_spacing =
        u16::from(area.height > ui_header::HEIGHT + summary_height + ui_footer::HEIGHT);
    let content_spacing = u16::from(
        area.height >= ui_header::HEIGHT + header_spacing + summary_height + ui_footer::HEIGHT + 2,
    );

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(ui_header::HEIGHT), Constraint::Min(0)])
            .spacing(header_spacing)
            .areas(area);
    let [summary_area, main_area] =
        Layout::vertical([Constraint::Length(summary_height), Constraint::Min(0)])
            .spacing(content_spacing)
            .areas(body_area);
    let [content_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(ui_footer::HEIGHT)])
            .areas(main_area);

    ui_header::render(frame, header_area, app);
    if app.filter_visible() {
        ui_filter::render(frame, summary_area, app);
    } else {
        ui_summary::render(frame, summary_area, app);
    }
    ui_content::render(frame, content_area, app);
    ui_footer::render(frame, footer_area, app);

    if let Some(dialog) = app.dialog() {
        match dialog.kind {
            DialogKind::Help => ui_help::render(frame, area, app),
            DialogKind::ConfirmApply => ui_apply::render(frame, area, app),
            DialogKind::ExportFile => ui_export::render(frame, area, app),
            DialogKind::ConfirmQuit => ui_quit::render(frame, area, app),
            DialogKind::ConfirmRevert => ui_revert::render(frame, area, app),
            DialogKind::ConfirmUninstall => ui_uninstall::render(frame, area, app),
            #[cfg(target_os = "linux")]
            DialogKind::SudoRequired => ui_sudo::render(frame, area, app),
        }
    }
}
