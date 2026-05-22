use std::path::PathBuf;

use anyhow::Result;

use crate::browser::{ApplyResult, BrowserState};
use crate::chromium::{Browser, detection, policy};
use crate::editor::{NewPolicyType, PolicyEditorState, PolicyKeyEditorState};
use crate::manifest::Manifest;
use crate::policy_tree::{EditablePolicyValue, PolicyTree, PolicyTreeRowKind, RowId};
use crate::tui::action::Action;
use crate::tui::event::{DialogInput, PolicyInputMode};

pub(crate) const REPORT_ISSUE_URL: &str = env!("CARGO_PKG_REPOSITORY");

#[derive(Debug)]
pub struct App {
    manifest: Manifest,
    browsers: [BrowserState; 3],
    active_browser: Browser,
    tui: TuiState,
    policy_tree_cache: Option<PolicyTreeCache>,
    visible_policy_cache: Option<VisiblePolicyCache>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    system_policy_requires_elevation: bool,
    should_quit: bool,
}

#[derive(Debug)]
struct PolicyTreeCache {
    browser: Browser,
    version: u64,
    tree: PolicyTree,
}

#[derive(Debug, Default)]
pub(crate) struct VisiblePolicyRows {
    indices: Vec<usize>,
    ids: Vec<RowId>,
}

#[derive(Debug)]
struct VisiblePolicyCache {
    browser: Browser,
    version: u64,
    query: String,
    rows: VisiblePolicyRows,
}

#[derive(Debug, Clone)]
struct CursorAnchor {
    cursor: Option<RowId>,
    visible_index: usize,
}

