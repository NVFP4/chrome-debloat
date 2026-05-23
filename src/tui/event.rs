use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    self,
    Event,
    KeyCode,
    KeyEvent,
    KeyEventKind,
    KeyModifiers,
    MouseButton,
    MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;

use super::action::{Action, ActionStep, BrowserTabIndex};
use super::ui::{self, HitTarget};
use crate::app::{App, DialogKind};

#[derive(Debug, Clone, Copy)]
pub struct DialogInput {
    pub kind: DialogKind,
    pub primary_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PolicyInputMode {
    Edit,
    Filter,
    Key,
}

pub fn read_action(app: &App, tick_rate: Duration, area: Rect) -> Result<Action> {
    if !event::poll(tick_rate).context("poll terminal event")? {
        return Ok(Action::Tick);
    }

    match event::read().context("read terminal event")? {
        Event::Key(key_event) => {
            #[cfg(target_os = "windows")]
            if windows_paste_key(key_event) && app.input_active() {
                return Ok(crate::windows::clipboard_text().map_or(Action::Noop, Action::Paste));
            }

            Ok(key_to_action(
                key_event,
                app.dialog_input(),
                app.policy_input_mode(),
                area,
            ))
        }
        Event::Mouse(mouse_event) => Ok(mouse_to_action(mouse_event, app, area)),
        Event::Resize(_, _) => Ok(Action::Redraw),
        Event::Paste(text) => Ok(Action::Paste(text)),
        Event::FocusGained | Event::FocusLost => Ok(Action::Noop),
    }
}

fn key_to_action(
    key_event: KeyEvent,
    dialog: Option<DialogInput>,
    policy_input: Option<PolicyInputMode>,
    area: Rect,
) -> Action {
    if key_event.kind == KeyEventKind::Release {
        return Action::Noop;
    }

    if key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('c' | 'C'))
    {
        return Action::Quit;
    }

    if let Some(dialog) = dialog {
        return dialog_key_to_action(key_event, dialog, area);
    }

    if let Some(policy_input) = policy_input {
        return policy_edit_key_to_action(key_event, policy_input);
    }

    match key_event.code {
        KeyCode::Char('?') => Action::ToggleHelp,
        KeyCode::Char('/') => Action::BeginFilter,
        KeyCode::Char('a') => Action::OpenApplyDialog,
        KeyCode::Char('S') => Action::OpenExportDialog,
        KeyCode::Enter | KeyCode::Char('e') => Action::BeginPolicyEdit,
        KeyCode::Char('R') => Action::OpenRevertDialog,
        KeyCode::Char('U') => Action::OpenUninstallDialog,
        KeyCode::Char('i' | 'n') => Action::NewPolicyItem,
        KeyCode::Char('r') => Action::Redo,
        KeyCode::Char('z') => Action::Undo,
        KeyCode::Char('d') | KeyCode::Backspace => Action::StagePolicyRemoval,
        KeyCode::Char(' ') => Action::TogglePolicyPresence,
        KeyCode::Char('[' | 'h') | KeyCode::Left => Action::MovePolicyGroup(ActionStep::PREVIOUS),
        KeyCode::Char(']' | 'l') | KeyCode::Right => Action::MovePolicyGroup(ActionStep::NEXT),
        KeyCode::Char('j') | KeyCode::Down => Action::MovePolicyCursor(ActionStep::NEXT),
        KeyCode::Char('k') | KeyCode::Up => Action::MovePolicyCursor(ActionStep::PREVIOUS),
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::CancelFilter,
        KeyCode::End => Action::PolicyCursorEnd,
        KeyCode::Home => Action::PolicyCursorStart,
        KeyCode::PageDown => Action::MovePolicyCursor(ActionStep::NEXT_POLICY_PAGE),
        KeyCode::PageUp => Action::MovePolicyCursor(ActionStep::PREVIOUS_POLICY_PAGE),
        KeyCode::Char(character) => {
            BrowserTabIndex::from_digit(character).map_or(Action::Noop, Action::SelectTab)
        }
        _ => Action::Noop,
    }
}

#[cfg(target_os = "windows")]
fn windows_paste_key(key_event: KeyEvent) -> bool {
    if key_event.kind == KeyEventKind::Release {
        return false;
    }

    let ctrl_v = key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('v' | 'V'));
    let shift_insert =
        key_event.modifiers.contains(KeyModifiers::SHIFT) && key_event.code == KeyCode::Insert;

    ctrl_v || shift_insert
}

fn policy_edit_key_to_action(key_event: KeyEvent, mode: PolicyInputMode) -> Action {
    if matches!(mode, PolicyInputMode::Key) {
        return policy_key_key_to_action(key_event);
    }
    if matches!(mode, PolicyInputMode::Filter) {
        return filter_key_to_action(key_event);
    }

    match key_event.code {
        KeyCode::Esc => Action::CancelPolicyEdit,
        KeyCode::Enter => Action::CommitPolicyEdit,
        KeyCode::Backspace => Action::BackspacePolicyEdit,
        KeyCode::Char(character) => Action::InputPolicyEdit(character),
        _ => Action::Noop,
    }
}

