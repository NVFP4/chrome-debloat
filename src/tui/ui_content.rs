use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::styles;
use super::ui_text::render_space_between;
use crate::app::App;
use crate::browser::BrowserState;
use crate::editor::{NewPolicyType, PolicyEditorState, PolicyKeyEditorState};
use crate::policy_tree::{
    CUSTOM_GROUP,
    GroupStatus,
    PolicyTree,
    PolicyTreeRow,
    PolicyTreeRowKind,
    PolicyValueSummary,
    RowId,
    RowStatus,
};

const APPLIED_MARK: Style = styles::GREEN.bold();
const KEY: Style = styles::WHITE;
const VALUE: Style = styles::WHITE;
const ADDED: Style = styles::GREEN;
const MODIFIED: Style = styles::YELLOW;
const DELETED: Style = styles::RED;
const EMPTY: Style = styles::DARK_GRAY;
const ERROR: Style = styles::RED;
const EXTENSION_NAME: Style = styles::GRAY;
const SELECTED_ROW: Style = styles::BG_DARK_GRAY;
const INPUT_ROW: Style = SELECTED_ROW;
const SCROLLBAR: Style = styles::DARK_GRAY;
const SCROLLBAR_THUMB: Style = styles::GRAY;
const PLACEHOLDER: Style = styles::CYAN;
const EDITING: Style = styles::CYAN.bold();
const EDIT_ERROR: Style = styles::RED.bold();
const TYPE_CHOICE: Style = styles::CYAN;
const SELECTED_TYPE: Style = Style::new().bg(Color::Cyan).fg(Color::Black).bold();
const GROUP_SPACING: usize = 1;
const END_SPACING: usize = 2;

#[derive(Clone, Copy)]
struct RowFormat {
    marker: &'static str,
    marker_style: Style,
    key_style: Style,
    value_style: Style,
    extension_name_style: Style,
}

struct ContentRow {
    left: Line<'static>,
    right: Line<'static>,
}

struct DisplayRow {
    row_id: Option<RowId>,
    highlighted: bool,
    content: ContentRow,
}

struct Selection<'a> {
    cursor: Option<&'a RowId>,
    style: Style,
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let content_area = area.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    if content_area.is_empty() {
        // Avoid building display rows when there is nowhere to draw them.
        return;
    }

    let rows = display_rows(app);
    let scroll = if app.policy_key_editor().is_some() {
        0
    } else {
        scroll_for_cursor(&rows, app.policy_cursor(), content_area.height)
    };
    let (rows_area, scrollbar_area) = scroll_areas(area, content_area, rows.len());

    render_rows(
        frame,
        rows_area,
        &rows,
        scroll,
        Selection {
            cursor: app.policy_cursor(),
            style: if app.editing_policy() {
                INPUT_ROW
            } else {
                SELECTED_ROW
            },
        },
    );
    render_scrollbar(
        frame,
        scrollbar_area,
        rows.len(),
        scroll,
        content_area.height,
    );
}

fn display_rows(app: &App) -> Vec<DisplayRow> {
    let state = app.active_browser_state();

    match (app.active_policy_tree(), &state.policy_error) {
        (Some(tree), _) => policy_rows(app, tree),
        (None, None) => Vec::new(),
        (None, Some(error)) => error_text(state, error),
    }
}

fn scroll_areas(area: Rect, content_area: Rect, row_count: usize) -> (Rect, Rect) {
    if row_count <= usize::from(content_area.height) || area.width < 3 {
        return (content_area, Rect::default());
    }

    (
        content_area,
        Rect {
            x: area.right().saturating_sub(1),
            width: 1,
            y: content_area.y,
            height: content_area.height,
        },
    )
}

fn render_rows(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[DisplayRow],
    scroll: usize,
    selection: Selection,
) {
    let synthetic_highlight = rows.iter().any(|row| row.highlighted);

    for (index, row) in rows
        .iter()
        .enumerate()
        .skip(scroll)
        .take(usize::from(area.height))
    {
        let row_area = Rect {
            y: area.y + (index - scroll) as u16,
            height: 1,
            ..area
        };
        let selected = matches!(
            (row.row_id.as_ref(), selection.cursor),
            (Some(row_id), Some(cursor)) if row_id == cursor
        );
        render_row(
            frame,
            row_area,
            row,
            (row.highlighted || !synthetic_highlight && selected).then_some(selection.style),
        );
    }
}

fn render_row(frame: &mut Frame<'_>, area: Rect, row: &DisplayRow, selection_style: Option<Style>) {
    if let Some(style) = selection_style {
        frame.render_widget(Paragraph::new(Line::default()).style(style), area);
    }

    render_space_between(
        frame,
        area,
        row.content.left.clone(),
        row.content.right.clone(),
    );
}

