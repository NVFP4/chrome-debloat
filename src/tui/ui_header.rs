use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use super::styles;
use super::ui_text::{blank, join_lines, render_space_between};
use crate::app::App;
use crate::browser::BrowserState;
use crate::diff::DiffCounts;

pub(super) const HEIGHT: u16 = 3;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(styles::DARK_GRAY);
    let content_area = block.inner(area);
    frame.render_widget(block, area);

    let row_area = content_area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });

    render_space_between(
        frame,
        row_area,
        browsers_line(app),
        meta_line(app.active_browser_state()),
    );
}

fn browsers_line(app: &App) -> Line<'static> {
    join_lines(
        app.browsers().iter().enumerate().map(|(index, state)| {
            browser_line(state, index, state.browser == app.active_browser())
        }),
        blank(2),
    )
}

fn browser_line(state: &BrowserState, index: usize, active: bool) -> Line<'static> {
    join_lines(
        [
            Line::styled(format!("[{}]", index + 1), styles::DARK_GRAY),
            blank(1),
            Line::styled(state.browser.name(), browser_style(state, active)),
            dirty_marker(state, active),
        ],
        Line::default(),
    )
}

fn browser_style(state: &BrowserState, active: bool) -> Style {
    if active {
        styles::GREEN.bold()
    } else if !state.detected() {
        styles::DARK_GRAY
    } else {
        styles::GRAY
    }
}

fn dirty_marker(state: &BrowserState, active: bool) -> Line<'static> {
    if state.managed_policy_exists() && state.is_dirty() && !active {
        Line::styled("*", styles::YELLOW)
    } else {
        blank(1)
    }
}

fn meta_line(state: &BrowserState) -> Line<'static> {
    let diff = state.diff_counts();

    join_lines(
        [
            state
                .awaiting_install()
                .then(|| Line::styled("AWAITING INSTALL", styles::CYAN.bold())),
            state
                .awaiting_uninstall()
                .then(|| Line::styled("AWAITING UNINSTALL", styles::CYAN.bold())),
            (!diff.is_empty()).then(|| diff_line(diff)),
        ]
        .into_iter()
        .flatten(),
        Line::styled(" . ", styles::DARK_GRAY),
    )
}

fn diff_line(diff: DiffCounts) -> Line<'static> {
    join_lines(
        [
            Line::styled(format!("+{}", diff.added), styles::GREEN),
            Line::styled(
                format!("~{}", diff.edited),
                Style::default().fg(Color::Rgb(255, 165, 0)),
            ),
            Line::styled(format!("-{}", diff.deleted), styles::RED),
        ],
        blank(1),
    )
}
