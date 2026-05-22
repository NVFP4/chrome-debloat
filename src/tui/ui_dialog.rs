use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::styles;
use super::ui_text::{blank, join_lines};

pub(super) const TITLE: Style = styles::WHITE.bold();
pub(super) const BODY: Style = styles::GRAY;
pub(super) const KEY: Style = styles::DARK_GRAY;

const BUTTON: Style = styles::GRAY;
const BUTTON_BORDER: Style = styles::DARK_GRAY;
const PRIMARY_BUTTON: Style = styles::GREEN;
const FOCUSED_BUTTON: Style = Style::new().fg(Color::Black).bg(Color::Green).bold();
const BUTTON_GAP_HEIGHT: u16 = 2;

#[derive(Debug, Clone, Copy)]
pub(super) struct DialogLayout {
    pub width_percent: u16,
    pub min_width: u16,
    pub max_width: u16,
    pub height_percent: u16,
    pub min_height: u16,
    pub max_height: u16,
    pub border_style: Style,
    pub content_margin: Margin,
}

#[derive(Debug, Clone)]
pub(super) struct DialogRender {
    pub layout: DialogLayout,
    pub scroll: (u16, u16),
    pub wrap: bool,
    pub content: Text<'static>,
    pub buttons: Option<Line<'static>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Button {
    hit: ButtonHit,
    key: &'static str,
    label: &'static str,
    focused: bool,
}

pub(super) type ButtonSpec = (&'static str, &'static str);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ButtonHit(usize);

impl ButtonHit {
    pub(super) const PRIMARY: Self = Self(0);
    pub(super) const SECONDARY: Self = Self(1);
}

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, render: DialogRender) {
    let DialogRender {
        layout,
        scroll,
        wrap,
        content,
        buttons,
    } = render;

    let dialog_area = dialog_area(area, layout);
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(layout.border_style);
    let content_area = dialog_content_area(dialog_area, layout);

    frame.render_widget(block, dialog_area);

    let (content_area, button_area) = content_and_button_areas(content_area, buttons.is_some());
    let paragraph = Paragraph::new(content).scroll(scroll);
    let paragraph = if wrap {
        paragraph.wrap(Wrap { trim: true })
    } else {
        paragraph
    };

    frame.render_widget(paragraph, content_area);
    if let (Some(buttons), Some(button_area)) = (buttons, button_area) {
        frame.render_widget(Paragraph::new(buttons), button_area);
    }
}

pub(super) fn dialog_area(area: Rect, layout: DialogLayout) -> Rect {
    if area.width < layout.min_width || area.height < layout.min_height {
        return area;
    }

    let width = area
        .width
        .saturating_mul(layout.width_percent)
        .saturating_div(100)
        .clamp(layout.min_width, layout.max_width);
    let height = area
        .height
        .saturating_mul(layout.height_percent)
        .saturating_div(100)
        .clamp(layout.min_height, layout.max_height);

    let [_, dialog_row, _] = Layout::vertical([
        Constraint::Length((area.height.saturating_sub(height)) / 2),
        Constraint::Length(height),
        Constraint::Min(0),
    ])
    .areas(area);

    let [_, dialog_area, _] = Layout::horizontal([
        Constraint::Length((area.width.saturating_sub(width)) / 2),
        Constraint::Length(width),
        Constraint::Min(0),
    ])
    .areas(dialog_row);

    dialog_area
}

pub(super) fn content_area(area: Rect, layout: DialogLayout) -> Rect {
    dialog_content_area(dialog_area(area, layout), layout)
}

fn dialog_content_area(dialog_area: Rect, layout: DialogLayout) -> Rect {
    Block::default()
        .borders(Borders::ALL)
        .inner(dialog_area)
        .inner(layout.content_margin)
}

pub(super) fn title_line(title: &'static str) -> Line<'static> {
    Line::styled(title, TITLE)
}

pub(super) fn buttons_line<const N: usize>(buttons: [Button; N]) -> Line<'static> {
    join_lines(buttons.into_iter().map(Button::line), blank(2))
}

