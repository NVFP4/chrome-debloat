use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::styles;
use super::ui_text::{blank, join_lines};
use crate::app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPORT_ISSUE: &str = "report issue";
const INPUT_FOOTER: Style = Style::new().bg(Color::Cyan);
const INPUT_TEXT: Style = Style::new().fg(Color::Black).bold();

pub(super) const HEIGHT: u16 = 1;

type InputFooter = (&'static str, &'static str, &'static [&'static [Shortcut]]);

struct FooterLayout {
    input_active: bool,
    left: Option<TextArea>,
    right: Option<TextArea>,
    report_issue: Option<Rect>,
}

struct TextArea {
    area: Rect,
    line: Line<'static>,
}

struct FooterLines {
    left: Line<'static>,
    right: Line<'static>,
    report_issue_width: Option<usize>,
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let layout = footer_layout(area, app);
    if layout.input_active {
        frame.render_widget(Paragraph::new(Line::default()).style(INPUT_FOOTER), area);
    }
    if let Some(text) = layout.left {
        frame.render_widget(Paragraph::new(text.line), text.area);
    }
    if let Some(text) = layout.right {
        frame.render_widget(Paragraph::new(text.line), text.area);
    }
}

pub(super) fn hit_test(area: Rect, app: &App, column: u16, row: u16) -> bool {
    footer_layout(area, app)
        .report_issue
        .is_some_and(|area| contains(area, column, row))
}

fn footer_layout(area: Rect, app: &App) -> FooterLayout {
    let input = input_footer(app);
    let row_area = area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    let lines = footer_lines(row_area.width, app, input);
    let (left_area, right_area) = text_areas(row_area, lines.left.width(), lines.right.width());
    let report_issue = lines
        .report_issue_width
        .zip(right_area)
        .map(|(width, area)| Rect::new(area.x, area.y, to_u16(width), 1));

    FooterLayout {
        input_active: input.is_some(),
        left: left_area.map(|area| TextArea {
            area,
            line: lines.left,
        }),
        right: right_area.map(|area| TextArea {
            area,
            line: lines.right,
        }),
        report_issue,
    }
}

fn text_areas(area: Rect, left_width: usize, right_width: usize) -> (Option<Rect>, Option<Rect>) {
    let width = usize::from(area.width);
    if width == 0 {
        return (None, None);
    }

    let left_present = left_width > 0;
    let right_present = right_width > 0;
    let both_fit_with_gap =
        right_present && left_width.saturating_add(1).saturating_add(right_width) <= width;

    match (left_present, both_fit_with_gap, right_present) {
        (true, true, _) => (
            Some(Rect::new(area.x, area.y, to_u16(left_width), 1)),
            Some(right_aligned_area(area, right_width)),
        ),
        (true, false, _) => (Some(area), None),
        (false, _, true) => (None, Some(right_aligned_area(area, right_width))),
        (false, _, false) => (None, None),
    }
}

fn right_aligned_area(area: Rect, width: usize) -> Rect {
    let width = to_u16(width).min(area.width);

    Rect::new(area.right().saturating_sub(width), area.y, width, 1)
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    (area.x..area.right()).contains(&column) && (area.y..area.bottom()).contains(&row)
}

fn footer_lines(width: u16, app: &App, input: Option<InputFooter>) -> FooterLines {
    if let Some((full_label, compact_label, shortcuts)) = input {
        return FooterLines {
            left: input_footer_line(usize::from(width), full_label, compact_label, shortcuts),
            right: Line::default(),
            report_issue_width: None,
        };
    }

    normal_footer_lines(width, app.new_policy_shortcut_label())
}

fn input_footer(app: &App) -> Option<InputFooter> {
    if app.filter_input_active() {
        Some(("FILTER:", "FILTER", FILTER_FOOTER_SHORTCUTS))
    } else if app.policy_key_editor().is_some() {
        Some(("NEW KEY:", "NEW KEY", NEW_KEY_FOOTER_SHORTCUTS))
    } else if app.policy_editor().is_some() {
        Some(("NEW VALUE:", "NEW VALUE", NEW_VALUE_FOOTER_SHORTCUTS))
    } else if !app.filter().query.is_empty() {
        Some(("FILTER", "FILTER", FILTER_VIEW_FOOTER_SHORTCUTS))
    } else {
        None
    }
}

