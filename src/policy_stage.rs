use std::collections::BTreeMap;
use std::sync::Arc;

use crate::chromium::Browser;
use crate::chromium::policy::{PolicySet, PolicyValue};
use crate::diff::DiffCounts;
use crate::manifest::{Manifest, PolicySetting};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BaseIndex(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppendId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Applied,
    Modified,
    Deleted,
    Added,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageTarget {
    Base(BaseIndex),
    Append(AppendId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseChange<Item> {
    Modified(Item),
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendState<Item> {
    Present(Item),
    Deleted(Item),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow<'a, Item> {
    pub target: StageTarget,
    pub base: Option<&'a Item>,
    pub value: &'a Item,
    pub status: StageStatus,
}

#[derive(Debug, Clone)]
pub struct StageStore<GroupId, Item>
where
    GroupId: Ord + Clone,
    Item: Clone + PartialEq,
{
    groups: BTreeMap<GroupId, GroupStage<Item>>,
    history: Vec<StageChange<GroupId, Item>>,
    cursor: usize,
}

#[derive(Debug, Clone)]
pub struct GroupStage<Item> {
    base: Vec<Item>,
    overlay: BTreeMap<BaseIndex, BaseChange<Item>>,
    appends: AppendLog<Item>,
}

#[derive(Debug, Clone)]
pub struct AppendLog<Item> {
    entries: Vec<Option<AppendState<Item>>>,
    next_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
enum StageChange<GroupId, Item> {
    Patch(StagePatch<GroupId, Item>),
    Batch(Box<[StagePatch<GroupId, Item>]>),
}

#[derive(Debug, Clone, PartialEq)]
struct StagePatch<GroupId, Item> {
    group: GroupId,
    target: StageTarget,
    previous: PatchState<Item>,
    next: PatchState<Item>,
}

#[derive(Debug, Clone, PartialEq)]
enum PatchState<Item> {
    Base(BaseState<Item>),
    Append(AppendSlotState<Item>),
}

#[derive(Debug, Clone, PartialEq)]
enum BaseState<Item> {
    Applied,
    Modified(Item),
    Deleted,
}

#[derive(Debug, Clone, PartialEq)]
enum AppendSlotState<Item> {
    Absent,
    Present(Item),
    Deleted(Item),
}

pub struct Rows<'a, Item> {
    stage: Option<&'a GroupStage<Item>>,
    next_base: usize,
    next_append: usize,
}

impl BaseIndex {
    #[allow(dead_code)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

impl AppendId {
    #[allow(dead_code)]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl<GroupId, Item> StageStore<GroupId, Item>
where
    GroupId: Ord + Clone,
    Item: Clone + PartialEq,
{
    pub fn new(groups: impl IntoIterator<Item = (GroupId, Vec<Item>)>) -> Self {
        Self {
            groups: groups
                .into_iter()
                .map(|(group, base)| (group, GroupStage::new(base)))
                .collect(),
            history: Vec::new(),
            cursor: 0,
        }
    }

    pub fn group_ids(&self) -> impl Iterator<Item = &GroupId> {
        self.groups.keys()
    }

    pub fn rows(&self, group: &GroupId) -> Rows<'_, Item> {
        Rows::new(self.groups.get(group))
    }

    pub fn row_value(&self, group: &GroupId, target: StageTarget) -> Option<&Item> {
        self.groups.get(group)?.row_value(target)
    }

    #[allow(dead_code)]
    pub fn materialize_group(&self, group: &GroupId) -> Vec<Item> {
        self.groups
            .get(group)
            .map_or_else(Vec::new, GroupStage::materialize)
    }

    #[allow(dead_code)]
    pub fn materialize_all(&self) -> BTreeMap<GroupId, Vec<Item>> {
        self.groups
            .iter()
            .map(|(group, stage)| (group.clone(), stage.materialize()))
            .collect()
    }

    pub fn modify_base(&mut self, group: &GroupId, index: BaseIndex, value: Item) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(previous) = stage.base_state(index) else {
            return false;
        };
        let Some(base) = stage.base.get(index.get()) else {
            return false;
        };
        let next = if base == &value {
            BaseState::Applied
        } else {
            BaseState::Modified(value)
        };

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Base(index),
            previous: PatchState::Base(previous),
            next: PatchState::Base(next),
        })
    }

    #[allow(dead_code)]
    pub fn delete_base(&mut self, group: &GroupId, index: BaseIndex) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(previous) = stage.base_state(index) else {
            return false;
        };

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Base(index),
            previous: PatchState::Base(previous),
            next: PatchState::Base(BaseState::Deleted),
        })
    }

    #[allow(dead_code)]
    pub fn restore_base(&mut self, group: &GroupId, index: BaseIndex) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(previous) = stage.base_state(index) else {
            return false;
        };

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Base(index),
            previous: PatchState::Base(previous),
            next: PatchState::Base(BaseState::Applied),
        })
    }

    pub fn append(&mut self, group: &GroupId, value: Item) -> Option<AppendId> {
        let stage = self.groups.get_mut(group)?;
        let id = stage.appends.reserve_id()?;
        let patch = StagePatch {
            group: group.clone(),
            target: StageTarget::Append(id),
            previous: PatchState::Append(AppendSlotState::Absent),
            next: PatchState::Append(AppendSlotState::Present(value)),
        };

        self.push_patch(patch).then_some(id)
    }

    pub fn edit_append(&mut self, group: &GroupId, id: AppendId, value: Item) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(previous) = stage.appends.slot_state(id) else {
            return false;
        };
        if !matches!(previous, AppendSlotState::Present(_)) {
            return false;
        }

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Append(id),
            previous: PatchState::Append(previous),
            next: PatchState::Append(AppendSlotState::Present(value)),
        })
    }

    pub fn delete_append(&mut self, group: &GroupId, id: AppendId) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(AppendSlotState::Present(value)) = stage.appends.slot_state(id) else {
            return false;
        };

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Append(id),
            previous: PatchState::Append(AppendSlotState::Present(value.clone())),
            next: PatchState::Append(AppendSlotState::Deleted(value)),
        })
    }

    #[allow(dead_code)]
    pub fn restore_append(&mut self, group: &GroupId, id: AppendId) -> bool {
        let Some(stage) = self.groups.get(group) else {
            return false;
        };
        let Some(AppendSlotState::Deleted(value)) = stage.appends.slot_state(id) else {
            return false;
        };

        self.push_patch(StagePatch {
            group: group.clone(),
            target: StageTarget::Append(id),
            previous: PatchState::Append(AppendSlotState::Deleted(value.clone())),
            next: PatchState::Append(AppendSlotState::Present(value)),
        })
    }

    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let change = &self.history[self.cursor - 1];
        if !apply_change_previous(&mut self.groups, change) {
            return false;
        }
        self.cursor -= 1;

        true
    }

    pub fn redo(&mut self) -> bool {
        if self.cursor >= self.history.len() {
            return false;
        }

        let change = &self.history[self.cursor];
        if !apply_change_next(&mut self.groups, change) {
            return false;
        }
        self.cursor += 1;

        true
    }

    pub fn revert(&mut self) -> bool {
        let changed = self.cursor != 0
            || !self.history.is_empty()
            || self.groups.values().any(GroupStage::has_stage_state);

        for stage in self.groups.values_mut() {
            stage.revert();
        }
        self.history.clear();
        self.cursor = 0;

        changed
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        self.groups.values().any(GroupStage::is_dirty)
    }

    #[allow(dead_code)]
    pub fn is_group_dirty(&self, group: &GroupId) -> bool {
        self.groups.get(group).is_some_and(GroupStage::is_dirty)
    }

    #[allow(dead_code)]
    pub fn dirty_groups(&self) -> impl Iterator<Item = &GroupId> {
        self.groups
            .iter()
            .filter_map(|(group, stage)| stage.is_dirty().then_some(group))
    }

    #[allow(dead_code)]
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    #[allow(dead_code)]
    pub fn can_redo(&self) -> bool {
        self.cursor < self.history.len()
    }

    fn push_patch(&mut self, patch: StagePatch<GroupId, Item>) -> bool {
        if patch.previous == patch.next {
            return false;
        }

        self.push_change(StageChange::Patch(patch))
    }

    fn clear_history(&mut self) {
        self.history.clear();
        self.cursor = 0;
    }

    fn push_patches(&mut self, mut patches: Vec<StagePatch<GroupId, Item>>) -> bool {
        patches.retain(|patch| patch.previous != patch.next);

        match patches.len() {
            0 => false,
            1 => match patches.pop() {
                Some(patch) => self.push_change(StageChange::Patch(patch)),
                None => false,
            },
            _ => self.push_change(StageChange::Batch(patches.into_boxed_slice())),
        }
    }

    fn push_change(&mut self, change: StageChange<GroupId, Item>) -> bool {
        if !apply_change_next(&mut self.groups, &change) {
            return false;
        }

        self.history.truncate(self.cursor);
        self.history.push(change);
        self.cursor = self.history.len();

        true
    }

    fn set_patch(
        &self,
        group: GroupId,
        target: StageTarget,
        value: Item,
    ) -> Option<StagePatch<GroupId, Item>> {
        match target {
            StageTarget::Base(index) => self.base_set_patch(group, index, value),
            StageTarget::Append(id) => self.append_set_patch(group, id, value),
        }
    }

    fn base_set_patch(
        &self,
        group: GroupId,
        index: BaseIndex,
        value: Item,
    ) -> Option<StagePatch<GroupId, Item>> {
        let stage = self.groups.get(&group)?;
        let previous = stage.base_state(index)?;
        let base = stage.base.get(index.get())?;
        let next = if base == &value {
            BaseState::Applied
        } else {
            BaseState::Modified(value)
        };

        Some(StagePatch {
            group,
            target: StageTarget::Base(index),
            previous: PatchState::Base(previous),
            next: PatchState::Base(next),
        })
    }

    fn append_set_patch(
        &self,
        group: GroupId,
        id: AppendId,
        value: Item,
    ) -> Option<StagePatch<GroupId, Item>> {
        let stage = self.groups.get(&group)?;
        let previous = stage.appends.slot_state(id)?;
        if !matches!(previous, AppendSlotState::Present(_)) {
            return None;
        }

        Some(StagePatch {
            group,
            target: StageTarget::Append(id),
            previous: PatchState::Append(previous),
            next: PatchState::Append(AppendSlotState::Present(value)),
        })
    }

    fn append_delete_patch(
        &self,
        group: GroupId,
        id: AppendId,
    ) -> Option<StagePatch<GroupId, Item>> {
        let stage = self.groups.get(&group)?;
        let AppendSlotState::Present(value) = stage.appends.slot_state(id)? else {
            return None;
        };

        Some(StagePatch {
            group,
            target: StageTarget::Append(id),
            previous: PatchState::Append(AppendSlotState::Present(value.clone())),
            next: PatchState::Append(AppendSlotState::Deleted(value)),
        })
    }
}

