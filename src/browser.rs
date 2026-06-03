use std::path::Path;

use crate::chromium::Browser;
use crate::chromium::detection::{BrowserDetectionResult, BrowserInstall};
use crate::chromium::policy::{self, BrowserPolicy, PolicyReadResult, PolicySet, PolicyValue};
use crate::diff::DiffCounts;
use crate::manifest::Manifest;
use crate::policy_stage::PolicyStage;
use crate::policy_tree::{self, EditablePolicyValue, NewListItemTarget, PolicyTree, RowId};
#[cfg(target_os = "macos")]
use crate::watcher::ManagedPolicyWatcher;

#[derive(Debug)]
pub struct BrowserState {
    pub browser: Browser,
    pub install: Option<BrowserInstall>,
    pub install_error: Option<String>,
    pub policy: Option<BrowserPolicy>,
    pub policy_error: Option<String>,
    managed_policy_exists: bool,
    awaiting_install: bool,
    awaiting_uninstall: bool,
    policy_tree_version: u64,
    #[cfg(target_os = "macos")]
    managed_policy_watcher: Option<ManagedPolicyWatcher>,
    edits: PolicyStage,
    first_missing_current: Option<PolicySet>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    AwaitingInstall,
    NoChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallResult {
    #[cfg(not(target_os = "macos"))]
    Uninstalled,
    #[cfg(target_os = "macos")]
    AwaitingUninstall,
    NoPolicy,
}

impl BrowserState {
    pub fn new(
        browser: Browser,
        install: BrowserDetectionResult,
        policy: PolicyReadResult,
        manifest: &Manifest,
        preset: PolicySet,
    ) -> Self {
        let (install, install_error) = match install {
            Ok(install) => (install, None),
            Err(error) => (None, Some(error.to_string())),
        };
        let (policy, policy_error) = match policy {
            Ok(policy) => (policy, None),
            Err(error) => (None, Some(error.to_string())),
        };
        let managed_policy_exists = policy.is_some();
        let (policy, edits, first_missing_current) =
            match (policy, policy_error.is_none() && !preset.is_empty()) {
                (Some(policy), _) => {
                    let edits = PolicyStage::new(manifest, browser, &policy.policies);
                    (Some(policy), edits, None)
                }
                (None, true) => {
                    let (policy, edits) = missing_policy_with_defaults(manifest, browser, &preset);
                    (Some(policy), edits, Some(preset))
                }
                (None, false) => (
                    None,
                    PolicyStage::new(manifest, browser, &PolicySet::new()),
                    None,
                ),
            };

        Self {
            browser,
            install,
            install_error,
            policy,
            policy_error,
            managed_policy_exists,
            awaiting_install: false,
            awaiting_uninstall: false,
            policy_tree_version: 0,
            #[cfg(target_os = "macos")]
            managed_policy_watcher: None,
            edits,
            first_missing_current,
        }
    }

    pub const fn detected(&self) -> bool {
        self.install.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        !self.diff_counts().is_empty()
    }

    pub fn has_user_pending_changes(&self) -> bool {
        if self.managed_policy_exists {
            return self.is_dirty();
        }

        self.is_dirty()
            && self
                .first_missing_current
                .as_ref()
                .is_some_and(|first| self.edits.materialize() != *first)
    }

    pub fn awaiting_install(&self) -> bool {
        self.awaiting_install && self.is_dirty()
    }

    pub const fn awaiting_uninstall(&self) -> bool {
        self.awaiting_uninstall
    }

    pub const fn awaiting_policy_change(&self) -> bool {
        self.awaiting_install || self.awaiting_uninstall
    }

    pub fn has_policy(&self) -> bool {
        self.policy.as_ref().is_some_and(|policy| {
            !policy.policies.is_empty() || !self.edits.materialize().is_empty()
        })
    }

    pub const fn managed_policy_exists(&self) -> bool {
        self.managed_policy_exists
    }

    pub const fn policy_tree_version(&self) -> u64 {
        self.policy_tree_version
    }