fn normal_footer_lines(width: u16, new_label: Option<&'static str>) -> FooterLines {
    let width = usize::from(width);
    let right = normal_right_line(width);
    let right_gap = usize::from(right.width() > 0);
    let left = normal_left_line(width.saturating_sub(right.width() + right_gap), new_label);

    FooterLines {
        left,
        right: right.line,
        report_issue_width: right.report_issue_width,
    }
}

struct RightLine {
    line: Line<'static>,
    report_issue_width: Option<usize>,
}

impl RightLine {
    fn width(&self) -> usize {
        self.line.width()
    }
}

fn normal_right_line(available_width: usize) -> RightLine {
    let report_issue = report_issue_line();
    if report_issue.width() <= available_width {
        return RightLine {
            line: report_issue,
            report_issue_width: Some(REPORT_ISSUE.len()),
        };
    }

    let version = Line::styled(VERSION, styles::GRAY);
    if version.width() <= available_width {
        return RightLine {
            line: version,
            report_issue_width: None,
        };
    }

    RightLine {
        line: Line::default(),
        report_issue_width: None,
    }
}

fn normal_left_line(available_width: usize, new_label: Option<&'static str>) -> Line<'static> {
    let full = shortcut_line(shortcuts_with_new_label(&FULL_SHORTCUTS, new_label));
    if full.width() <= available_width {
        return full;
    }

    for visible_prefix in [3, 2, 1] {
        let collapsed = collapsed_shortcut_line(
            shortcuts_with_new_label(&FULL_SHORTCUTS, new_label).take(visible_prefix),
        );
        if collapsed.width() <= available_width {
            return collapsed;
        }
    }

    let core = shortcut_line(shortcuts_with_new_label(CORE_SHORTCUTS, new_label));
    if core.width() <= available_width {
        return core;
    }

    let key_only = shortcut_line(shortcuts_with_new_label(
        KEY_ONLY_SHORTCUTS,
        new_label.map(|_| ""),
    ));
    if key_only.width() <= available_width {
        return key_only;
    }

    Line::default()
}

fn input_footer_line(
    available_width: usize,
    full_label: &'static str,
    compact_label: &'static str,
    shortcut_variants: &[&[Shortcut]],
) -> Line<'static> {
    for shortcuts in shortcut_variants {
        let line = join_lines(
            [
                Line::styled(full_label, INPUT_TEXT),
                input_shortcut_line(shortcuts),
            ],
            blank(2),
        );
        if line.width() <= available_width {
            return line;
        }
    }

    let compact = Line::styled(compact_label, INPUT_TEXT);
    if compact.width() <= available_width {
        return compact;
    }

    Line::default()
}

fn collapsed_shortcut_line(
    visible_prefix_items: impl IntoIterator<Item = Shortcut>,
) -> Line<'static> {
    join_lines(
        [
            shortcut_line(visible_prefix_items),
            Line::styled("…", styles::GRAY),
            shortcut_line(HELP_SHORTCUTS.iter().copied()),
        ],
        blank(2),
    )
}

fn report_issue_line() -> Line<'static> {
    join_lines(
        [
            Line::styled(REPORT_ISSUE, issue_style()),
            Line::styled(VERSION, styles::GRAY),
        ],
        blank(2),
    )
}

fn shortcut_line(items: impl IntoIterator<Item = Shortcut>) -> Line<'static> {
    join_lines(
        items.into_iter().map(|shortcut| shortcut.normal_line()),
        blank(2),
    )
}

fn input_shortcut_line(items: &[Shortcut]) -> Line<'static> {
    join_lines(
        items
            .iter()
            .copied()
            .map(|shortcut| shortcut.line_with(INPUT_TEXT)),
        blank(2),
    )
}