fn filter_key_to_action(key_event: KeyEvent) -> Action {
    match key_event.code {
        KeyCode::Esc => Action::CancelFilter,
        KeyCode::Enter | KeyCode::Tab | KeyCode::Down => Action::CommitFilter,
        KeyCode::Backspace => Action::BackspaceFilter,
        KeyCode::Char(character) => Action::InputFilter(character),
        _ => Action::Noop,
    }
}

fn policy_key_key_to_action(key_event: KeyEvent) -> Action {
    match key_event.code {
        KeyCode::Esc => Action::CancelPolicyEdit,
        KeyCode::Backspace => Action::BackspacePolicyEdit,
        KeyCode::Enter | KeyCode::Char(' ') => Action::CommitPolicyEdit,
        KeyCode::Right | KeyCode::Tab => Action::MovePolicyType(ActionStep::NEXT),
        KeyCode::Left | KeyCode::BackTab => Action::MovePolicyType(ActionStep::PREVIOUS),
        KeyCode::Char(character) => Action::InputPolicyEdit(character),
        _ => Action::Noop,
    }
}

fn dialog_key_to_action(key_event: KeyEvent, dialog: DialogInput, area: Rect) -> Action {
    if dialog.kind == DialogKind::Help {
        return help_key_to_action(key_event, area);
    }

    match key_event.code {
        KeyCode::Esc => Action::CloseDialog,
        KeyCode::Char('q') if dialog.kind == DialogKind::ExportFile => Action::CloseDialog,
        KeyCode::Char('l') if dialog.kind == DialogKind::ExportFile && dialog.primary_enabled => {
            Action::LocateExportFile
        }
        KeyCode::Enter | KeyCode::Char(' ') => Action::ActivateDialogButton,
        KeyCode::Char('h') | KeyCode::Left => Action::MoveDialogFocus(ActionStep::PREVIOUS),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
            Action::MoveDialogFocus(ActionStep::NEXT)
        }
        KeyCode::Char('a') if dialog.kind == DialogKind::ConfirmApply && dialog.primary_enabled => {
            Action::ConfirmApply
        }
        KeyCode::Char('q') if dialog.kind == DialogKind::ConfirmQuit && dialog.primary_enabled => {
            Action::ConfirmQuit
        }
        #[cfg(not(target_os = "macos"))]
        KeyCode::Char('u')
            if dialog.kind == DialogKind::ConfirmUninstall && dialog.primary_enabled =>
        {
            Action::ConfirmUninstall
        }
        #[cfg(target_os = "macos")]
        KeyCode::Char('o')
            if dialog.kind == DialogKind::ConfirmUninstall && dialog.primary_enabled =>
        {
            Action::ConfirmUninstall
        }
        KeyCode::Char('r')
            if dialog.kind == DialogKind::ConfirmRevert && dialog.primary_enabled =>
        {
            Action::ConfirmRevert
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        KeyCode::Char('o') if dialog.kind == DialogKind::ElevatedPermissionsRequired => {
            Action::CloseDialog
        }
        _ => Action::Noop,
    }
}

fn mouse_to_action(mouse_event: MouseEvent, app: &App, area: Rect) -> Action {
    match mouse_event.kind {
        MouseEventKind::ScrollUp => match app.dialog_input().map(|dialog| dialog.kind) {
            Some(DialogKind::Help) => help_scroll_action(ActionStep::PREVIOUS, area),
            Some(_) => Action::Noop,
            None => Action::MovePolicyCursor(ActionStep::PREVIOUS),
        },
        MouseEventKind::ScrollDown => match app.dialog_input().map(|dialog| dialog.kind) {
            Some(DialogKind::Help) => help_scroll_action(ActionStep::NEXT, area),
            Some(_) => Action::Noop,
            None => Action::MovePolicyCursor(ActionStep::NEXT),
        },
        MouseEventKind::Down(MouseButton::Left) => mouse_click_action(mouse_event, app, area),
        _ => Action::Noop,
    }
}

fn mouse_click_action(mouse_event: MouseEvent, app: &App, area: Rect) -> Action {
    ui::hit_test(app, area, mouse_event.column, mouse_event.row)
        .map_or(Action::Noop, hit_target_action)
}

fn hit_target_action(target: HitTarget) -> Action {
    match target {
        HitTarget::ReportIssue => Action::OpenReportIssue,
    }
}

fn help_key_to_action(key_event: KeyEvent, area: Rect) -> Action {
    match key_event.code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => Action::CloseDialog,
        KeyCode::Char('j') | KeyCode::Down => help_scroll_action(ActionStep::NEXT, area),
        KeyCode::Char('k') | KeyCode::Up => help_scroll_action(ActionStep::PREVIOUS, area),
        KeyCode::PageDown => help_scroll_action(ActionStep::NEXT_HELP_PAGE, area),
        KeyCode::PageUp => help_scroll_action(ActionStep::PREVIOUS_HELP_PAGE, area),
        _ => Action::Noop,
    }
}

fn help_scroll_action(step: ActionStep, area: Rect) -> Action {
    let max_scroll = ui::help_max_scroll(area);

    Action::ScrollHelp { step, max_scroll }
}