    pub fn diff_counts(&self) -> DiffCounts {
        self.edits.diff_counts()
    }

    pub fn policy_tree(&self, manifest: &Manifest) -> Option<PolicyTree> {
        self.policy.as_ref()?;

        Some(PolicyTree::build(
            manifest,
            self.browser,
            &self.edits,
            self.first_missing_current.as_ref(),
        ))
    }

    pub fn undo(&mut self) -> bool {
        if self.policy.is_none() {
            return false;
        }
        let changed = self.edits.undo();
        if changed {
            self.bump_policy_tree_version();
            self.clear_awaiting_policy_change();
        }

        changed
    }

    pub fn redo(&mut self) -> bool {
        if self.policy.is_none() {
            return false;
        }
        let changed = self.edits.redo();
        if changed {
            self.bump_policy_tree_version();
            self.clear_awaiting_policy_change();
        }

        changed
    }

    pub fn revert(&mut self) -> bool {
        let changed = self.edits.revert() || self.awaiting_install || self.awaiting_uninstall;
        if changed {
            self.bump_policy_tree_version();
        }
        self.clear_awaiting_policy_change();

        changed
    }

    pub fn apply_policy_changes(&mut self, manifest: &Manifest) -> Result<ApplyResult, String> {
        let Some(policy) = &self.policy else {
            return Ok(ApplyResult::NoChanges);
        };

        let current = self.edits.materialize();
        if current == policy.policies {
            return Ok(ApplyResult::NoChanges);
        }

        let write = policy::write(self.browser, &current).map_err(|error| error.to_string())?;
        if should_wait_for_managed_policy_install() {
            self.watch_managed_policy()?;
        }
        if let Err(error) = open_written_policy(&write) {
            self.clear_awaiting_policy_change();
            return Err(error);
        }

        if should_wait_for_managed_policy_install() {
            self.awaiting_install = true;
            self.awaiting_uninstall = false;
            self.policy_error = None;
            return Ok(ApplyResult::AwaitingInstall);
        }

        if let Some(policy) = &mut self.policy {
            policy.source = write.target;
            policy.policies = current;
            self.edits = PolicyStage::new(manifest, self.browser, &policy.policies);
        }
        self.managed_policy_exists = true;
        self.first_missing_current = None;
        self.bump_policy_tree_version();
        self.clear_awaiting_policy_change();
        self.policy_error = None;

        Ok(ApplyResult::Applied)
    }

    pub fn export_policy_file(&self, path: &Path) -> Result<policy::PolicyWrite, String> {
        if self.policy.is_none() {
            return Err("no policy is available to save".to_owned());
        }

        let current = self.edits.materialize();
        policy::export(self.browser, &current, path).map_err(|error| error.to_string())
    }

    pub fn refresh_awaiting_policy_change(
        &mut self,
        manifest: &Manifest,
        preset: PolicySet,
    ) -> bool {
        if !self.awaiting_policy_change() {
            return false;
        }
        if !self.managed_policy_may_have_changed() {
            return false;
        }

        if self.awaiting_uninstall {
            return self.refresh_awaiting_uninstall(manifest, preset);
        }

        self.refresh_awaiting_install(manifest)
    }

    fn refresh_awaiting_install(&mut self, manifest: &Manifest) -> bool {
        if self.policy.is_none() {
            self.clear_awaiting_policy_change();
            return true;
        }
        let expected = self.edits.materialize();

        match policy::read(self.browser) {
            Ok(Some(updated)) if updated.policies == expected => {
                let edits = PolicyStage::new(manifest, self.browser, &updated.policies);
                self.policy = Some(updated);
                self.policy_error = None;
                self.managed_policy_exists = true;
                self.edits = edits;
                self.first_missing_current = None;
                self.bump_policy_tree_version();
                self.clear_awaiting_policy_change();
                true
            }
            Ok(Some(updated)) => {
                let changed = self.policy.as_ref().is_none_or(|policy| {
                    policy.source != updated.source || policy.policies != updated.policies
                }) || self.policy_error.is_some();
                if changed {
                    let edits = PolicyStage::new(manifest, self.browser, &updated.policies);
                    self.policy = Some(updated);
                    self.policy_error = None;
                    self.managed_policy_exists = true;
                    self.edits = edits;
                    self.first_missing_current = None;
                    self.bump_policy_tree_version();
                }

                changed
            }
            Ok(None) => false,
            Err(error) => self.set_policy_error(error.to_string()),
        }
    }

