use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;

use super::styles;
use crate::app::App;
use crate::browser::BrowserState;

const SUMMARY: Style = styles::GRAY;
const NOTICE: Style = styles::YELLOW;
const BROWSER_DETECTION_FAILED: &str = "Browser detection failed, but policy can still be applied. Recommended policies have been selected.";
const BROWSER_NOT_INSTALLED: &str = "Browser not installed, but policy can still be applied. Recommended policies have been selected.";
const POLICY_NOT_FOUND: &str =
    "No policy found on the system. Recommended policies have been selected.";

pub(super) fn height(app: &App, width: u16) -> u16 {
    summary_lines(app.active_browser_state(), width).len() as u16
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let content_area = area.inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    let lines = summary_lines(app.active_browser_state(), content_area.width);
    let paragraph = Paragraph::new(Text::from(lines));

    frame.render_widget(paragraph, content_area);
}

fn summary_lines(state: &BrowserState, width: u16) -> Vec<Line<'static>> {
    match (&state.policy, &state.policy_error) {
        (Some(policy), _) if state.managed_policy_exists() => policy_summary_lines(
            policy.policy_count(),
            policy.extension_count(),
            policy.source.to_string(),
            width,
        ),
        (Some(_), _) | (None, None) => missing_policy_lines(state),
        (None, Some(error)) => [
            Line::styled("policy read failed", SUMMARY),
            Line::styled(error.clone(), SUMMARY),
        ]
        .into_iter()
        .collect(),
    }
}

fn missing_policy_lines(state: &BrowserState) -> Vec<Line<'static>> {
    if let Some(error) = &state.install_error {
        [
            Line::styled(BROWSER_DETECTION_FAILED, NOTICE),
            Line::styled(error.clone(), SUMMARY),
        ]
        .into_iter()
        .collect()
    } else if !state.detected() {
        std::iter::once(Line::styled(BROWSER_NOT_INSTALLED, NOTICE)).collect()
    } else {
        std::iter::once(Line::styled(POLICY_NOT_FOUND, NOTICE)).collect()
    }
}

fn policy_summary_lines(
    policy_count: usize,
    extension_count: usize,
    path: String,
    width: u16,
) -> Vec<Line<'static>> {
    let counts = format!(
        "{} • {}",
        count_text(policy_count, "policy"),
        count_text(extension_count, "extension")
    );
    let full_summary = format!("Detected {counts} • \"{path}\"");

    if line_width(&full_summary) <= usize::from(width) {
        std::iter::once(Line::styled(full_summary, SUMMARY)).collect()
    } else {
        std::iter::once(Line::styled(format!("Detected {counts}"), SUMMARY)).collect()
    }
}

fn count_text(count: usize, singular: &str) -> String {
    let noun = match (count, singular) {
        (1, noun) => noun.to_owned(),
        (_, "policy") => "policies".to_owned(),
        (_, noun) => format!("{noun}s"),
    };

    format!("{count} {noun}")
}

fn line_width(text: &str) -> usize {
    Line::raw(text.to_owned()).width()
}