fn apply_change_previous<GroupId, Item>(
    groups: &mut BTreeMap<GroupId, GroupStage<Item>>,
    change: &StageChange<GroupId, Item>,
) -> bool
where
    GroupId: Ord,
    Item: Clone + PartialEq,
{
    match change {
        StageChange::Patch(patch) => apply_patch_previous(groups, patch),
        StageChange::Batch(patches) => patches
            .iter()
            .rev()
            .all(|patch| apply_patch_previous(groups, patch)),
    }
}

fn apply_change_next<GroupId, Item>(
    groups: &mut BTreeMap<GroupId, GroupStage<Item>>,
    change: &StageChange<GroupId, Item>,
) -> bool
where
    GroupId: Ord,
    Item: Clone + PartialEq,
{
    match change {
        StageChange::Patch(patch) => apply_patch_next(groups, patch),
        StageChange::Batch(patches) => patches.iter().all(|patch| apply_patch_next(groups, patch)),
    }
}

fn apply_patch_previous<GroupId, Item>(
    groups: &mut BTreeMap<GroupId, GroupStage<Item>>,
    patch: &StagePatch<GroupId, Item>,
) -> bool
where
    GroupId: Ord,
    Item: Clone + PartialEq,
{
    apply_patch_state(groups, &patch.group, patch.target, &patch.previous)
}