    fn refresh_awaiting_uninstall(&mut self, manifest: &Manifest, preset: PolicySet) -> bool {
        match policy::read(self.browser) {
            Ok(None) => {
                self.use_missing_policy_defaults(manifest, preset);
                true
            }
            Ok(Some(updated)) => {
                let changed = self.policy.as_ref().is_none_or(|policy| {
                    policy.source != updated.source || policy.policies != updated.policies
                }) || self.policy_error.is_some();
                if changed {
                    let edits = PolicyStage::new(manifest, self.browser, &updated.policies);
                    self.policy = Some(updated);
                    self.policy_error = None;
                    self.managed_policy_exists = true;
                    self.edits = edits;
                    self.first_missing_current = None;
                    self.bump_policy_tree_version();
                }

                changed
            }
            Err(error) => self.set_policy_error(error.to_string()),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn uninstall_policy(&mut self) -> Result<UninstallResult, String> {
        if !self.managed_policy_exists {
            return Ok(UninstallResult::NoPolicy);
        }

        policy::uninstall(self.browser).map_err(|error| error.to_string())?;

        Ok(UninstallResult::Uninstalled)
    }

    #[cfg(target_os = "macos")]
    pub fn uninstall_policy(&mut self) -> Result<UninstallResult, String> {
        if !self.managed_policy_exists {
            return Ok(UninstallResult::NoPolicy);
        }

        self.watch_managed_policy()?;
        if let Err(error) = crate::macos::open_profiles_settings() {
            self.clear_awaiting_policy_change();
            return Err(error.to_string());
        }

        self.awaiting_install = false;
        self.awaiting_uninstall = true;
        self.policy_error = None;
        if self.edits.revert() {
            self.bump_policy_tree_version();
        }

        Ok(UninstallResult::AwaitingUninstall)
    }

    pub fn use_missing_policy_defaults(&mut self, manifest: &Manifest, preset: PolicySet) {
        let (policy, edits) = missing_policy_with_defaults(manifest, self.browser, &preset);
        self.policy = Some(policy);
        self.policy_error = None;
        self.managed_policy_exists = false;
        self.edits = edits;
        self.first_missing_current = Some(preset);
        self.bump_policy_tree_version();
        self.clear_awaiting_policy_change();
    }

    pub fn stage_policy_removal_at(&mut self, cursor: &RowId) -> bool {
        self.edit_stage(|stage| policy_tree::remove_at(stage, cursor))
    }

    pub fn stage_policy_group_removal_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        let tree = self.policy_tree(manifest);
        self.edit_stage(|stage| {
            let Some(tree) = &tree else {
                return false;
            };

            policy_tree::remove_group_at(stage, tree, cursor)
        })
    }

    pub fn toggle_policy_group_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        let tree = self.policy_tree(manifest);
        self.edit_stage(|stage| {
            let Some(tree) = &tree else {
                return false;
            };

            policy_tree::toggle_group_at(stage, tree, cursor)
        })
    }

    pub fn toggle_policy_at(&mut self, cursor: &RowId) -> bool {
        self.edit_stage(|stage| policy_tree::toggle_policy_at(stage, cursor))
    }

    pub fn add_policy_key(&mut self, key: String, value: PolicyValue) -> bool {
        self.edit_stage(|stage| stage.add_custom_key(key, value))
    }

    pub fn new_list_item_target_at(
        &self,
        manifest: &Manifest,
        cursor: &RowId,
    ) -> Option<NewListItemTarget> {
        let tree = self.policy_tree(manifest)?;

        policy_tree::new_list_item_target_at(&tree, &self.edits, cursor)
    }