#[derive(Debug, Default)]
pub struct TuiState {
    policy_cursor: Option<RowId>,
    dialog: Option<DialogState>,
    policy_editor: Option<PolicyEditorState>,
    policy_key_editor: Option<PolicyKeyEditorState>,
    filter: FilterState,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct FilterState {
    pub(crate) query: String,
    editing: bool,
}

impl FilterState {
    pub(crate) const fn editing(&self) -> bool {
        self.editing
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogState {
    pub kind: DialogKind,
    pub status: Option<String>,
    pub export_path: Option<PathBuf>,
    pub focused_button: usize,
    pub scroll: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Help,
    ConfirmApply,
    ExportFile,
    ConfirmQuit,
    ConfirmUninstall,
    ConfirmRevert,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    ElevatedPermissionsRequired,
}

impl App {
    pub fn new() -> Result<Self> {
        let manifest = Manifest::load()?;
        let mut browsers = Browser::all().map(|browser| {
            BrowserState::new(
                browser,
                detection::detect(browser),
                policy::read(browser),
                manifest.balanced_preset(browser),
            )
        });
        browsers.sort_by_key(|b| (!b.detected(), !b.has_policy(), b.browser.name()));
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let system_policy_requires_elevation = system_policy_requires_elevation();

        let active_browser = browsers[0].browser;
        let mut app = Self {
            manifest,
            browsers,
            active_browser,
            tui: TuiState::default(),
            policy_tree_cache: None,
            visible_policy_cache: None,
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            system_policy_requires_elevation,
            should_quit: false,
        };
        app.move_policy_cursor_to_start();
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        app.open_elevation_dialog_if_needed();

        Ok(app)
    }

    pub const fn browsers(&self) -> &[BrowserState; 3] {
        &self.browsers
    }

    pub fn active_browser_index(&self) -> usize {
        self.browsers
            .iter()
            .position(|state| state.browser == self.active_browser)
            .unwrap_or(0)
    }

    pub const fn active_browser(&self) -> Browser {
        self.active_browser
    }

    pub fn active_browser_state(&self) -> &BrowserState {
        &self.browsers[self.active_browser_index()]
    }

    pub(crate) fn prepare_policy_view(&mut self) {
        self.prepare_policy_tree();
        self.prepare_visible_policy_rows();
    }

    fn prepare_policy_tree(&mut self) {
        let browser = self.active_browser;
        let version = self.active_browser_state().policy_tree_version();
        if self
            .policy_tree_cache
            .as_ref()
            .is_some_and(|cache| cache.browser == browser && cache.version == version)
        {
            return;
        }

        let tree = {
            let state = self.active_browser_state();
            let Some((baseline, current)) = state.policy_sets() else {
                self.policy_tree_cache = None;
                self.visible_policy_cache = None;
                return;
            };

            state.policy_tree(&self.manifest, baseline, current)
        };

        self.policy_tree_cache = Some(PolicyTreeCache {
            browser,
            version,
            tree,
        });
        self.visible_policy_cache = None;
    }

    fn prepare_visible_policy_rows(&mut self) {
        let browser = self.active_browser;
        let version = self.active_browser_state().policy_tree_version();
        let query = self.tui.filter.query.clone();

        if self.visible_policy_cache.as_ref().is_some_and(|cache| {
            cache.browser == browser && cache.version == version && cache.query == query
        }) {
            return;
        }

        let Some(tree) = self.active_policy_tree() else {
            self.visible_policy_cache = Some(VisiblePolicyCache {
                browser,
                version,
                query,
                rows: VisiblePolicyRows::default(),
            });
            return;
        };

        let (indices, ids) = {
            let indices = tree.visible_indices(&query);
            let ids = indices
                .iter()
                .filter_map(|index| tree.rows().get(*index).map(|row| row.id().clone()))
                .collect();

            (indices, ids)
        };
        let rows = VisiblePolicyRows { indices, ids };

        self.visible_policy_cache = Some(VisiblePolicyCache {
            browser,
            version,
            query,
            rows,
        });
    }

    pub(crate) fn active_policy_tree(&self) -> Option<&PolicyTree> {
        let browser = self.active_browser;
        let version = self.active_browser_state().policy_tree_version();

        self.policy_tree_cache
            .as_ref()
            .filter(|cache| cache.browser == browser && cache.version == version)
            .map(|cache| &cache.tree)
    }

    pub(crate) fn visible_policy_indices(&self) -> &[usize] {
        self.cached_visible_policy_rows()
            .map_or(&[], |rows| rows.indices.as_slice())
    }

    fn visible_policy_row_ids(&self) -> &[RowId] {
        self.cached_visible_policy_rows()
            .map_or(&[], |rows| rows.ids.as_slice())
    }

    fn cached_visible_policy_rows(&self) -> Option<&VisiblePolicyRows> {
        let browser = self.active_browser;
        let version = self.active_browser_state().policy_tree_version();

        self.visible_policy_cache
            .as_ref()
            .filter(|cache| {
                cache.browser == browser
                    && cache.version == version
                    && cache.query == self.tui.filter.query
            })
            .map(|cache| &cache.rows)
    }

    fn active_browser_state_mut(&mut self) -> &mut BrowserState {
        let index = self.active_browser_index();
        &mut self.browsers[index]
    }

    pub fn help_scroll(&self) -> u16 {
        self.tui
            .dialog
            .as_ref()
            .filter(|dialog| dialog.kind == DialogKind::Help)
            .map_or(0, |dialog| dialog.scroll)
    }

    pub const fn policy_cursor(&self) -> Option<&RowId> {
        self.tui.policy_cursor.as_ref()
    }

    pub const fn dialog(&self) -> Option<&DialogState> {
        self.tui.dialog.as_ref()
    }

    pub(crate) const fn policy_editor(&self) -> Option<&PolicyEditorState> {
        self.tui.policy_editor.as_ref()
    }

    pub(crate) const fn policy_key_editor(&self) -> Option<&PolicyKeyEditorState> {
        self.tui.policy_key_editor.as_ref()
    }

    pub(crate) const fn editing_policy(&self) -> bool {
        self.tui.policy_editor.is_some() || self.tui.policy_key_editor.is_some()
    }

    pub(crate) const fn filter(&self) -> &FilterState {
        &self.tui.filter
    }

    pub(crate) fn filter_visible(&self) -> bool {
        self.tui.filter.editing || !self.tui.filter.query.is_empty()
    }

    pub(crate) const fn filter_input_active(&self) -> bool {
        self.tui.filter.editing
    }

    pub(crate) const fn input_active(&self) -> bool {
        self.editing_policy() || self.filter_input_active()
    }

    pub(crate) const fn policy_input_mode(&self) -> Option<PolicyInputMode> {
        if self.tui.policy_key_editor.is_some() {
            Some(PolicyInputMode::Key)
        } else if self.tui.filter.editing {
            Some(PolicyInputMode::Filter)
        } else if self.tui.policy_editor.is_some() {
            Some(PolicyInputMode::Edit)
        } else {
            None
        }
    }

    pub(crate) fn new_policy_shortcut_label(&self) -> Option<&'static str> {
        const ADD_LIST_ITEM: &str = "insert value";
        const NEW_KEY: &str = "new key";
        let new_key = self.tui.filter.query.is_empty().then_some(NEW_KEY);

        let Some(tree) = self.active_policy_tree() else {
            return new_key;
        };
        let Some(cursor) = &self.tui.policy_cursor else {
            return new_key;
        };
        let Some(row) = tree.row(cursor) else {
            return new_key;
        };

        match &row.kind {
            PolicyTreeRowKind::Policy { value, .. } if value.is_list() => Some(ADD_LIST_ITEM),
            PolicyTreeRowKind::Value { .. } => Some(ADD_LIST_ITEM),
            PolicyTreeRowKind::Group { .. } | PolicyTreeRowKind::Policy { .. } => new_key,
        }
    }

    pub fn dialog_input(&self) -> Option<DialogInput> {
        let dialog = self.tui.dialog.as_ref()?;

        Some(DialogInput {
            kind: dialog.kind,
            primary_enabled: self.dialog_primary_enabled(dialog.kind),
        })
    }

    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    const fn system_policy_requires_elevation(&self) -> bool {
        self.system_policy_requires_elevation
    }

    pub fn handle_action(&mut self, action: Action) -> bool {
        match action {
            Action::BackspaceFilter => self.backspace_filter(),
            Action::CloseDialog => self.close_dialog(),
            Action::BackspacePolicyEdit => self.backspace_policy_edit(),
            Action::BeginFilter => self.begin_filter(),
            Action::BeginPolicyEdit => self.begin_policy_edit(),
            Action::CancelFilter => self.cancel_filter(),
            Action::CancelPolicyEdit => self.cancel_policy_edit(),
            Action::CommitFilter => self.commit_filter(),
            Action::CommitPolicyEdit => self.commit_policy_edit(),
            Action::ConfirmApply => self.confirm_apply(),
            Action::ConfirmQuit => self.confirm_quit(),
            Action::ConfirmRevert => self.confirm_revert(),
            Action::ConfirmUninstall => self.confirm_uninstall(),
            Action::InputFilter(character) => self.input_filter(character),
            Action::InputPolicyEdit(character) => self.input_policy_edit(character),
            Action::LocateExportFile => self.locate_export_file(),
            Action::ActivateDialogButton => self.activate_dialog_button(),
            Action::MoveDialogFocus(delta) => self.move_dialog_focus(delta),
            Action::OpenApplyDialog => self.open_apply_dialog(),
            Action::OpenExportDialog => self.open_export_dialog(),
            Action::OpenReportIssue => self.open_report_issue(),
            Action::OpenRevertDialog => self.open_revert_dialog(),
            Action::OpenUninstallDialog => self.open_uninstall_dialog(),
            Action::Paste(text) => self.paste(text),
            Action::MovePolicyCursor(delta) => self.move_policy_cursor(delta),
            Action::MovePolicyGroup(delta) => self.move_policy_group(delta),
            Action::MovePolicyType(delta) => self.move_policy_type(delta),
            Action::NewPolicyItem => self.new_policy_item(),
            Action::Noop => false,
            Action::PolicyCursorEnd => self.move_policy_cursor_to_end(),
            Action::PolicyCursorStart => self.move_policy_cursor_to_start(),
            Action::Quit => self.quit(),
            Action::Redraw => true,
            Action::Redo => self.redo(),
            Action::ScrollHelp(delta, max_scroll) => self.scroll_help(delta, max_scroll),
            Action::SelectTab(index) => self.select_browser_at(index),
            Action::StagePolicyRemoval => self.stage_policy_removal(),
            Action::Tick => self.refresh_awaiting_installs(),
            Action::ToggleHelp => self.toggle_help(),
            Action::TogglePolicyPresence => self.toggle_policy_presence(),
            Action::Undo => self.undo(),
        }
    }

    fn select_browser_at(&mut self, index: usize) -> bool {
        if index >= self.browsers.len() {
            return false;
        }

        let previous_browser = self.active_browser;
        self.active_browser = self.browsers[index].browser;
        let editors_changed = self.clear_policy_editors();
        let cursor_changed = self.move_policy_cursor_to_start();

        self.active_browser != previous_browser || cursor_changed || editors_changed
    }

    fn scroll_help(&mut self, delta: i16, max_scroll: u16) -> bool {
        let Some(dialog) = &mut self.tui.dialog else {
            return false;
        };
        if dialog.kind != DialogKind::Help {
            return false;
        }

        let current_scroll = i32::from(dialog.scroll);
        let requested_scroll = current_scroll + i32::from(delta);
        let max_scroll = i32::from(max_scroll);
        let next_scroll = requested_scroll.clamp(0, max_scroll) as u16;

        if dialog.scroll == next_scroll {
            return false;
        }

        dialog.scroll = next_scroll;
        true
    }

    fn close_dialog(&mut self) -> bool {
        let changed = self.tui.dialog.is_some();

        self.tui.dialog = None;

        changed
    }

    fn move_policy_cursor(&mut self, delta: i16) -> bool {
        self.prepare_policy_view();
        let Some(next_cursor) = offset_cursor(
            self.visible_policy_row_ids(),
            self.tui.policy_cursor.as_ref(),
            delta,
        ) else {
            return false;
        };

        if self.tui.policy_cursor.as_ref() == Some(&next_cursor) {
            return false;
        }

        self.tui.policy_cursor = Some(next_cursor);
        self.clear_policy_editors();
        true
    }

    fn move_policy_cursor_to_start(&mut self) -> bool {
        self.prepare_policy_view();
        let next_cursor = self.visible_policy_row_ids().first().cloned();
        if self.tui.policy_cursor == next_cursor {
            return false;
        }

        self.tui.policy_cursor = next_cursor;
        self.clear_policy_editors();
        true
    }

    fn move_policy_cursor_to_end(&mut self) -> bool {
        self.prepare_policy_view();
        let next_cursor = self.visible_policy_row_ids().last().cloned();
        if self.tui.policy_cursor == next_cursor {
            return false;
        }

        self.tui.policy_cursor = next_cursor;
        self.clear_policy_editors();
        true
    }

    fn policy_cursor_anchor(&mut self) -> CursorAnchor {
        self.prepare_policy_view();
        let cursor = self.tui.policy_cursor.clone();
        let visible_index = cursor
            .as_ref()
            .and_then(|cursor| {
                self.visible_policy_row_ids()
                    .iter()
                    .position(|visible_cursor| visible_cursor == cursor)
            })
            .unwrap_or_default();

        CursorAnchor {
            cursor,
            visible_index,
        }
    }

    fn sync_policy_cursor_to_filter(&mut self) -> bool {
        self.prepare_policy_view();
        let Some(next_cursor) = nearest_cursor(
            self.visible_policy_row_ids(),
            self.tui.policy_cursor.as_ref(),
        ) else {
            let changed = self.tui.policy_cursor.is_some();
            self.tui.policy_cursor = None;
            return changed;
        };
        if self.tui.policy_cursor.as_ref() == Some(&next_cursor) {
            return false;
        }

        self.tui.policy_cursor = Some(next_cursor);
        true
    }

    fn sync_policy_cursor_to_anchor(&mut self, anchor: CursorAnchor) -> bool {
        self.prepare_policy_view();
        let next_cursor = anchored_cursor(
            self.visible_policy_row_ids(),
            anchor.cursor.as_ref(),
            anchor.visible_index,
        );
        if self.tui.policy_cursor == next_cursor {
            return false;
        }

        self.tui.policy_cursor = next_cursor;
        true
    }

    fn sync_policy_cursor_to_first_filter_match(&mut self) -> bool {
        self.prepare_policy_view();
        let Some(next_cursor) = self.visible_policy_row_ids().first().cloned() else {
            let changed = self.tui.policy_cursor.is_some();
            self.tui.policy_cursor = None;
            return changed;
        };
        if self.tui.policy_cursor.as_ref() == Some(&next_cursor) {
            return false;
        }

        self.tui.policy_cursor = Some(next_cursor);
        true
    }

    fn policy_cursor_is_visible(&mut self, cursor: &RowId) -> bool {
        self.prepare_policy_view();
        self.visible_policy_row_ids()
            .iter()
            .any(|visible_cursor| visible_cursor == cursor)
    }

    fn sync_policy_cursor_to_filter_input(&mut self) -> bool {
        if self.tui.filter.query.is_empty() {
            self.sync_policy_cursor_to_filter()
        } else {
            self.sync_policy_cursor_to_first_filter_match()
        }
    }

    fn clear_policy_editors(&mut self) -> bool {
        let changed = self.editing_policy();
        self.tui.policy_editor = None;
        self.tui.policy_key_editor = None;
        changed
    }

    fn has_pending_changes(&self) -> bool {
        self.browsers
            .iter()
            .any(BrowserState::has_user_pending_changes)
    }

    fn quit(&mut self) -> bool {
        if self.has_pending_changes() {
            return self.open_quit_dialog();
        }

        let changed = !self.should_quit;
        self.should_quit = true;
        changed
    }

    fn begin_filter(&mut self) -> bool {
        let editors_changed = self.clear_policy_editors();
        let changed = !self.tui.filter.editing;

        self.tui.filter.editing = true;

        changed || editors_changed
    }

    fn input_filter(&mut self, character: char) -> bool {
        if character.is_control() {
            return false;
        }

        self.tui.filter.query.push(character);
        self.sync_policy_cursor_to_filter_input();
        true
    }

    fn paste(&mut self, text: String) -> bool {
        if self.tui.filter.editing {
            return self.paste_filter(&text);
        }

        self.paste_policy_edit(&text)
    }

    fn paste_filter(&mut self, text: &str) -> bool {
        let previous_len = self.tui.filter.query.len();
        self.tui
            .filter
            .query
            .extend(text.chars().filter(|character| !character.is_control()));
        let changed = self.tui.filter.query.len() != previous_len;
        if changed {
            self.sync_policy_cursor_to_filter_input();
        }

        changed
    }

    fn paste_policy_edit(&mut self, text: &str) -> bool {
        if let Some(editor) = &mut self.tui.policy_key_editor {
            let previous_len = editor.buffer.len();
            editor.buffer.extend(
                text.chars()
                    .filter(|character| PolicyKeyEditorState::accepts(*character)),
            );
            let changed = editor.buffer.len() != previous_len || editor.invalid;
            editor.invalid = false;
            return changed;
        }

        let Some(editor) = &mut self.tui.policy_editor else {
            return false;
        };
        let mut changed = editor.invalid;
        editor.invalid = false;
        for character in text.chars() {
            if editor.accepts(character) {
                editor.buffer.push(character);
                changed = true;
            }
        }

        changed
    }

    fn backspace_filter(&mut self) -> bool {
        let changed = self.tui.filter.query.pop().is_some();
        if changed {
            self.sync_policy_cursor_to_filter_input();
        }

        changed
    }

    fn commit_filter(&mut self) -> bool {
        if !self.tui.filter.editing {
            return false;
        }

        self.tui.filter.editing = false;
        true
    }

    fn cancel_filter(&mut self) -> bool {
        let changed = self.tui.filter.editing || !self.tui.filter.query.is_empty();
        self.tui.filter = FilterState::default();
        self.sync_policy_cursor_to_filter();

        changed
    }

    fn stage_policy_removal(&mut self) -> bool {
        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        let index = self.active_browser_index();
        let anchor = self.policy_cursor_anchor();

        if self.browsers[index].stage_policy_removal_at(&cursor) {
            self.sync_policy_cursor_to_anchor(anchor);
            self.clear_policy_editors();
            return true;
        }

        false
    }

    fn toggle_policy_presence(&mut self) -> bool {
        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        let index = self.active_browser_index();
        let anchor = self.policy_cursor_anchor();

        if self.tui.filter.query.is_empty()
            && self.browsers[index].toggle_policy_group_at(&self.manifest, &cursor)
        {
            self.sync_policy_cursor_to_anchor(anchor);
            self.clear_policy_editors();
            return true;
        }

        if self.browsers[index].toggle_policy_at(&self.manifest, &cursor) {
            self.sync_policy_cursor_to_anchor(anchor);
            self.clear_policy_editors();
            return true;
        }

        false
    }

    fn begin_policy_edit(&mut self) -> bool {
        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        let index = self.active_browser_index();
        let anchor = self.policy_cursor_anchor();

        if self.browsers[index].toggle_policy_bool_at(&self.manifest, &cursor) {
            self.sync_policy_cursor_to_anchor(anchor);
            self.clear_policy_editors();
            return true;
        }

        let Some(edit) = self.editable_policy_value() else {
            return false;
        };
        let next_editor = PolicyEditorState::new(cursor, edit);
        let changed = self.tui.policy_editor.as_ref() != Some(&next_editor);

        self.tui.policy_key_editor = None;
        self.tui.policy_editor = Some(next_editor);
        changed
    }

    fn editable_policy_value(&self) -> Option<EditablePolicyValue> {
        let cursor = self.tui.policy_cursor.as_ref()?;

        self.active_browser_state()
            .editable_policy_value_at(&self.manifest, cursor)
    }

    fn input_policy_edit(&mut self, character: char) -> bool {
        if let Some(editor) = &mut self.tui.policy_key_editor {
            if !PolicyKeyEditorState::accepts(character) {
                return false;
            }

            editor.buffer.push(character);
            editor.invalid = false;
            return true;
        }

        let Some(editor) = &mut self.tui.policy_editor else {
            return false;
        };
        if !editor.accepts(character) {
            return false;
        }

        editor.buffer.push(character);
        editor.invalid = false;
        true
    }

    fn backspace_policy_edit(&mut self) -> bool {
        if let Some(editor) = &mut self.tui.policy_key_editor {
            let changed = editor.buffer.pop().is_some() || editor.invalid;
            editor.invalid = false;
            return changed;
        }

        let Some(editor) = &mut self.tui.policy_editor else {
            return false;
        };

        let changed = editor.buffer.pop().is_some() || editor.invalid;
        editor.invalid = false;

        changed
    }

    fn cancel_policy_edit(&mut self) -> bool {
        self.clear_policy_editors()
    }

    fn commit_policy_edit(&mut self) -> bool {
        if self.tui.policy_key_editor.is_some() {
            return self.commit_policy_key_edit();
        }

        let Some(editor) = self.tui.policy_editor.clone() else {
            return false;
        };
        let Some(value) = editor.policy_value() else {
            if let Some(editor) = &mut self.tui.policy_editor {
                editor.invalid = true;
            }
            return true;
        };

        let index = self.active_browser_index();
        if let Some(target) = editor.new_list_item() {
            let Some(cursor) = self.browsers[index].add_list_item_value_at(
                &self.manifest,
                &target.source_cursor,
                value,
            ) else {
                return false;
            };
            self.tui.policy_cursor = Some(cursor);
            self.clear_policy_editors();
            return true;
        }

        let Some(cursor) = editor.existing_cursor() else {
            return false;
        };
        let anchor = self.policy_cursor_anchor();
        if self.browsers[index].set_policy_value_at(cursor, value) {
            self.sync_policy_cursor_to_anchor(anchor);
        }
        self.clear_policy_editors();
        true
    }

    fn commit_policy_key_edit(&mut self) -> bool {
        let Some(editor) = self.tui.policy_key_editor.clone() else {
            return false;
        };
        let Some(key) = editor.key() else {
            if let Some(editor) = &mut self.tui.policy_key_editor {
                editor.invalid = true;
            }
            return true;
        };
        if self.manifest.has_policy_key(self.active_browser, &key) {
            if let Some(editor) = &mut self.tui.policy_key_editor {
                editor.invalid = true;
            }
            return true;
        }

        let policy_type = editor.selected_type();
        let index = self.active_browser_index();
        if !self.browsers[index].add_policy_key(key.clone(), policy_type.initial_value()) {
            if let Some(editor) = &mut self.tui.policy_key_editor {
                editor.invalid = true;
            }
            return true;
        }

        let cursor = self.browsers[index]
            .policy_key_cursor(&self.manifest, &key)
            .or_else(|| self.tui.policy_cursor.clone());
        self.tui.policy_cursor = cursor.clone();
        self.tui.policy_key_editor = None;
        self.tui.policy_editor = match (policy_type, cursor) {
            (NewPolicyType::String, Some(cursor)) => {
                Some(PolicyEditorState::string(cursor, String::new()))
            }
            (NewPolicyType::Integer, Some(cursor)) => Some(PolicyEditorState::integer(cursor)),
            (NewPolicyType::Bool | NewPolicyType::List, _) | (_, None) => None,
        };
        true
    }

    fn move_policy_type(&mut self, delta: i16) -> bool {
        let Some(editor) = &mut self.tui.policy_key_editor else {
            return false;
        };

        editor.move_selection(delta)
    }

    fn move_policy_group(&mut self, delta: i16) -> bool {
        if !self.tui.filter.query.is_empty() {
            return self.move_filtered_policy_group(delta);
        }

        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        self.prepare_policy_view();
        let next_cursor = {
            let Some(tree) = self.active_policy_tree() else {
                return false;
            };
            let Some(next_cursor) = tree.group_cursor(&cursor, delta) else {
                return false;
            };

            next_cursor
        };

        if self.tui.policy_cursor.as_ref() == Some(&next_cursor) {
            return false;
        }

        self.tui.policy_cursor = Some(next_cursor);
        self.clear_policy_editors();
        true
    }

    fn move_filtered_policy_group(&mut self, delta: i16) -> bool {
        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        self.prepare_policy_view();
        let next_cursor = {
            let Some(tree) = self.active_policy_tree() else {
                return false;
            };
            let Some(next_cursor) =
                tree.filtered_group_cursor(&self.tui.filter.query, &cursor, delta)
            else {
                return false;
            };

            next_cursor
        };

        if self.tui.policy_cursor.as_ref() == Some(&next_cursor) {
            return false;
        }

        self.tui.policy_cursor = Some(next_cursor);
        self.clear_policy_editors();
        true
    }

    fn new_policy_item(&mut self) -> bool {
        if self.active_browser_state().policy_sets().is_none() {
            return false;
        }

        let Some(cursor) = self.tui.policy_cursor.clone() else {
            return false;
        };
        if !self.policy_cursor_is_visible(&cursor) {
            return false;
        }

        let index = self.active_browser_index();
        if let Some(target) = self.browsers[index].new_list_item_target_at(&self.manifest, &cursor)
        {
            self.tui.policy_cursor = Some(target.insert_after.clone());
            self.tui.policy_key_editor = None;
            self.tui.policy_editor = Some(PolicyEditorState::list_item(
                cursor,
                target.insert_after,
                target.indent,
            ));
            return true;
        }
        if !self.tui.filter.query.is_empty() {
            return false;
        }

        let next_editor = PolicyKeyEditorState::default();
        let changed = self.tui.policy_key_editor.as_ref() != Some(&next_editor)
            || self.tui.policy_editor.is_some();
        self.tui.policy_editor = None;
        self.tui.policy_key_editor = Some(next_editor);

        changed
    }

    fn open_uninstall_dialog(&mut self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.system_policy_requires_elevation() {
            return self.open_dialog(DialogKind::ElevatedPermissionsRequired);
        }

        self.open_dialog(DialogKind::ConfirmUninstall)
    }

    fn activate_dialog_button(&mut self) -> bool {
        let Some((kind, focused_button)) = self
            .tui
            .dialog
            .as_ref()
            .map(|dialog| (dialog.kind, dialog.focused_button))
        else {
            return false;
        };

        if focused_button == 0 && !self.dialog_primary_enabled(kind) {
            return self.close_dialog();
        }

        match (kind, focused_button) {
            (DialogKind::Help, _) => self.close_dialog(),
            (DialogKind::ConfirmApply, 0) => self.confirm_apply(),
            (DialogKind::ExportFile, 0) => self.locate_export_file(),
            (DialogKind::ConfirmQuit, 0) => self.confirm_quit(),
            (DialogKind::ConfirmUninstall, 0) => self.confirm_uninstall(),
            (DialogKind::ConfirmRevert, 0) => self.confirm_revert(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            (DialogKind::ElevatedPermissionsRequired, 0) => self.close_dialog(),
            (_, _) => self.close_dialog(),
        }
    }

    fn move_dialog_focus(&mut self, delta: i16) -> bool {
        let Some(kind) = self.tui.dialog.as_ref().map(|dialog| dialog.kind) else {
            return false;
        };
        let button_count = self.dialog_button_count(kind);
        if button_count < 2 {
            return false;
        }

        let Some(dialog) = &mut self.tui.dialog else {
            return false;
        };

        let current = dialog.focused_button as i32;
        let next = (current + i32::from(delta)).rem_euclid(button_count as i32);
        if dialog.focused_button == next as usize {
            return false;
        }

        dialog.focused_button = next as usize;
        true
    }

    fn dialog_button_count(&self, kind: DialogKind) -> usize {
        if kind == DialogKind::Help {
            return 0;
        }

        if self.dialog_primary_enabled(kind) {
            2
        } else {
            1
        }
    }

    fn dialog_primary_enabled(&self, kind: DialogKind) -> bool {
        match kind {
            DialogKind::Help => false,
            DialogKind::ExportFile => self
                .tui
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.export_path.is_some()),
            DialogKind::ConfirmQuit => self.has_pending_changes(),
            DialogKind::ConfirmApply | DialogKind::ConfirmRevert => {
                self.active_browser_state().is_dirty()
            }
            DialogKind::ConfirmUninstall => true,
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            DialogKind::ElevatedPermissionsRequired => false,
        }
    }

    fn open_apply_dialog(&mut self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.active_browser_state().is_dirty() && self.system_policy_requires_elevation() {
            return self.open_dialog(DialogKind::ElevatedPermissionsRequired);
        }

        self.open_dialog(DialogKind::ConfirmApply)
    }

    fn open_export_dialog(&mut self) -> bool {
        let path = PathBuf::from(self.default_export_path());
        let result = self.active_browser_state().export_policy_file(&path);
        let (status, export_path) = match result {
            Ok(_) => (None, Some(path)),
            Err(error) => (Some(error), None),
        };
        self.clear_policy_editors();
        self.tui.dialog = Some(DialogState {
            kind: DialogKind::ExportFile,
            status,
            export_path,
            focused_button: 0,
            scroll: 0,
        });
        true
    }

    fn locate_export_file(&mut self) -> bool {
        let Some(path) = self
            .tui
            .dialog
            .as_ref()
            .filter(|dialog| dialog.kind == DialogKind::ExportFile)
            .and_then(|dialog| dialog.export_path.clone())
        else {
            return false;
        };

        match crate::opener::locate_file(&path) {
            Ok(()) => true,
            Err(error) => {
                if let Some(dialog) = &mut self.tui.dialog {
                    dialog.status = Some(format!("Could not locate file: {error}"));
                }
                true
            }
        }
    }

    fn default_export_path(&self) -> String {
        let file_name = policy::export_file_name(self.active_browser);

        home_dir()
            .map(|directory| directory.join(&file_name).display().to_string())
            .unwrap_or(file_name)
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    fn open_elevation_dialog_if_needed(&mut self) -> bool {
        if !self.system_policy_requires_elevation() {
            return false;
        }

        self.open_dialog(DialogKind::ElevatedPermissionsRequired)
    }

    fn open_quit_dialog(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.kind == DialogKind::ConfirmQuit)
        {
            return false;
        }

        self.open_dialog(DialogKind::ConfirmQuit)
    }

    fn open_report_issue(&self) -> bool {
        crate::opener::open_url(REPORT_ISSUE_URL).is_ok()
    }

    fn toggle_help(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.kind == DialogKind::Help)
        {
            return self.close_dialog();
        }

        self.open_dialog(DialogKind::Help)
    }