fn render_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    row_count: usize,
    scroll: usize,
    viewport_height: u16,
) {
    if area.is_empty() {
        // This scrollbar is drawn with a custom per-row loop.
        return;
    }

    let viewport_height = usize::from(viewport_height);
    let Some(thumb) = scrollbar_thumb(area.height, row_count, viewport_height, scroll) else {
        return;
    };

    for row in 0..area.height {
        let style = if (thumb.start..thumb.end()).contains(&row) {
            SCROLLBAR_THUMB
        } else {
            SCROLLBAR
        };
        let area = Rect {
            y: area.y + row,
            height: 1,
            ..area
        };

        frame.render_widget(Paragraph::new("│").style(style), area);
    }
}

fn scrollbar_thumb(
    track_height: u16,
    row_count: usize,
    viewport_height: usize,
    scroll: usize,
) -> Option<ScrollbarThumb> {
    if track_height == 0 || row_count <= viewport_height {
        return None;
    }

    let track_height = usize::from(track_height);
    let thumb_height = proportional_thumb_height(track_height, row_count, viewport_height);
    let max_scroll = row_count.saturating_sub(viewport_height);
    let max_thumb_start = track_height.saturating_sub(thumb_height);
    let thumb_start = scroll
        .min(max_scroll)
        .saturating_mul(max_thumb_start)
        .checked_div(max_scroll)
        .unwrap_or_default();

    Some(ScrollbarThumb {
        start: thumb_start as u16,
        height: thumb_height as u16,
    })
}

fn proportional_thumb_height(
    track_height: usize,
    row_count: usize,
    viewport_height: usize,
) -> usize {
    viewport_height
        .saturating_mul(track_height)
        .div_ceil(row_count)
        .clamp(1, track_height)
}

#[derive(Clone, Copy)]
struct ScrollbarThumb {
    start: u16,
    height: u16,
}

impl ScrollbarThumb {
    const fn end(self) -> u16 {
        self.start.saturating_add(self.height)
    }
}

fn policy_rows(app: &App, tree: &PolicyTree) -> Vec<DisplayRow> {
    let rows = policy_display_rows(
        tree.rows(),
        app.visible_policy_indices(),
        app.policy_editor(),
        app.policy_key_editor(),
    );

    if rows.is_empty() {
        let message = if app.filter_visible() {
            "No matching policies."
        } else {
            "Policy is empty."
        };

        std::iter::once(DisplayRow::content(ContentRow::left(Line::styled(
            message, EMPTY,
        ))))
        .collect()
    } else {
        rows
    }
}

fn new_key_row(editor: &PolicyKeyEditorState) -> DisplayRow {
    DisplayRow::highlighted(ContentRow {
        left: Line::from_iter([
            Span::raw("  "),
            Span::styled("+ ", ADDED),
            Span::styled(edit_key_label(editor), key_editor_style(editor)),
        ]),
        right: type_choices(editor),
    })
}

fn new_list_item_row(editor: &PolicyEditorState, indent: usize) -> DisplayRow {
    DisplayRow::highlighted(ContentRow::left(Line::from_iter([
        Span::raw("  ".repeat(indent)),
        Span::styled("+ ", ADDED),
        Span::styled(edit_value_label(editor), editor_style(editor)),
    ])))
}

fn type_choices(editor: &PolicyKeyEditorState) -> Line<'static> {
    Line::from_iter(
        NewPolicyType::OPTIONS
            .into_iter()
            .map(|policy_type| type_choice(editor, policy_type)),
    )
}

fn type_choice(editor: &PolicyKeyEditorState, policy_type: NewPolicyType) -> Span<'static> {
    let style = if editor.selected_type() == policy_type {
        SELECTED_TYPE
    } else {
        TYPE_CHOICE
    };

    Span::styled(format!(" {} ", policy_type.label()), style)
}

fn policy_display_rows(
    rows: &[PolicyTreeRow],
    visible_indices: &[usize],
    editor: Option<&PolicyEditorState>,
    key_editor: Option<&PolicyKeyEditorState>,
) -> Vec<DisplayRow> {
    let end_spacing = if visible_indices.is_empty() && key_editor.is_none() {
        0
    } else {
        END_SPACING
    };
    let custom_prefix_len = usize::from(
        key_editor.is_some()
            && !visible_indices
                .first()
                .and_then(|index| rows.get(*index))
                .is_some_and(is_custom_group),
    ) * (1 + usize::from(!rows.is_empty()));
    let mut display_rows =
        Vec::with_capacity(custom_prefix_len + visible_indices.len() + end_spacing);
    let has_custom_group = visible_indices
        .first()
        .and_then(|index| rows.get(*index))
        .is_some_and(is_custom_group);

    if !has_custom_group {
        push_custom_editor_rows(&mut display_rows, key_editor, !rows.is_empty());
    }

    for (position, index) in visible_indices.iter().enumerate() {
        let Some(row) = rows.get(*index) else {
            continue;
        };
        if position > 0 && matches!(row.kind, PolicyTreeRowKind::Group { .. }) {
            push_spacers(&mut display_rows, GROUP_SPACING);
        }

        display_rows.push(DisplayRow::indexed(
            row.id().clone(),
            policy_tree_row(
                row,
                editor.filter(|editor| editor.existing_cursor() == Some(row.id())),
            ),
        ));
        if let Some(new_row) = new_list_item_row_after(editor, row.id()) {
            display_rows.push(new_row);
        }
        if is_custom_group(row) {
            push_custom_editor_rows(&mut display_rows, key_editor, false);
        }
    }

    push_spacers(&mut display_rows, end_spacing);
    display_rows
}

