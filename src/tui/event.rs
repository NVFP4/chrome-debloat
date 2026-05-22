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
use crossterm::terminal;
use ratatui::layout::Rect;

use super::action::Action;
use super::ui_dialog::ButtonHit;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use super::ui_elevation;
use super::{ui_apply, ui_export, ui_footer, ui_help, ui_quit, ui_revert, ui_uninstall};
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

pub fn read_action(app: &App, tick_rate: Duration) -> Result<Action> {
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
            ))
        }
        Event::Mouse(mouse_event) => Ok(mouse_to_action(mouse_event, app)),
        Event::Resize(_, _) => Ok(Action::Redraw),
        Event::Paste(text) => Ok(Action::Paste(text)),
        Event::FocusGained | Event::FocusLost => Ok(Action::Noop),
    }
}

fn key_to_action(
    key_event: KeyEvent,
    dialog: Option<DialogInput>,
    policy_input: Option<PolicyInputMode>,
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
        return dialog_key_to_action(key_event, dialog);
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
        KeyCode::Char('[' | 'h') | KeyCode::Left => Action::MovePolicyGroup(-1),
        KeyCode::Char(']' | 'l') | KeyCode::Right => Action::MovePolicyGroup(1),
        KeyCode::Char('j') | KeyCode::Down => Action::MovePolicyCursor(1),
        KeyCode::Char('k') | KeyCode::Up => Action::MovePolicyCursor(-1),
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::CancelFilter,
        KeyCode::End => Action::PolicyCursorEnd,
        KeyCode::Home => Action::PolicyCursorStart,
        KeyCode::PageDown => Action::MovePolicyCursor(8),
        KeyCode::PageUp => Action::MovePolicyCursor(-8),
        KeyCode::Char(character) if ('1'..='9').contains(&character) => {
            Action::SelectTab(character as usize - '1' as usize)
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
        KeyCode::Right | KeyCode::Tab => Action::MovePolicyType(1),
        KeyCode::Left | KeyCode::BackTab => Action::MovePolicyType(-1),
        KeyCode::Char(character) => Action::InputPolicyEdit(character),
        _ => Action::Noop,
    }
}

fn dialog_key_to_action(key_event: KeyEvent, dialog: DialogInput) -> Action {
    if dialog.kind == DialogKind::Help {
        return help_key_to_action(key_event);
    }

    match key_event.code {
        KeyCode::Esc => Action::CloseDialog,
        KeyCode::Char('q') if dialog.kind == DialogKind::ExportFile => Action::CloseDialog,
        KeyCode::Char('l') if dialog.kind == DialogKind::ExportFile && dialog.primary_enabled => {
            Action::LocateExportFile
        }
        KeyCode::Enter | KeyCode::Char(' ') => Action::ActivateDialogButton,
        KeyCode::Char('h') | KeyCode::Left => Action::MoveDialogFocus(-1),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => Action::MoveDialogFocus(1),
        KeyCode::Char('a') if dialog.kind == DialogKind::ConfirmApply && dialog.primary_enabled => {
            Action::ConfirmApply
        }
        KeyCode::Char('q') if dialog.kind == DialogKind::ConfirmQuit && dialog.primary_enabled => {
            Action::ConfirmQuit
        }
        #[cfg(not(target_os = "macos"))]
        KeyCode::Char('u') if dialog.kind == DialogKind::ConfirmUninstall => {
            Action::ConfirmUninstall
        }
        #[cfg(target_os = "macos")]
        KeyCode::Char('o') if dialog.kind == DialogKind::ConfirmUninstall => {
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

fn mouse_to_action(mouse_event: MouseEvent, app: &App) -> Action {
    match mouse_event.kind {
        MouseEventKind::ScrollUp => {
            return match app.dialog_input().map(|dialog| dialog.kind) {
                Some(DialogKind::Help) => help_scroll_action(-1),
                Some(_) => Action::Noop,
                None => Action::MovePolicyCursor(-1),
            };
        }
        MouseEventKind::ScrollDown => {
            return match app.dialog_input().map(|dialog| dialog.kind) {
                Some(DialogKind::Help) => help_scroll_action(1),
                Some(_) => Action::Noop,
                None => Action::MovePolicyCursor(1),
            };
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return Action::Noop,
    }

    if let Some(dialog) = app.dialog_input() {
        return dialog_button_at(dialog, mouse_event.column, mouse_event.row)
            .map(dialog_button_click_action)
            .unwrap_or(Action::Noop);
    }

    let Some(area) = terminal_area() else {
        return Action::Noop;
    };
    if ui_footer::report_issue_hit(area, app, mouse_event.column, mouse_event.row) {
        Action::OpenReportIssue
    } else {
        Action::Noop
    }
}

fn dialog_button_click_action(button: DialogButtonHit) -> Action {
    if button.hit == ButtonHit::SECONDARY {
        return Action::CloseDialog;
    }

    if button.hit != ButtonHit::PRIMARY {
        return Action::Noop;
    }

    match button.dialog_kind {
        DialogKind::Help => Action::Noop,
        DialogKind::ExportFile => Action::LocateExportFile,
        DialogKind::ConfirmApply => Action::ConfirmApply,
        DialogKind::ConfirmQuit => Action::ConfirmQuit,
        DialogKind::ConfirmUninstall => Action::ConfirmUninstall,
        DialogKind::ConfirmRevert => Action::ConfirmRevert,
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        DialogKind::ElevatedPermissionsRequired => Action::CloseDialog,
    }
}

fn dialog_button_at(dialog: DialogInput, column: u16, row: u16) -> Option<DialogButtonHit> {
    let area = terminal_area()?;
    let hit = match dialog.kind {
        DialogKind::Help => None,
        DialogKind::ExportFile => ui_export::button_hit(dialog.primary_enabled, area, column, row),
        DialogKind::ConfirmApply => ui_apply::button_hit(dialog.primary_enabled, area, column, row),
        DialogKind::ConfirmRevert => {
            ui_revert::button_hit(dialog.primary_enabled, area, column, row)
        }
        DialogKind::ConfirmQuit => ui_quit::button_hit(area, column, row),
        DialogKind::ConfirmUninstall => ui_uninstall::button_hit(area, column, row),
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        DialogKind::ElevatedPermissionsRequired => ui_elevation::button_hit(area, column, row),
    };

    hit.map(|hit| DialogButtonHit {
        dialog_kind: dialog.kind,
        hit,
    })
}

fn terminal_area() -> Option<Rect> {
    let (width, height) = terminal::size().ok()?;

    Some(Rect::new(0, 0, width, height))
}

#[derive(Debug, Clone, Copy)]
struct DialogButtonHit {
    dialog_kind: DialogKind,
    hit: ButtonHit,
}

fn help_key_to_action(key_event: KeyEvent) -> Action {
    match key_event.code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => Action::CloseDialog,
        KeyCode::Char('j') | KeyCode::Down => help_scroll_action(1),
        KeyCode::Char('k') | KeyCode::Up => help_scroll_action(-1),
        KeyCode::End => help_scroll_action(i16::MAX),
        KeyCode::Home => help_scroll_action(i16::MIN),
        KeyCode::PageDown => help_scroll_action(6),
        KeyCode::PageUp => help_scroll_action(-6),
        _ => Action::Noop,
    }
}

fn help_scroll_action(delta: i16) -> Action {
    let max_scroll = terminal_area().map(ui_help::max_scroll).unwrap_or_default();

    Action::ScrollHelp(delta, max_scroll)
}
