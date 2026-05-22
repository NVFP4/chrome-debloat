use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

pub(super) fn blank(width: usize) -> Line<'static> {
    match width {
        0 => Line::default(),
        1 => Line::raw(" "),
        2 => Line::raw("  "),
        _ => Line::raw(" ".repeat(width)),
    }
}

pub(super) fn join_lines(
    groups: impl IntoIterator<Item = Line<'static>>,
    separator: Line<'static>,
) -> Line<'static> {
    groups
        .into_iter()
        .filter(|line| line.width() > 0)
        .enumerate()
        .flat_map(|(index, line)| {
            let prefix = if index > 0 {
                Some(separator.clone())
            } else {
                None
            };

            prefix
                .into_iter()
                .chain(std::iter::once(line))
                .flat_map(styled_spans)
        })
        .collect()
}

fn styled_spans(line: Line<'static>) -> impl Iterator<Item = ratatui::text::Span<'static>> {
    let style = line.style;

    line.into_iter().map(move |span| span.patch_style(style))
}

pub(super) fn render_space_between(
    frame: &mut Frame<'_>,
    area: Rect,
    left: Line<'static>,
    right: Line<'static>,
) {
    let width = usize::from(area.width);
    if width == 0 {
        return;
    }

    let left_width = left.width();
    let right_width = right.width();
    let left_present = left_width > 0;
    // keep atleast one column between them
    let both_fit_with_gap =
        right_width > 0 && left_width.saturating_add(1).saturating_add(right_width) <= width;

    match (left_present, both_fit_with_gap) {
        (true, true) => render_left_and_right(frame, area, left, right),
        (true, false) => frame.render_widget(Paragraph::new(left), area),
        // render right-aligned
        (false, _) => frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), area),
    }
}

fn render_left_and_right(
    frame: &mut Frame<'_>,
    area: Rect,
    left: Line<'static>,
    right: Line<'static>,
) {
    let [left_area, right_area] = Layout::horizontal([
        Constraint::Length(to_u16(left.width())),
        Constraint::Length(to_u16(right.width())),
    ])
    .flex(Flex::SpaceBetween)
    .areas(area);

    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(right), right_area);
}

fn to_u16(width: usize) -> u16 {
    width.min(usize::from(u16::MAX)) as u16
}