fn push_custom_editor_rows(
    rows: &mut Vec<DisplayRow>,
    key_editor: Option<&PolicyKeyEditorState>,
    include_header: bool,
) {
    let Some(editor) = key_editor else {
        return;
    };

    if include_header {
        rows.push(DisplayRow::content(group_row(
            CUSTOM_GROUP,
            GroupStatus::None,
        )));
    }
    rows.push(new_key_row(editor));
    if include_header {
        rows.push(DisplayRow::spacer());
    }
}

fn push_spacers(rows: &mut Vec<DisplayRow>, count: usize) {
    rows.extend((0..count).map(|_| DisplayRow::spacer()));
}

fn new_list_item_row_after(editor: Option<&PolicyEditorState>, id: &RowId) -> Option<DisplayRow> {
    let editor = editor?;
    let target = editor.new_list_item()?;

    (target.insert_after == *id).then(|| new_list_item_row(editor, target.indent))
}

fn is_custom_group(row: &PolicyTreeRow) -> bool {
    matches!(&row.kind, PolicyTreeRowKind::Group { title, .. } if title == CUSTOM_GROUP)
}

fn error_text(state: &BrowserState, error: &str) -> Vec<DisplayRow> {
    [
        DisplayRow::content(ContentRow::left(Line::styled(
            format!("Could not read {} policies.", state.browser.name()),
            ERROR,
        ))),
        DisplayRow::content(ContentRow::left(Line::styled(error.to_owned(), EMPTY))),
    ]
    .into_iter()
    .collect()
}

fn policy_tree_row(row: &PolicyTreeRow, editor: Option<&PolicyEditorState>) -> ContentRow {
    match &row.kind {
        PolicyTreeRowKind::Group { title, status } => group_row(title, *status),
        PolicyTreeRowKind::Policy {
            indent,
            key,
            value,
            status,
        } => policy_row(*indent, key, value, *status, editor),
        PolicyTreeRowKind::Value {
            indent,
            value,
            status,
            extension_name,
        } => child_value_row(*indent, value, *status, extension_name.as_deref(), editor),
    }
}

fn group_row(title: &str, status: GroupStatus) -> ContentRow {
    let (indicator, indicator_style) = match status {
        GroupStatus::All => ("●", ADDED),
        GroupStatus::Some => ("◐", KEY),
        GroupStatus::None => ("○", KEY),
    };

    ContentRow::left(Line::from_iter([
        Span::styled(indicator, indicator_style),
        Span::raw(" "),
        Span::styled(title.to_owned(), KEY.bold()),
    ]))
}

fn child_value_row(
    indent: usize,
    value: &PolicyValueSummary,
    status: RowStatus,
    extension_name: Option<&str>,
    editor: Option<&PolicyEditorState>,
) -> ContentRow {
    let row_format = RowFormat::from_status(status);
    let display_format = if status == RowStatus::Applied || status == RowStatus::NotApplied {
        RowFormat {
            marker: "•",
            marker_style: EMPTY,
            ..row_format
        }
    } else {
        row_format
    };

    match editor {
        Some(editor) => ContentRow::left(Line::from_iter([
            Span::raw("  ".repeat(indent)),
            Span::styled(display_format.marker, display_format.marker_style),
            Span::raw(" "),
            Span::styled(edit_value_label(editor), editor_style(editor)),
        ])),
        None => child_value_display_row(indent, display_format, value, extension_name),
    }
}

fn policy_row(
    indent: usize,
    key: &str,
    value: &PolicyValueSummary,
    status: RowStatus,
    editor: Option<&PolicyEditorState>,
) -> ContentRow {
    let row_format = RowFormat::from_status(status);
    let prefix = format!("{}{} ", "  ".repeat(indent), row_format.marker);

    ContentRow {
        left: Line::from_iter([
            Span::styled(prefix, row_format.marker_style),
            Span::styled(key.to_owned(), row_format.key_style),
        ]),
        right: Line::styled(
            editor
                .map(edit_value_label)
                .unwrap_or_else(|| value.policy_label().to_owned()),
            editor.map_or(row_format.value_style, editor_style),
        ),
    }
}