fn apply_patch_next<GroupId, Item>(
    groups: &mut BTreeMap<GroupId, GroupStage<Item>>,
    patch: &StagePatch<GroupId, Item>,
) -> bool
where
    GroupId: Ord,
    Item: Clone + PartialEq,
{
    apply_patch_state(groups, &patch.group, patch.target, &patch.next)
}

fn apply_patch_state<GroupId, Item>(
    groups: &mut BTreeMap<GroupId, GroupStage<Item>>,
    group: &GroupId,
    target: StageTarget,
    state: &PatchState<Item>,
) -> bool
where
    GroupId: Ord,
    Item: Clone + PartialEq,
{
    let Some(stage) = groups.get_mut(group) else {
        return false;
    };

    match (target, state) {
        (StageTarget::Base(index), PatchState::Base(state)) => stage.apply_base_state(index, state),
        (StageTarget::Append(id), PatchState::Append(state)) => {
            stage.appends.apply_slot_state(id, state)
        }
        (StageTarget::Base(_), PatchState::Append(_))
        | (StageTarget::Append(_), PatchState::Base(_)) => false,
    }
}

impl<GroupId, Item> Default for StageStore<GroupId, Item>
where
    GroupId: Ord + Clone,
    Item: Clone + PartialEq,
{
    fn default() -> Self {
        Self {
            groups: BTreeMap::new(),
            history: Vec::new(),
            cursor: 0,
        }
    }
}

impl<Item> GroupStage<Item>
where
    Item: Clone + PartialEq,
{
    pub fn new(base: Vec<Item>) -> Self {
        Self {
            base,
            overlay: BTreeMap::new(),
            appends: AppendLog::new(),
        }
    }

    #[allow(dead_code)]
    pub fn overlay(&self) -> &BTreeMap<BaseIndex, BaseChange<Item>> {
        &self.overlay
    }

    #[allow(dead_code)]
    pub fn appends(&self) -> &AppendLog<Item> {
        &self.appends
    }

    #[allow(dead_code)]
    pub fn rows(&self) -> Rows<'_, Item> {
        Rows::new(Some(self))
    }

    fn row_value(&self, target: StageTarget) -> Option<&Item> {
        match target {
            StageTarget::Base(index) => {
                let base = self.base.get(index.get())?;
                match self.overlay.get(&index) {
                    Some(BaseChange::Modified(value)) => Some(value),
                    Some(BaseChange::Deleted) | None => Some(base),
                }
            }
            StageTarget::Append(id) => self.appends.present_value(id),
        }
    }

    #[allow(dead_code)]
    pub fn materialize(&self) -> Vec<Item> {
        let mut items = Vec::with_capacity(self.base.len() + self.appends.present_len());

        for (index, item) in self.base.iter().enumerate() {
            match self.overlay.get(&BaseIndex(index)) {
                Some(BaseChange::Modified(value)) => items.push(value.clone()),
                Some(BaseChange::Deleted) => {}
                None => items.push(item.clone()),
            }
        }

        items.extend(self.appends.present_values().cloned());
        items
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        !self.overlay.is_empty() || self.appends.has_present()
    }

    fn has_stage_state(&self) -> bool {
        !self.overlay.is_empty() || !self.appends.is_empty()
    }

    fn revert(&mut self) {
        self.overlay.clear();
        self.appends.clear();
    }

    fn base_state(&self, index: BaseIndex) -> Option<BaseState<Item>> {
        self.base.get(index.get())?;

        Some(match self.overlay.get(&index) {
            Some(BaseChange::Modified(value)) => BaseState::Modified(value.clone()),
            Some(BaseChange::Deleted) => BaseState::Deleted,
            None => BaseState::Applied,
        })
    }

    fn apply_base_state(&mut self, index: BaseIndex, state: &BaseState<Item>) -> bool {
        let Some(base) = self.base.get(index.get()) else {
            return false;
        };

        match state {
            BaseState::Applied => {
                self.overlay.remove(&index);
            }
            BaseState::Modified(value) if base == value => {
                self.overlay.remove(&index);
            }
            BaseState::Modified(value) => {
                self.overlay
                    .insert(index, BaseChange::Modified(value.clone()));
            }
            BaseState::Deleted => {
                self.overlay.insert(index, BaseChange::Deleted);
            }
        }

        true
    }
}