    pub fn add_list_item_value_at(
        &mut self,
        manifest: &Manifest,
        cursor: &RowId,
        value: PolicyValue,
    ) -> Option<RowId> {
        let before = self.policy_tree(manifest)?;
        let cursor = policy_tree::add_list_item_value_at(&before, &mut self.edits, cursor, value)?;

        self.bump_policy_tree_version();
        self.clear_awaiting_policy_change();

        Some(cursor)
    }

    pub fn toggle_policy_bool_at(&mut self, cursor: &RowId) -> bool {
        self.edit_stage(|stage| policy_tree::toggle_bool_at(stage, cursor))
    }

    pub fn editable_policy_value_at(&self, cursor: &RowId) -> Option<EditablePolicyValue> {
        policy_tree::editable_value_at(&self.edits, cursor)
    }

    pub fn set_policy_value_at(&mut self, cursor: &RowId, value: PolicyValue) -> bool {
        self.edit_stage(|stage| policy_tree::set_value_at(stage, cursor, value))
    }

    pub fn policy_key_cursor(&self, manifest: &Manifest, key: &str) -> Option<RowId> {
        let tree = self.policy_tree(manifest)?;

        policy_tree::key_cursor(&tree, key)
    }

    fn edit_stage(&mut self, update: impl FnOnce(&mut PolicyStage) -> bool) -> bool {
        if self.policy.is_none() {
            return false;
        }
        if !update(&mut self.edits) {
            return false;
        }

        self.bump_policy_tree_version();
        self.clear_awaiting_policy_change();
        true
    }

    fn set_policy_error(&mut self, error: String) -> bool {
        if self.policy_error.as_ref() == Some(&error) {
            return false;
        }

        self.policy_error = Some(error);
        true
    }

    fn clear_awaiting_policy_change(&mut self) {
        self.awaiting_install = false;
        self.awaiting_uninstall = false;
        #[cfg(target_os = "macos")]
        {
            self.managed_policy_watcher = None;
        }
    }

    fn bump_policy_tree_version(&mut self) {
        self.policy_tree_version = self.policy_tree_version.wrapping_add(1);
    }

    fn managed_policy_may_have_changed(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.managed_policy_watcher
                .as_ref()
                .is_some_and(ManagedPolicyWatcher::has_events)
        }

        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    fn watch_managed_policy(&mut self) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            self.managed_policy_watcher = Some(
                self.policy
                    .as_ref()
                    .and_then(managed_policy_path)
                    .ok_or_else(|| "managed policy path is not available".to_owned())
                    .and_then(|path| {
                        ManagedPolicyWatcher::new(path).map_err(|error| error.to_string())
                    })?,
            );
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }
}

fn missing_policy_with_defaults(
    manifest: &Manifest,
    browser: Browser,
    preset: &PolicySet,
) -> (BrowserPolicy, PolicyStage) {
    let baseline = PolicySet::new();
    let edits = PolicyStage::with_current(manifest, browser, &baseline, preset);

    (
        BrowserPolicy {
            browser,
            source: policy::managed_location(browser),
            policies: baseline,
        },
        edits,
    )
}

#[cfg(target_os = "macos")]
fn managed_policy_path(policy: &BrowserPolicy) -> Option<&Path> {
    match &policy.source {
        crate::chromium::policy::PolicyLocation::File(path) => Some(path.as_path()),
    }
}

#[cfg(target_os = "macos")]
const fn should_wait_for_managed_policy_install() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
const fn should_wait_for_managed_policy_install() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn open_written_policy(write: &policy::PolicyWrite) -> Result<(), String> {
    match &write.target {
        crate::chromium::policy::PolicyLocation::File(path) => {
            crate::macos::open_mobileconfig(path).map_err(|error| error.to_string())
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn open_written_policy(_write: &policy::PolicyWrite) -> Result<(), String> {
    Ok(())
}
