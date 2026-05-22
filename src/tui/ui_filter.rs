use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::styles;
use crate::app::{App, FilterState};

pub(super) const HEIGHT: u16 = 1;

const INPUT: Style = styles::CYAN.bold();
const LABEL: Style = styles::GRAY;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let content_area = area.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    frame.render_widget(Paragraph::new(input_line(app.filter())), content_area);
}

fn input_line(filter: &FilterState) -> Line<'static> {
    Line::from_iter([
        Span::styled("Filter: ", LABEL),
        Span::styled(input_label(filter), INPUT),
    ])
}

fn input_label(filter: &FilterState) -> String {
    if filter.editing() {
        format!("{}▌", filter.query)
    } else {
        filter.query.clone()
    }
}
