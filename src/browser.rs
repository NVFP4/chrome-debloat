use std::path::Path;

use crate::chromium::Browser;
use crate::chromium::detection::BrowserInstall;
use crate::chromium::policy::{self, BrowserPolicy, PolicyReadResult, PolicySet, PolicyValue};
use crate::diff::{self, DiffCounts};
use crate::history::EditHistory;
use crate::manifest::Manifest;
use crate::policy_tree::{
    self,
    EditablePolicyValue,
    NewListItemTarget,
    PolicyTree,
    PolicyValueUpdate,
    RowId,
};
#[cfg(target_os = "macos")]
use crate::watcher::ManagedPolicyWatcher;

#[derive(Debug)]
pub struct BrowserState {
    pub browser: Browser,
    pub install: Option<BrowserInstall>,
    pub policy: Option<BrowserPolicy>,
    pub policy_error: Option<String>,
    managed_policy_exists: bool,
    awaiting_install: bool,
    policy_tree_version: u64,
    #[cfg(target_os = "macos")]
    install_watcher: Option<ManagedPolicyWatcher>,
    edits: EditHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    AwaitingInstall,
    NoChanges,
}

impl BrowserState {
    pub fn new(
        browser: Browser,
        install: Option<BrowserInstall>,
        policy: PolicyReadResult,
        preset: PolicySet,
    ) -> Self {
        let (policy, policy_error) = match policy {
            Ok(policy) => (policy, None),
            Err(error) => (None, Some(error.to_string())),
        };
        let managed_policy_exists = policy.is_some();
        let (policy, edits) = match (policy, policy_error.is_none() && !preset.is_empty()) {
            (Some(policy), _) => (Some(policy), EditHistory::default()),
            (None, true) => {
                let baseline = PolicySet::new();
                let mut edits = EditHistory::default();
                edits.push(&baseline, preset);
                (
                    Some(BrowserPolicy {
                        browser,
                        source: policy::managed_location(browser),
                        policies: baseline,
                    }),
                    edits,
                )
            }
            (None, false) => (None, EditHistory::default()),
        };

        Self {
            browser,
            install,
            policy,
            policy_error,
            managed_policy_exists,
            awaiting_install: false,
            policy_tree_version: 0,
            #[cfg(target_os = "macos")]
            install_watcher: None,
            edits,
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

        self.edits.current_differs_from_first()
    }

    pub fn awaiting_install(&self) -> bool {
        self.awaiting_install && self.is_dirty()
    }

    pub fn has_policy(&self) -> bool {
        self.policy_sets()
            .is_some_and(|(baseline, current)| !baseline.is_empty() || !current.is_empty())
    }

    pub const fn managed_policy_exists(&self) -> bool {
        self.managed_policy_exists
    }

    pub const fn policy_tree_version(&self) -> u64 {
        self.policy_tree_version
    }

    pub fn policy_sets(&self) -> Option<(&PolicySet, &PolicySet)> {
        let policy = self.policy.as_ref()?;

        Some((&policy.policies, self.edits.current(&policy.policies)))
    }

    pub fn diff_counts(&self) -> DiffCounts {
        let Some((baseline, current)) = self.policy_sets() else {
            return DiffCounts::default();
        };

        diff::counts(baseline, current)
    }

    pub fn policy_tree(
        &self,
        manifest: &Manifest,
        baseline: &PolicySet,
        current: &PolicySet,
    ) -> PolicyTree {
        PolicyTree::build(manifest, self.browser, baseline, current)
    }

    pub fn undo(&mut self) -> bool {
        let Some(policy) = &self.policy else {
            return false;
        };
        let changed = self.edits.undo(&policy.policies);
        if changed {
            self.bump_policy_tree_version();
            self.clear_awaiting_install();
        }

        changed
    }

    pub fn redo(&mut self) -> bool {
        let Some(policy) = &self.policy else {
            return false;
        };
        let changed = self.edits.redo(&policy.policies);
        if changed {
            self.bump_policy_tree_version();
            self.clear_awaiting_install();
        }

        changed
    }

    pub fn revert(&mut self) -> bool {
        let changed = self.edits.revert() || self.awaiting_install;
        if changed {
            self.bump_policy_tree_version();
        }
        self.clear_awaiting_install();

        changed
    }

    pub fn apply_policy_changes(&mut self) -> Result<ApplyResult, String> {
        let Some(policy) = &self.policy else {
            return Ok(ApplyResult::NoChanges);
        };

        let current = self.edits.current(&policy.policies);
        if current == &policy.policies {
            return Ok(ApplyResult::NoChanges);
        }

        let current = current.clone();
        let write = policy::write(self.browser, &current).map_err(|error| error.to_string())?;
        if should_wait_for_managed_policy_install() {
            self.watch_managed_policy()?;
        }
        if let Err(error) = open_written_policy(&write) {
            self.clear_awaiting_install();
            return Err(error);
        }

        if should_wait_for_managed_policy_install() {
            self.awaiting_install = true;
            self.policy_error = None;
            return Ok(ApplyResult::AwaitingInstall);
        }

        if let Some(policy) = &mut self.policy {
            policy.source = write.target;
            policy.policies = current;
        }
        self.managed_policy_exists = true;
        self.edits.revert();
        self.bump_policy_tree_version();
        self.clear_awaiting_install();
        self.policy_error = None;

        Ok(ApplyResult::Applied)
    }

    pub fn export_policy_file(&self, path: &Path) -> Result<policy::PolicyWrite, String> {
        let Some(policy) = &self.policy else {
            return Err("no policy is available to save".to_owned());
        };

        let current = self.edits.current(&policy.policies);
        policy::export(self.browser, current, path).map_err(|error| error.to_string())
    }

    pub fn refresh_awaiting_install(&mut self) -> bool {
        if !self.awaiting_install {
            return false;
        }
        if !self.managed_policy_may_have_changed() {
            return false;
        }

        let Some(policy) = &self.policy else {
            self.clear_awaiting_install();
            return true;
        };
        let expected = self.edits.current(&policy.policies).clone();

        match policy::read(self.browser) {
            Ok(Some(updated)) if updated.policies == expected => {
                self.policy = Some(updated);
                self.policy_error = None;
                self.managed_policy_exists = true;
                self.edits.revert();
                self.bump_policy_tree_version();
                self.clear_awaiting_install();
                true
            }
            Ok(Some(updated)) => {
                let changed = self.policy.as_ref().is_none_or(|policy| {
                    policy.source != updated.source || policy.policies != updated.policies
                }) || self.policy_error.is_some();
                if changed {
                    self.policy = Some(updated);
                    self.policy_error = None;
                    self.managed_policy_exists = true;
                    self.bump_policy_tree_version();
                }

                changed
            }
            Ok(None) => false,
            Err(error) => {
                let error = error.to_string();
                if self.policy_error.as_ref() == Some(&error) {
                    return false;
                }

                self.policy_error = Some(error);
                true
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn uninstall_policy(&mut self) -> Result<(), String> {
        policy::uninstall(self.browser).map_err(|error| error.to_string())?;

        self.policy = Some(BrowserPolicy {
            browser: self.browser,
            source: policy::managed_location(self.browser),
            policies: PolicySet::new(),
        });
        self.policy_error = None;
        self.managed_policy_exists = false;
        self.edits.revert();
        self.bump_policy_tree_version();
        self.clear_awaiting_install();

        Ok(())
    }

    pub fn stage_policy_removal_at(&mut self, cursor: &RowId) -> bool {
        self.edit_current(|_, _, current| policy_tree::remove_at(current, cursor))
    }

    pub fn stage_policy_group_removal_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        self.edit_current(|browser, _, current| {
            policy_tree::remove_group_at(manifest, browser, current, cursor)
        })
    }

    pub fn toggle_policy_group_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        self.edit_current(|browser, baseline, current| {
            policy_tree::toggle_group_at(manifest, browser, baseline, current, cursor)
        })
    }

    pub fn toggle_policy_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        self.edit_current(|browser, baseline, current| {
            policy_tree::toggle_policy_at(manifest, browser, baseline, current, cursor)
        })
    }

    pub fn add_policy_key(&mut self, key: String, value: PolicyValue) -> bool {
        self.edit_current(|_, baseline, current| {
            if baseline.contains_key(&key) || current.contains_key(&key) {
                return None;
            }

            let mut updated = current.clone();
            updated.insert(key, value);
            Some(updated)
        })
    }

    pub fn new_list_item_target_at(
        &self,
        manifest: &Manifest,
        cursor: &RowId,
    ) -> Option<NewListItemTarget> {
        let policy = self.policy.as_ref()?;
        let baseline = &policy.policies;
        let current = self.edits.current(baseline);

        policy_tree::new_list_item_target_at(manifest, self.browser, baseline, current, cursor)
    }

    pub fn add_list_item_value_at(
        &mut self,
        manifest: &Manifest,
        cursor: &RowId,
        value: PolicyValue,
    ) -> Option<RowId> {
        let policy = self.policy.as_ref()?;
        let baseline = &policy.policies;
        let current = self.edits.current(baseline);
        let update = policy_tree::add_list_item_value_at(
            manifest,
            self.browser,
            baseline,
            current,
            PolicyValueUpdate {
                target: cursor.clone(),
                value,
            },
        )?;

        self.push_edit(update.policies);
        Some(update.cursor)
    }

    pub fn toggle_policy_bool_at(&mut self, manifest: &Manifest, cursor: &RowId) -> bool {
        self.edit_current(|browser, baseline, current| {
            policy_tree::toggle_bool_at(manifest, browser, baseline, current, cursor)
        })
    }

    pub fn editable_policy_value_at(
        &self,
        manifest: &Manifest,
        cursor: &RowId,
    ) -> Option<EditablePolicyValue> {
        let (baseline, current) = self.policy_sets()?;

        policy_tree::editable_value_at(manifest, self.browser, baseline, current, cursor)
    }

    pub fn set_policy_value_at(&mut self, cursor: &RowId, value: PolicyValue) -> bool {
        self.edit_current(|_, _, current| {
            policy_tree::set_value_at(
                current,
                PolicyValueUpdate {
                    target: cursor.clone(),
                    value,
                },
            )
        })
    }

    pub fn policy_key_cursor(&self, manifest: &Manifest, key: &str) -> Option<RowId> {
        let (baseline, current) = self.policy_sets()?;

        policy_tree::key_cursor(manifest, self.browser, baseline, current, key)
    }

    fn edit_current(
        &mut self,
        update: impl FnOnce(Browser, &PolicySet, &PolicySet) -> Option<PolicySet>,
    ) -> bool {
        let updated = {
            let Some(policy) = &self.policy else {
                return false;
            };
            let baseline = &policy.policies;
            let current = self.edits.current(baseline);
            let Some(updated) = update(self.browser, baseline, current) else {
                return false;
            };

            updated
        };

        self.push_edit(updated);
        true
    }

    fn clear_awaiting_install(&mut self) {
        self.awaiting_install = false;
        #[cfg(target_os = "macos")]
        {
            self.install_watcher = None;
        }
    }

    fn push_edit(&mut self, policies: PolicySet) {
        let Some(policy) = &self.policy else {
            return;
        };

        self.edits.push(&policy.policies, policies);
        self.bump_policy_tree_version();
        self.clear_awaiting_install();
    }

    fn bump_policy_tree_version(&mut self) {
        self.policy_tree_version = self.policy_tree_version.wrapping_add(1);
    }

    fn managed_policy_may_have_changed(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.install_watcher
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
            self.install_watcher = Some(
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
