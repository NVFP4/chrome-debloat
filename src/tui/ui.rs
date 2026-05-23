use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use super::ui_elevation;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HitTarget {
    ReportIssue,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LayoutAreas {
    pub(super) header: Rect,
    pub(super) summary: Rect,
    pub(super) content: Rect,
    pub(super) footer: Rect,
}

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let areas = layout_areas(area, app);

    ui_header::render(frame, areas.header, app);
    if app.filter_visible() {
        ui_filter::render(frame, areas.summary, app);
    } else {
        ui_summary::render(frame, areas.summary, app);
    }
    ui_content::render(frame, areas.content, app);
    ui_footer::render(frame, areas.footer, app);

    if let Some(dialog) = app.dialog() {
        match dialog.kind {
            DialogKind::Help => ui_help::render(frame, area, app),
            DialogKind::ConfirmApply => ui_apply::render(frame, area, app),
            DialogKind::ExportFile => ui_export::render(frame, area, app),
            DialogKind::ConfirmQuit => ui_quit::render(frame, area, app),
            DialogKind::ConfirmRevert => ui_revert::render(frame, area, app),
            DialogKind::ConfirmUninstall => ui_uninstall::render(frame, area, app),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            DialogKind::ElevatedPermissionsRequired => ui_elevation::render(frame, area, app),
        }
    }
}

pub(super) fn hit_test(app: &App, area: Rect, column: u16, row: u16) -> Option<HitTarget> {
    if app.dialog().is_some() {
        return None;
    }

    let areas = layout_areas(area, app);
    ui_footer::hit_test(areas.footer, app, column, row).then_some(HitTarget::ReportIssue)
}

pub(super) fn help_max_scroll(area: Rect) -> u16 {
    ui_help::max_scroll(area)
}

pub(super) fn layout_areas(area: Rect, app: &App) -> LayoutAreas {
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

    LayoutAreas {
        header: header_area,
        summary: summary_area,
        content: content_area,
        footer: footer_area,
    }
}