fn child_value_display_row(
    indent: usize,
    row_format: RowFormat,
    value: &PolicyValueSummary,
    extension_name: Option<&str>,
) -> ContentRow {
    match extension_name {
        Some(extension_name) => ContentRow::left(Line::from_iter([
            Span::raw("  ".repeat(indent)),
            Span::styled(row_format.marker, row_format.marker_style),
            Span::raw(" "),
            Span::styled(value.child_label().to_owned(), row_format.value_style),
            Span::raw(" "),
            Span::styled(
                format!("({extension_name})"),
                row_format.extension_name_style,
            ),
        ])),
        None => ContentRow::left(Line::from_iter([
            Span::raw("  ".repeat(indent)),
            Span::styled(row_format.marker, row_format.marker_style),
            Span::raw(" "),
            Span::styled(value.child_label().to_owned(), row_format.value_style),
        ])),
    }
}

fn edit_value_label(editor: &PolicyEditorState) -> String {
    edit_buffer_label(editor.placeholder().unwrap_or(&editor.buffer))
}

fn edit_key_label(editor: &PolicyKeyEditorState) -> String {
    edit_buffer_label(editor.placeholder().unwrap_or(&editor.buffer))
}

fn edit_buffer_label(buffer: &str) -> String {
    format!("{buffer}▌")
}

fn editor_style(editor: &PolicyEditorState) -> Style {
    if editor.invalid {
        EDIT_ERROR
    } else if editor.placeholder().is_some() {
        PLACEHOLDER
    } else {
        EDITING
    }
}

fn key_editor_style(editor: &PolicyKeyEditorState) -> Style {
    if editor.invalid {
        EDIT_ERROR
    } else if editor.placeholder().is_some() {
        PLACEHOLDER
    } else {
        EDITING
    }
}

fn scroll_for_cursor(rows: &[DisplayRow], cursor: Option<&RowId>, height: u16) -> usize {
    let height = usize::from(height).max(1);
    let line_count = rows.len();
    let max_scroll = line_count.saturating_sub(height);
    let cursor_row = rows
        .iter()
        .position(|row| row.highlighted)
        .or_else(|| rows.iter().position(|row| row.row_id.as_ref() == cursor))
        .unwrap_or_default();
    let scroll = cursor_row.saturating_sub(height / 2);

    scroll.min(max_scroll)
}

impl RowFormat {
    const fn applied() -> Self {
        Self {
            marker: "✓",
            marker_style: APPLIED_MARK,
            key_style: KEY,
            value_style: VALUE,
            extension_name_style: EXTENSION_NAME,
        }
    }

    const fn added() -> Self {
        Self {
            marker: "+",
            marker_style: ADDED,
            key_style: ADDED,
            value_style: ADDED,
            extension_name_style: EXTENSION_NAME,
        }
    }

    const fn edited() -> Self {
        Self {
            marker: "~",
            marker_style: MODIFIED,
            key_style: MODIFIED,
            value_style: MODIFIED,
            extension_name_style: EXTENSION_NAME,
        }
    }

    const fn deleted() -> Self {
        Self {
            marker: "-",
            marker_style: DELETED,
            key_style: DELETED,
            value_style: DELETED,
            extension_name_style: EXTENSION_NAME,
        }
    }

    const fn not_applied() -> Self {
        Self {
            marker: " ",
            marker_style: EMPTY,
            key_style: EMPTY,
            value_style: EMPTY,
            extension_name_style: EMPTY,
        }
    }

    const fn from_status(status: RowStatus) -> Self {
        match status {
            RowStatus::Applied => Self::applied(),
            RowStatus::Added => Self::added(),
            RowStatus::Edited => Self::edited(),
            RowStatus::Deleted => Self::deleted(),
            RowStatus::NotApplied => Self::not_applied(),
        }
    }
}

impl ContentRow {
    fn left(left: Line<'static>) -> Self {
        Self {
            left,
            right: Line::default(),
        }
    }
}

impl DisplayRow {
    fn content(content: ContentRow) -> Self {
        Self {
            row_id: None,
            highlighted: false,
            content,
        }
    }

    fn indexed(row_id: RowId, content: ContentRow) -> Self {
        Self {
            row_id: Some(row_id),
            highlighted: false,
            content,
        }
    }

    fn highlighted(content: ContentRow) -> Self {
        Self {
            row_id: None,
            highlighted: true,
            content,
        }
    }

    fn spacer() -> Self {
        Self::content(ContentRow::left(Line::default()))
    }
}