    fn open_dialog(&mut self, kind: DialogKind) -> bool {
        self.clear_policy_editors();
        self.tui.dialog = Some(DialogState {
            kind,
            status: None,
            export_path: None,
            focused_button: 0,
            scroll: 0,
        });
        true
    }

    fn confirm_apply(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_none_or(|dialog| dialog.kind != DialogKind::ConfirmApply)
        {
            return false;
        }

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.system_policy_requires_elevation() {
            return self.open_dialog(DialogKind::ElevatedPermissionsRequired);
        }

        let anchor = self.policy_cursor_anchor();
        match self.active_browser_state_mut().apply_policy_changes() {
            Ok(ApplyResult::Applied) => {
                self.sync_policy_cursor_to_anchor(anchor);
                self.tui.dialog = None;
            }
            Ok(ApplyResult::AwaitingInstall) => self.tui.dialog = None,
            Ok(ApplyResult::NoChanges) => {
                if let Some(dialog) = &mut self.tui.dialog {
                    dialog.status = Some("No pending changes to apply.".to_owned());
                }
            }
            Err(error) => {
                if let Some(dialog) = &mut self.tui.dialog {
                    dialog.status = Some(error.to_string());
                }
            }
        }

        true
    }

    fn refresh_awaiting_installs(&mut self) -> bool {
        if !self.browsers.iter().any(BrowserState::awaiting_install) {
            return false;
        }

        let anchor = self.policy_cursor_anchor();
        let changed = self
            .browsers
            .iter_mut()
            .any(BrowserState::refresh_awaiting_install);
        if changed {
            self.sync_policy_cursor_to_anchor(anchor);
        }

        changed
    }