fn issue_style() -> Style {
    styles::YELLOW.add_modifier(Modifier::UNDERLINED)
}

fn to_u16(width: usize) -> u16 {
    width.min(usize::from(u16::MAX)) as u16
}

#[derive(Debug, Clone, Copy)]
struct Shortcut {
    key: &'static str,
    label: &'static str,
}

const FULL_SHORTCUTS: [Shortcut; 7] = [
    Shortcut::new("a", "apply"),
    Shortcut::new("z", "undo"),
    Shortcut::new("n", "new key"),
    Shortcut::new("R", "revert"),
    Shortcut::new("U", "uninstall"),
    Shortcut::new("q", "quit"),
    Shortcut::new("?", "help"),
];

const CORE_SHORTCUTS: &[Shortcut] = &[
    Shortcut::new("a", "apply"),
    Shortcut::new("n", "new key"),
    Shortcut::new("q", "quit"),
    Shortcut::new("?", "help"),
];
const HELP_SHORTCUTS: &[Shortcut] = &[Shortcut::new("q", "quit"), Shortcut::new("?", "help")];
const KEY_ONLY_SHORTCUTS: &[Shortcut] = &[
    Shortcut::new("a", ""),
    Shortcut::new("n", ""),
    Shortcut::new("q", ""),
    Shortcut::new("?", ""),
];

fn shortcuts_with_new_label<'a>(
    shortcuts: &'a [Shortcut],
    new_label: Option<&'static str>,
) -> impl Iterator<Item = Shortcut> + 'a {
    shortcuts.iter().copied().filter_map(move |shortcut| {
        if shortcut.key != "n" {
            return Some(shortcut);
        }

        new_label.map(|label| Shortcut::new("n", label))
    })
}
const NEW_KEY_SHORTCUTS: &[Shortcut] = &[
    Shortcut::new("← →", "type"),
    Shortcut::new("enter", "create"),
    Shortcut::new("esc", "cancel"),
];
const NEW_VALUE_SHORTCUTS: &[Shortcut] = &[
    Shortcut::new("enter", "save"),
    Shortcut::new("esc", "cancel"),
];
const FILTER_SHORTCUTS: &[Shortcut] =
    &[Shortcut::new("tab", "keep"), Shortcut::new("esc", "clear")];
const FILTER_VIEW_SHORTCUTS: &[Shortcut] =
    &[Shortcut::new("/", "edit"), Shortcut::new("esc", "clear")];
const NEW_KEY_FOOTER_SHORTCUTS: &[&[Shortcut]] = &[NEW_KEY_SHORTCUTS];
const NEW_VALUE_FOOTER_SHORTCUTS: &[&[Shortcut]] = &[NEW_VALUE_SHORTCUTS];
const FILTER_FOOTER_SHORTCUTS: &[&[Shortcut]] = &[FILTER_SHORTCUTS];
const FILTER_VIEW_FOOTER_SHORTCUTS: &[&[Shortcut]] = &[FILTER_VIEW_SHORTCUTS];

impl Shortcut {
    const fn new(key: &'static str, label: &'static str) -> Self {
        Self { key, label }
    }

    fn normal_line(self) -> Line<'static> {
        if self.label.is_empty() {
            return self.key_line_with(styles::GRAY);
        }

        join_lines(
            [
                self.key_line_with(styles::GRAY),
                blank(1),
                Line::styled(self.label, styles::DARK_GRAY),
            ],
            Line::default(),
        )
    }

    fn line_with(self, style: Style) -> Line<'static> {
        if self.label.is_empty() {
            self.key_line_with(style)
        } else {
            join_lines(
                [
                    self.key_line_with(style),
                    blank(1),
                    Line::styled(self.label, style),
                ],
                Line::default(),
            )
        }
    }

    fn key_line_with(self, style: Style) -> Line<'static> {
        Line::styled(self.key, style)
    }
}
