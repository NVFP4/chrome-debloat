use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

use super::styles;
use super::ui_dialog::{self, DialogLayout, DialogRender};
use super::ui_text::{blank, join_lines};
use crate::app::App;

pub const HELP_LINE_COUNT: u16 = 30;
const LAYOUT: DialogLayout = DialogLayout {
    width_percent: 80,
    min_width: 48,
    max_width: 84,
    height_percent: 80,
    min_height: 16,
    max_height: 26,
    border_style: styles::CYAN,
    content_margin: Margin {
        horizontal: 1,
        vertical: 0,
    },
};
const HEADER: Style = styles::WHITE.bold();
const KEY: Style = styles::CYAN;
const DETAIL: Style = styles::GRAY;
const SCROLLBAR: Style = styles::DARK_GRAY;
const SCROLLBAR_THUMB: Style = styles::GRAY;
const END_SPACING: usize = 2;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    ui_dialog::render(
        frame,
        area,
        DialogRender {
            layout: LAYOUT,
            scroll: (app.help_scroll(), 0),
            wrap: false,
            content: help_text(),
            buttons: None,
        },
    );

    render_scrollbar(frame, area, app.help_scroll());
}

pub(super) fn max_scroll(area: Rect) -> u16 {
    let content_area = ui_dialog::content_area(area, LAYOUT);

    max_scroll_for_height(content_area.height)
}

fn help_text() -> Text<'static> {
    Text::from_iter(
        [
            ui_dialog::title_line("Help"),
            Line::default(),
            header_line("Navigation"),
            binding_line("        h ←", "previous policy group"),
            binding_line("        l →", "next policy group"),
            binding_line("        j ↓", "move down"),
            binding_line("        k ↑", "move up"),
            binding_line("        [ ]", "jump between policy groups"),
            binding_line("        1-9", "jump to tab number"),
            Line::default(),
            header_line("Global Actions"),
            binding_line("          a", "Apply changes"),
            binding_line("          S", "Save policy file"),
            binding_line("          z", "Undo last change"),
            binding_line("          r", "Redo last change"),
            binding_line("          R", "Revert current changes to baseline"),
            binding_line("          U", "Uninstall policies"),
            binding_line("          q", "Quit"),
            binding_line("          ?", "Show Help"),
            Line::default(),
            header_line("Editor Action"),
            binding_line("      space", "Toggle highlighted group/key"),
            binding_line("        i n", "Insert new key or list item"),
            binding_line("    d bcksp", "Stage highlighted policy removal"),
            binding_line("    e enter", "Edit scalar value or toggle boolean"),
            binding_line("      enter", "Save inline edit"),
            binding_line("        esc", "Cancel inline edit"),
            binding_line("          /", "Filter keys in list"),
        ]
        .into_iter()
        .chain((0..END_SPACING).map(|_| Line::default())),
    )
}

fn render_scrollbar(frame: &mut Frame<'_>, area: Rect, scroll: u16) {
    let dialog_area = ui_dialog::dialog_area(area, LAYOUT);
    let content_area = ui_dialog::content_area(area, LAYOUT);
    let area = scrollbar_area(dialog_area, content_area);
    if area.is_empty() {
        // Avoid configuring a scrollbar when there is no drawable track.
        return;
    }

    let viewport_height = usize::from(content_area.height);
    let max_scroll = usize::from(max_scroll_for_height(content_area.height));
    let scrollable_positions = max_scroll.saturating_add(1);
    let scroll = usize::from(scroll).min(max_scroll);
    let mut state = ScrollbarState::new(scrollable_positions)
        .position(scroll)
        .viewport_content_length(viewport_height);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("│"))
        .thumb_symbol("│")
        .track_style(SCROLLBAR)
        .thumb_style(SCROLLBAR_THUMB);

    frame.render_stateful_widget(scrollbar, area, &mut state);
}

fn scrollbar_area(dialog_area: Rect, content_area: Rect) -> Rect {
    if max_scroll_for_height(content_area.height) == 0 || dialog_area.width < 3 {
        return Rect::default();
    }

    Rect {
        x: dialog_area.right().saturating_sub(2),
        width: 1,
        y: content_area.y,
        height: content_area.height,
    }
}

fn max_scroll_for_height(height: u16) -> u16 {
    HELP_LINE_COUNT.saturating_sub(height)
}

fn header_line(text: &'static str) -> Line<'static> {
    Line::styled(text, HEADER)
}

fn binding_line(binding: &'static str, detail: &'static str) -> Line<'static> {
    join_lines(
        [
            blank(4),
            Line::styled(binding, KEY),
            blank(3),
            Line::styled(detail, DETAIL),
        ],
        Line::default(),
    )
}