    #[cfg(not(target_os = "macos"))]
    fn confirm_uninstall(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_none_or(|dialog| dialog.kind != DialogKind::ConfirmUninstall)
        {
            return false;
        }

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.system_policy_requires_elevation() {
            return self.open_dialog(DialogKind::ElevatedPermissionsRequired);
        }

        match self.active_browser_state_mut().uninstall_policy() {
            Ok(()) => {
                self.move_policy_cursor_to_start();
                self.tui.dialog = None;
                true
            }
            Err(error) => {
                if let Some(dialog) = &mut self.tui.dialog {
                    dialog.status = Some(error);
                }
                true
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn confirm_uninstall(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_none_or(|dialog| dialog.kind != DialogKind::ConfirmUninstall)
        {
            return false;
        }

        match crate::macos::open_profiles_settings() {
            Ok(()) => self.tui.dialog = None,
            Err(error) => {
                if let Some(dialog) = &mut self.tui.dialog {
                    dialog.status = Some(error.to_string());
                }
            }
        }

        true
    }

    fn open_revert_dialog(&mut self) -> bool {
        self.clear_policy_editors();
        self.tui.dialog = Some(DialogState {
            kind: DialogKind::ConfirmRevert,
            status: None,
            export_path: None,
            focused_button: 0,
            scroll: 0,
        });
        true
    }

    fn confirm_revert(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_none_or(|dialog| dialog.kind != DialogKind::ConfirmRevert)
        {
            return false;
        }

        let anchor = self.policy_cursor_anchor();
        if self.active_browser_state_mut().revert() {
            self.sync_policy_cursor_to_anchor(anchor);
        }
        self.tui.dialog = None;
        true
    }

    fn confirm_quit(&mut self) -> bool {
        if self
            .tui
            .dialog
            .as_ref()
            .is_none_or(|dialog| dialog.kind != DialogKind::ConfirmQuit)
        {
            return false;
        }

        let changed = !self.should_quit;
        self.should_quit = true;
        changed
    }

    fn undo(&mut self) -> bool {
        self.clear_policy_editors();
        let anchor = self.policy_cursor_anchor();
        let changed = self.active_browser_state_mut().undo();
        if changed {
            self.sync_policy_cursor_to_anchor(anchor);
        }

        changed
    }

    fn redo(&mut self) -> bool {
        self.clear_policy_editors();
        let anchor = self.policy_cursor_anchor();
        let changed = self.active_browser_state_mut().redo();
        if changed {
            self.sync_policy_cursor_to_anchor(anchor);
        }

        changed
    }
}

fn offset_cursor(cursors: &[RowId], current: Option<&RowId>, delta: i16) -> Option<RowId> {
    if cursors.is_empty() {
        return None;
    }

    let current_position = cursors
        .iter()
        .position(|cursor| Some(cursor) == current)
        .unwrap_or_default();
    let requested_position = current_position as i32 + i32::from(delta);
    let max_position = cursors.len().saturating_sub(1) as i32;
    let next_position = requested_position.clamp(0, max_position) as usize;

    cursors.get(next_position).cloned()
}

fn nearest_cursor(cursors: &[RowId], current: Option<&RowId>) -> Option<RowId> {
    current
        .and_then(|current| cursors.iter().find(|cursor| *cursor == current))
        .or_else(|| cursors.first())
        .cloned()
}

fn anchored_cursor(
    cursors: &[RowId],
    current: Option<&RowId>,
    fallback_index: usize,
) -> Option<RowId> {
    if cursors.is_empty() {
        return None;
    }

    current
        .and_then(|current| cursors.iter().find(|cursor| *cursor == current))
        .or_else(|| cursors.get(fallback_index.min(cursors.len().saturating_sub(1))))
        .cloned()
}

#[cfg(target_os = "windows")]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(target_os = "linux")]
fn home_dir() -> Option<PathBuf> {
    sudo_user_home().or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(target_os = "linux")]
fn sudo_user_home() -> Option<PathBuf> {
    let user = std::env::var("SUDO_USER").ok()?;
    if user.is_empty() || user == "root" {
        return None;
    }

    passwd_home_for_user(&user)
}

#[cfg(target_os = "linux")]
fn passwd_home_for_user(user: &str) -> Option<PathBuf> {
    std::fs::read_to_string("/etc/passwd")
        .ok()?
        .lines()
        .find_map(|line| passwd_line_home(line, user))
}

#[cfg(target_os = "linux")]
fn passwd_line_home(line: &str, user: &str) -> Option<PathBuf> {
    let mut fields = line.split(':');
    if fields.next()? != user {
        return None;
    }

    fields.next()?;
    fields.next()?;
    fields.next()?;
    fields.next()?;
    let home = fields.next()?;

    (!home.is_empty()).then(|| PathBuf::from(home))
}

#[cfg(target_os = "linux")]
fn system_policy_requires_elevation() -> bool {
    linux_requires_sudo()
}

#[cfg(target_os = "windows")]
fn system_policy_requires_elevation() -> bool {
    crate::windows::needs_elevation()
}

#[cfg(target_os = "linux")]
fn linux_requires_sudo() -> bool {
    effective_linux_uid().is_none_or(|uid| uid != 0)
}

#[cfg(target_os = "linux")]
fn effective_linux_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_effective_linux_uid(&status)
}

#[cfg(target_os = "linux")]
fn parse_effective_linux_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|uids| uids.split_whitespace().nth(1))
        .and_then(|uid| uid.parse().ok())
}