impl<Item> AppendLog<Item>
where
    Item: Clone,
{
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
        }
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> &[Option<AppendState<Item>>] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }

    #[allow(dead_code)]
    pub fn has_present(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, Some(AppendState::Present(_))))
    }

    #[allow(dead_code)]
    pub fn present_len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, Some(AppendState::Present(_))))
            .count()
    }

    #[allow(dead_code)]
    fn present_values(&self) -> impl Iterator<Item = &Item> {
        self.entries.iter().filter_map(|entry| match entry {
            Some(AppendState::Present(value)) => Some(value),
            Some(AppendState::Deleted(_)) | None => None,
        })
    }

    fn present_value(&self, id: AppendId) -> Option<&Item> {
        match self.entry(id)? {
            AppendState::Present(value) => Some(value),
            AppendState::Deleted(_) => None,
        }
    }

    fn reserve_id(&mut self) -> Option<AppendId> {
        let next_id = self.next_id.checked_add(1)?;
        let id = AppendId(self.next_id);
        self.next_id = next_id;

        Some(id)
    }

    fn slot_state(&self, id: AppendId) -> Option<AppendSlotState<Item>> {
        let entry = self.entry(id)?;

        Some(match entry {
            AppendState::Present(value) => AppendSlotState::Present(value.clone()),
            AppendState::Deleted(value) => AppendSlotState::Deleted(value.clone()),
        })
    }

    fn apply_slot_state(&mut self, id: AppendId, state: &AppendSlotState<Item>) -> bool {
        match state {
            AppendSlotState::Absent => self.remove(id),
            AppendSlotState::Present(value) => {
                self.set_state(id, AppendState::Present(value.clone()));
                true
            }
            AppendSlotState::Deleted(value) => {
                self.set_state(id, AppendState::Deleted(value.clone()));
                true
            }
        }
    }

    fn entry(&self, id: AppendId) -> Option<&AppendState<Item>> {
        self.entries.get(append_position(id)?)?.as_ref()
    }

    fn set_state(&mut self, id: AppendId, state: AppendState<Item>) {
        let Some(position) = append_position(id) else {
            return;
        };
        if self.entries.len() <= position {
            self.entries.resize_with(position + 1, || None);
        }
        self.entries[position] = Some(state);

        if let Some(next_id) = id.get().checked_add(1) {
            self.next_id = self.next_id.max(next_id);
        }
    }

    fn remove(&mut self, id: AppendId) -> bool {
        let Some(position) = append_position(id) else {
            return false;
        };
        let Some(entry) = self.entries.get_mut(position) else {
            return false;
        };
        let removed = entry.take().is_some();
        self.trim_absent_tail();

        removed
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.next_id = 0;
    }

    fn trim_absent_tail(&mut self) {
        while self.entries.last().is_some_and(Option::is_none) {
            self.entries.pop();
        }
    }
}

fn append_position(id: AppendId) -> Option<usize> {
    usize::try_from(id.get()).ok()
}

impl<Item> Default for AppendLog<Item>
where
    Item: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, Item> Rows<'a, Item> {
    fn new(stage: Option<&'a GroupStage<Item>>) -> Self {
        Self {
            stage,
            next_base: 0,
            next_append: 0,
        }
    }
}

impl<'a, Item> Iterator for Rows<'a, Item> {
    type Item = StageRow<'a, Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let stage = self.stage?;

        if self.next_base < stage.base.len() {
            let index = BaseIndex(self.next_base);
            let base = &stage.base[self.next_base];
            self.next_base += 1;

            return Some(match stage.overlay.get(&index) {
                Some(BaseChange::Modified(value)) => StageRow {
                    target: StageTarget::Base(index),
                    base: Some(base),
                    value,
                    status: StageStatus::Modified,
                },
                Some(BaseChange::Deleted) => StageRow {
                    target: StageTarget::Base(index),
                    base: Some(base),
                    value: base,
                    status: StageStatus::Deleted,
                },
                None => StageRow {
                    target: StageTarget::Base(index),
                    base: Some(base),
                    value: base,
                    status: StageStatus::Applied,
                },
            });
        }