pub(super) fn confirm_buttons(
    primary: ButtonSpec,
    secondary: ButtonSpec,
    focused_button: usize,
) -> [Button; 2] {
    [
        Button::new(
            ButtonHit::PRIMARY,
            primary.0,
            primary.1,
            focused_button == 0,
        ),
        Button::new(
            ButtonHit::SECONDARY,
            secondary.0,
            secondary.1,
            focused_button == 1,
        ),
    ]
}

pub(super) fn secondary_button(secondary: ButtonSpec) -> [Button; 1] {
    [Button::new(
        ButtonHit::SECONDARY,
        secondary.0,
        secondary.1,
        true,
    )]
}

pub(super) fn confirm_or_secondary_buttons_line(
    has_primary: bool,
    buttons: [ButtonSpec; 2],
    focused_button: usize,
) -> Line<'static> {
    let [primary, secondary] = buttons;
    if has_primary {
        buttons_line(confirm_buttons(primary, secondary, focused_button))
    } else {
        buttons_line(secondary_button(secondary))
    }
}

pub(super) fn confirm_or_secondary_button_hit(
    has_primary: bool,
    area: Rect,
    layout: DialogLayout,
    buttons: [ButtonSpec; 2],
    position: (u16, u16),
) -> Option<ButtonHit> {
    let [primary, secondary] = buttons;
    if has_primary {
        button_hit(
            area,
            layout,
            confirm_buttons(primary, secondary, 0),
            position,
        )
    } else {
        button_hit(area, layout, secondary_button(secondary), position)
    }
}

pub(super) fn button_hit<const N: usize>(
    area: Rect,
    layout: DialogLayout,
    buttons: [Button; N],
    position: (u16, u16),
) -> Option<ButtonHit> {
    let (column, row) = position;
    let content_area = content_area(area, layout);
    let button_row = bottom_row(content_area)?;
    if row != button_row.y {
        return None;
    }

    let mut start = content_area.x;
    for button in buttons {
        if button_contains(button, start, column) {
            return Some(button.hit);
        }

        start = start.saturating_add(button.width()).saturating_add(2);
    }

    None
}

fn content_and_button_areas(area: Rect, has_buttons: bool) -> (Rect, Option<Rect>) {
    if !has_buttons {
        return (area, None);
    }

    let Some(button_area) = bottom_row(area) else {
        return (area, None);
    };

    (
        Rect {
            height: area
                .height
                .saturating_sub(BUTTON_GAP_HEIGHT.saturating_add(1)),
            ..area
        },
        Some(button_area),
    )
}

fn bottom_row(area: Rect) -> Option<Rect> {
    (area.width > 0 && area.height > 0).then_some(Rect {
        y: area.bottom().saturating_sub(1),
        height: 1,
        ..area
    })
}

fn button_contains(button: Button, start: u16, column: u16) -> bool {
    (start..start.saturating_add(button.width())).contains(&column)
}

impl Button {
    pub(super) const fn new(
        hit: ButtonHit,
        key: &'static str,
        label: &'static str,
        focused: bool,
    ) -> Self {
        Self {
            hit,
            key,
            label,
            focused,
        }
    }

    fn width(self) -> u16 {
        (self.key.len() + self.label.len() + 5).min(usize::from(u16::MAX)) as u16
    }

    fn line(self) -> Line<'static> {
        let (button_style, key_style, border_style) = if self.focused {
            (FOCUSED_BUTTON, FOCUSED_BUTTON, FOCUSED_BUTTON)
        } else if self.hit == ButtonHit::PRIMARY {
            (PRIMARY_BUTTON, PRIMARY_BUTTON, PRIMARY_BUTTON)
        } else {
            (BUTTON, KEY, BUTTON_BORDER)
        };

        Line::from_iter([
            Span::styled("[ ", border_style),
            Span::styled(self.key, key_style),
            Span::styled(" ", button_style),
            Span::styled(self.label, button_style),
            Span::styled(" ]", border_style),
        ])
    }
}