        while self.next_append < stage.appends.entries.len() {
            let position = self.next_append;
            let entry = &stage.appends.entries[position];
            self.next_append += 1;

            if let Some(AppendState::Present(value)) = entry {
                let id = AppendId(u64::try_from(position).ok()?);
                return Some(StageRow {
                    target: StageTarget::Append(id),
                    base: None,
                    value,
                    status: StageStatus::Added,
                });
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicyGroupId {
    Custom,
    Manifest(Arc<str>),
}

impl PolicyGroupId {
    pub(crate) fn manifest(id: &str) -> Self {
        Self::Manifest(Arc::from(id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyItem {
    key: Arc<str>,
    value: Arc<PolicyValue>,
    applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyAddress {
    group: PolicyGroupId,
    target: StageTarget,
}

#[derive(Debug, Clone)]
pub(crate) struct PolicyStage {
    store: StageStore<PolicyGroupId, PolicyItem>,
    key_index: BTreeMap<Arc<str>, PolicyAddress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyItemStatus {
    Applied,
    Added,
    Edited,
    Deleted,
    NotApplied,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PolicyItemRow<'a> {
    pub(crate) group: &'a PolicyGroupId,
    pub(crate) target: StageTarget,
    pub(crate) base: Option<&'a PolicyItem>,
    pub(crate) current: &'a PolicyItem,
    pub(crate) status: PolicyItemStatus,
}

impl PolicyItem {
    pub(crate) fn applied(key: impl Into<Arc<str>>, value: PolicyValue) -> Self {
        Self {
            key: key.into(),
            value: Arc::new(value),
            applied: true,
        }
    }

    pub(crate) fn not_applied(key: impl Into<Arc<str>>, value: PolicyValue) -> Self {
        Self {
            key: key.into(),
            value: Arc::new(value),
            applied: false,
        }
    }

    pub(crate) fn with_value(&self, value: PolicyValue) -> Self {
        Self {
            key: Arc::clone(&self.key),
            value: Arc::new(value),
            applied: true,
        }
    }

    pub(crate) fn with_applied(&self, applied: bool) -> Self {
        Self {
            key: Arc::clone(&self.key),
            value: Arc::clone(&self.value),
            applied,
        }
    }

    pub(crate) fn key(&self) -> &str {
        self.key.as_ref()
    }

    pub(crate) fn value(&self) -> &PolicyValue {
        self.value.as_ref()
    }

    pub(crate) const fn is_applied(&self) -> bool {
        self.applied
    }
}

impl PolicyStage {
    pub(crate) fn new(manifest: &Manifest, browser: Browser, baseline: &PolicySet) -> Self {
        let store = StageStore::new(group_bases(manifest, browser, baseline));
        let key_index = build_key_index(&store);

        Self { store, key_index }
    }

    pub(crate) fn with_current(
        manifest: &Manifest,
        browser: Browser,
        baseline: &PolicySet,
        current: &PolicySet,
    ) -> Self {
        let mut stage = Self::new(manifest, browser, baseline);
        stage.set_current(current);
        stage.store.clear_history();
        stage
    }

    pub(crate) fn groups(&self) -> impl Iterator<Item = &PolicyGroupId> {
        self.store.group_ids()
    }

    pub(crate) fn rows<'a>(
        &'a self,
        group: &'a PolicyGroupId,
    ) -> impl Iterator<Item = PolicyItemRow<'a>> + 'a {
        self.store.rows(group).map(move |row| {
            let status = item_status(row.base, row.value);
            PolicyItemRow {
                group,
                target: row.target,
                base: row.base,
                current: row.value,
                status,
            }
        })
    }

    pub(crate) fn materialize(&self) -> PolicySet {
        let mut policies = PolicySet::new();
        for group in self.groups() {
            for row in self.store.rows(group) {
                if row.value.is_applied() {
                    policies.insert(row.value.key().to_owned(), row.value.value().clone());
                }
            }
        }

        policies
    }

    pub(crate) fn diff_counts(&self) -> DiffCounts {
        let mut counts = DiffCounts::default();
        for group in self.groups() {
            for row in self.rows(group) {
                match row.status {
                    PolicyItemStatus::Added => counts.added = counts.added.saturating_add(1),
                    PolicyItemStatus::Edited => counts.edited = counts.edited.saturating_add(1),
                    PolicyItemStatus::Deleted => counts.deleted = counts.deleted.saturating_add(1),
                    PolicyItemStatus::Applied | PolicyItemStatus::NotApplied => {}
                }
            }
        }

        counts
    }

    #[allow(dead_code)]
    pub(crate) fn is_dirty(&self) -> bool {
        !self.diff_counts().is_empty()
    }

    pub(crate) fn value_at(
        &self,
        group: &PolicyGroupId,
        target: StageTarget,
    ) -> Option<&PolicyValue> {
        self.item_at(group, target)
            .filter(|item| item.is_applied())
            .map(PolicyItem::value)
    }

    pub(crate) fn item_value_at(
        &self,
        group: &PolicyGroupId,
        target: StageTarget,
    ) -> Option<&PolicyValue> {
        self.item_at(group, target).map(PolicyItem::value)
    }

    pub(crate) fn has_key(&self, key: &str) -> bool {
        self.key_index.contains_key(key)
    }

    #[allow(dead_code)]
    pub(crate) fn has_applied_key(&self, key: &str) -> bool {
        self.key_index
            .get(key)
            .and_then(|address| self.value_at(&address.group, address.target))
            .is_some()
    }

    pub(crate) fn set_key(&mut self, key: &str, value: PolicyValue) -> bool {
        match self.key_index.get(key).cloned() {
            Some(address) => self.set_row_value(&address.group, address.target, value),
            None => {
                let item = PolicyItem::applied(key, value);
                let index_key = Arc::clone(&item.key);
                let group = PolicyGroupId::Custom;
                let Some(id) = self.store.append(&group, item) else {
                    return false;
                };

                self.key_index.insert(
                    index_key,
                    PolicyAddress {
                        group,
                        target: StageTarget::Append(id),
                    },
                );
                true
            }
        }
    }

    pub(crate) fn add_custom_key(&mut self, key: String, value: PolicyValue) -> bool {
        if self.has_key(&key) {
            return false;
        }

        let item = PolicyItem::applied(key, value);
        let index_key = Arc::clone(&item.key);
        let group = PolicyGroupId::Custom;
        let Some(id) = self.store.append(&group, item) else {
            return false;
        };

        self.key_index.insert(
            index_key,
            PolicyAddress {
                group,
                target: StageTarget::Append(id),
            },
        );
        true
    }

    pub(crate) fn set_row_applied(
        &mut self,
        group: &PolicyGroupId,
        target: StageTarget,
        applied: bool,
    ) -> bool {
        let Some(item) = self.row_item(group, target) else {
            return false;
        };

        self.set_row(group, target, item.with_applied(applied))
    }

    pub(crate) fn set_row_value(
        &mut self,
        group: &PolicyGroupId,
        target: StageTarget,
        value: PolicyValue,
    ) -> bool {
        let Some(item) = self.row_item(group, target) else {
            return false;
        };

        self.set_row(group, target, item.with_value(value))
    }

    pub(crate) fn delete_row(&mut self, group: &PolicyGroupId, target: StageTarget) -> bool {
        let changed = match target {
            StageTarget::Base(_) => self.set_row_applied(group, target, false),
            StageTarget::Append(id) => self.store.delete_append(group, id),
        };
        if changed && matches!(target, StageTarget::Append(_)) {
            self.rebuild_key_index();
        }

        changed
    }

    pub(crate) fn set_rows_applied(
        &mut self,
        targets: impl IntoIterator<Item = (PolicyGroupId, StageTarget)>,
        applied: bool,
    ) -> bool {
        let mut targets = targets.into_iter();
        let mut patches = Vec::with_capacity(targets.size_hint().0);
        for (group, target) in &mut targets {
            let Some(item) = self.item_at(&group, target) else {
                return false;
            };
            if item.is_applied() == applied {
                continue;
            }

            let item = item.with_applied(applied);
            let Some(patch) = self.store.set_patch(group, target, item) else {
                return false;
            };

            patches.push(patch);
        }

        self.push_policy_patches(patches)
    }

    pub(crate) fn delete_rows(
        &mut self,
        targets: impl IntoIterator<Item = (PolicyGroupId, StageTarget)>,
    ) -> bool {
        let mut targets = targets.into_iter();
        let mut patches = Vec::with_capacity(targets.size_hint().0);
        for (group, target) in &mut targets {
            if matches!(target, StageTarget::Base(_))
                && self
                    .item_at(&group, target)
                    .is_some_and(|item| !item.is_applied())
            {
                continue;
            }
            let Some(patch) = self.delete_row_patch(group, target) else {
                return false;
            };
            patches.push(patch);
        }

        self.push_policy_patches(patches)
    }

    pub(crate) fn undo(&mut self) -> bool {
        let changed = self.store.undo();
        if changed {
            self.rebuild_key_index();
        }

        changed
    }

    pub(crate) fn redo(&mut self) -> bool {
        let changed = self.store.redo();
        if changed {
            self.rebuild_key_index();
        }

        changed
    }

    pub(crate) fn revert(&mut self) -> bool {
        let changed = self.store.revert();
        if changed {
            self.rebuild_key_index();
        }

        changed
    }

    fn set_current(&mut self, current: &PolicySet) {
        for (key, value) in current {
            self.set_key(key, value.clone());
        }

        let removed = self
            .key_index
            .iter()
            .filter(|(key, address)| {
                !current.contains_key(key.as_ref())
                    && self.value_at(&address.group, address.target).is_some()
            })
            .map(|(_, address)| address.clone())
            .collect::<Vec<_>>();

        for address in removed {
            self.set_row_applied(&address.group, address.target, false);
        }

        self.rebuild_key_index();
    }

    fn row_item(&self, group: &PolicyGroupId, target: StageTarget) -> Option<PolicyItem> {
        self.item_at(group, target).cloned()
    }

    fn item_at(&self, group: &PolicyGroupId, target: StageTarget) -> Option<&PolicyItem> {
        self.store.row_value(group, target)
    }

    fn set_row(&mut self, group: &PolicyGroupId, target: StageTarget, item: PolicyItem) -> bool {
        match target {
            StageTarget::Base(index) => self.store.modify_base(group, index, item),
            StageTarget::Append(id) => self.store.edit_append(group, id, item),
        }
    }

    fn delete_row_patch(
        &self,
        group: PolicyGroupId,
        target: StageTarget,
    ) -> Option<StagePatch<PolicyGroupId, PolicyItem>> {
        match target {
            StageTarget::Base(_) => {
                let item = self.item_at(&group, target)?.with_applied(false);
                self.store.set_patch(group, target, item)
            }
            StageTarget::Append(id) => self.store.append_delete_patch(group, id),
        }
    }

    fn push_policy_patches(&mut self, patches: Vec<StagePatch<PolicyGroupId, PolicyItem>>) -> bool {
        let changed = self.store.push_patches(patches);
        if changed {
            self.rebuild_key_index();
        }

        changed
    }

    fn rebuild_key_index(&mut self) {
        self.key_index = build_key_index(&self.store);
    }
}

fn build_key_index(
    store: &StageStore<PolicyGroupId, PolicyItem>,
) -> BTreeMap<Arc<str>, PolicyAddress> {
    let mut index = BTreeMap::new();
    for group in store.group_ids() {
        for row in store.rows(group) {
            index.insert(
                Arc::clone(&row.value.key),
                PolicyAddress {
                    group: group.clone(),
                    target: row.target,
                },
            );
        }
    }

    index
}

fn group_bases(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
) -> Vec<(PolicyGroupId, Vec<PolicyItem>)> {
    let manifest_keys = manifest_keys(manifest, browser);
    let mut groups = Vec::new();
    let custom_items = baseline
        .iter()
        .filter(|(key, _)| !manifest_keys.contains(key.as_str()))
        .map(|(key, value)| PolicyItem::applied(key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    groups.push((PolicyGroupId::Custom, custom_items));

    for group in manifest.policy_groups(browser) {
        let items = group
            .settings
            .iter()
            .map(|setting| policy_item_from_setting(setting, baseline))
            .collect();
        groups.push((PolicyGroupId::manifest(&group.id), items));
    }

    groups
}

fn policy_item_from_setting(setting: &PolicySetting, baseline: &PolicySet) -> PolicyItem {
    match baseline.get(&setting.key) {
        Some(value) => PolicyItem::applied(setting.key.as_str(), value.clone()),
        None => PolicyItem::not_applied(setting.key.as_str(), setting.value.clone()),
    }
}

fn manifest_keys(manifest: &Manifest, browser: Browser) -> std::collections::BTreeSet<&str> {
    manifest
        .policy_groups(browser)
        .flat_map(|group| group.settings.iter().map(|setting| setting.key.as_str()))
        .collect()
}

fn item_status(base: Option<&PolicyItem>, current: &PolicyItem) -> PolicyItemStatus {
    match (base, current.is_applied()) {
        (None, true) => PolicyItemStatus::Added,
        (None, false) => PolicyItemStatus::NotApplied,
        (Some(base), false) if base.is_applied() => PolicyItemStatus::Deleted,
        (Some(_), false) => PolicyItemStatus::NotApplied,
        (Some(base), true) if !base.is_applied() => PolicyItemStatus::Added,
        (Some(base), true) if base.value() != current.value() => PolicyItemStatus::Edited,
        (Some(_), true) => PolicyItemStatus::Applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum TestGroup {
        Extensions,
    }

    #[test]
    fn modifies_and_deletes_base_rows_by_stable_index() {
        let group = group();
        let mut store = store(&["alpha", "beta"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("updated")));
        assert!(store.delete_base(&group, BaseIndex(1)));

        assert_eq!(
            rows(&store, group),
            vec![
                (
                    StageTarget::Base(BaseIndex(0)),
                    "updated".to_owned(),
                    StageStatus::Modified,
                ),
                (
                    StageTarget::Base(BaseIndex(1)),
                    "beta".to_owned(),
                    StageStatus::Deleted,
                ),
            ],
        );
        assert_eq!(store.materialize_group(&group), items(&["updated"]));
        assert!(store.is_group_dirty(&group));
        assert!(store.is_dirty());
    }

    #[test]
    fn appends_edits_and_deletes_by_stable_append_id() {
        let group = group();
        let mut store = store(&[]);

        assert_eq!(store.append(&group, item("first")), Some(AppendId(0)));
        assert!(store.edit_append(&group, AppendId(0), item("edited")));
        assert_eq!(
            rows(&store, group),
            vec![(
                StageTarget::Append(AppendId(0)),
                "edited".to_owned(),
                StageStatus::Added,
            )],
        );

        assert!(store.delete_append(&group, AppendId(0)));

        assert_eq!(
            rows(&store, group),
            Vec::<(StageTarget, String, StageStatus)>::new()
        );
        assert_eq!(store.materialize_group(&group), Vec::<String>::new());
        assert!(!store.is_dirty());

        assert!(store.undo());
        assert_eq!(store.materialize_group(&group), items(&["edited"]));
    }

    #[test]
    fn undo_and_redo_move_the_history_cursor() {
        let group = group();
        let mut store = store(&["base"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("modified")));
        assert_eq!(store.append(&group, item("added")), Some(AppendId(0)));

        assert!(store.can_undo());
        assert!(!store.can_redo());
        assert_eq!(
            store.materialize_group(&group),
            items(&["modified", "added"])
        );

        assert!(store.undo());
        assert_eq!(store.materialize_group(&group), items(&["modified"]));
        assert!(store.can_redo());

        assert!(store.undo());
        assert_eq!(store.materialize_group(&group), items(&["base"]));
        assert!(!store.can_undo());

        assert!(store.redo());
        assert_eq!(store.materialize_group(&group), items(&["modified"]));

        assert!(store.redo());
        assert_eq!(
            store.materialize_group(&group),
            items(&["modified", "added"])
        );
        assert!(!store.redo());
    }

    #[test]
    fn batch_patches_undo_and_redo_as_one_history_entry() -> Result<(), &'static str> {
        let group = group();
        let mut store = store(&["alpha", "beta", "gamma"]);
        let patches = vec![
            store
                .set_patch(group, StageTarget::Base(BaseIndex(0)), item("alpha edited"))
                .ok_or("base index 0 exists")?,
            store
                .set_patch(group, StageTarget::Base(BaseIndex(1)), item("beta edited"))
                .ok_or("base index 1 exists")?,
        ];

        assert!(store.push_patches(patches));
        assert_eq!(
            store.materialize_group(&group),
            items(&["alpha edited", "beta edited", "gamma"])
        );

        assert!(store.undo());
        assert_eq!(
            store.materialize_group(&group),
            items(&["alpha", "beta", "gamma"])
        );
        assert!(!store.can_undo());

        assert!(store.redo());
        assert_eq!(
            store.materialize_group(&group),
            items(&["alpha edited", "beta edited", "gamma"])
        );
        assert!(!store.can_redo());

        Ok(())
    }

    #[test]
    fn new_edit_after_undo_clears_redo_history() {
        let group = group();
        let mut store = store(&["alpha", "beta"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("first edit")));
        assert!(store.modify_base(&group, BaseIndex(1), item("redo candidate")));
        assert!(store.undo());
        assert_eq!(
            store.materialize_group(&group),
            items(&["first edit", "beta"])
        );

        assert!(store.delete_base(&group, BaseIndex(1)));

        assert!(!store.can_redo());
        assert!(!store.redo());
        assert_eq!(store.materialize_group(&group), items(&["first edit"]));
    }

    #[test]
    fn clearing_history_keeps_staged_state_without_undo() {
        let group = group();
        let mut store = store(&["base"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("recommended")));
        store.clear_history();

        assert_eq!(store.materialize_group(&group), items(&["recommended"]));
        assert!(store.is_dirty());
        assert!(!store.can_undo());
        assert!(!store.undo());
    }

    #[test]
    fn lazy_rows_expose_borrowed_values_and_statuses() {
        let group = group();
        let mut store = store(&["base", "remove"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("changed")));
        assert!(store.delete_base(&group, BaseIndex(1)));
        assert_eq!(
            store.append(&group, item("visible append")),
            Some(AppendId(0))
        );
        assert_eq!(
            store.append(&group, item("hidden append")),
            Some(AppendId(1))
        );
        assert!(store.delete_append(&group, AppendId(1)));

        let rows = store.rows(&group).collect::<Vec<_>>();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].target, StageTarget::Base(BaseIndex(0)));
        assert_eq!(rows[0].value.as_str(), "changed");
        assert_eq!(rows[0].status, StageStatus::Modified);
        assert_eq!(rows[1].target, StageTarget::Base(BaseIndex(1)));
        assert_eq!(rows[1].value.as_str(), "remove");
        assert_eq!(rows[1].status, StageStatus::Deleted);
        assert_eq!(rows[2].target, StageTarget::Append(AppendId(0)));
        assert_eq!(rows[2].value.as_str(), "visible append");
        assert_eq!(rows[2].status, StageStatus::Added);
    }

    #[test]
    fn materialize_merges_base_overlay_and_visible_appends_at_the_end() {
        let group = group();
        let mut store = store(&["alpha", "beta", "gamma"]);

        assert!(store.delete_base(&group, BaseIndex(0)));
        assert!(store.modify_base(&group, BaseIndex(1), item("beta edited")));
        assert_eq!(
            store.append(&group, item("hidden append")),
            Some(AppendId(0))
        );
        assert_eq!(
            store.append(&group, item("visible append")),
            Some(AppendId(1))
        );
        assert!(store.delete_append(&group, AppendId(0)));

        assert_eq!(
            store.materialize_group(&group),
            items(&["beta edited", "gamma", "visible append"]),
        );
        assert_eq!(
            store.materialize_all(),
            BTreeMap::from([(group, items(&["beta edited", "gamma", "visible append"]))]),
        );
    }

    #[test]
    fn revert_clears_overlay_appends_and_history() {
        let group = group();
        let mut store = store(&["base"]);

        assert!(store.modify_base(&group, BaseIndex(0), item("changed")));
        assert_eq!(store.append(&group, item("added")), Some(AppendId(0)));
        assert!(store.undo());

        assert!(store.revert());

        assert_eq!(store.materialize_group(&group), items(&["base"]));
        assert!(!store.is_dirty());
        assert!(!store.can_undo());
        assert!(!store.can_redo());
        assert_eq!(store.append(&group, item("new append")), Some(AppendId(0)));
    }

    fn group() -> TestGroup {
        TestGroup::Extensions
    }

    fn store(base: &[&str]) -> StageStore<TestGroup, String> {
        StageStore::new([(group(), items(base))])
    }

    fn item(value: &str) -> String {
        value.to_owned()
    }

    fn items(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| item(value)).collect()
    }

    fn rows(
        store: &StageStore<TestGroup, String>,
        group: TestGroup,
    ) -> Vec<(StageTarget, String, StageStatus)> {
        store
            .rows(&group)
            .map(|row| (row.target, row.value.clone(), row.status))
            .collect()
    }
}
