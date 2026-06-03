use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::chromium::Browser;
use crate::chromium::policy::{PolicySet, PolicyValue};
use crate::diff::{self, DiffStatus};
use crate::manifest::{EXTENSION_INSTALL_FORCELIST, Manifest};
use crate::policy_stage::{
    PolicyGroupId,
    PolicyItem,
    PolicyItemRow,
    PolicyItemStatus,
    PolicyStage,
    StageTarget,
};

pub(crate) const CUSTOM_GROUP: &str = "Custom";

#[derive(Debug)]
pub(crate) struct PolicyTree {
    rows: Vec<PolicyTreeRow>,
}

#[derive(Debug)]
pub(crate) struct PolicyTreeRow {
    pub(crate) kind: PolicyTreeRowKind,
    id: RowId,
    search_text: String,
}

#[derive(Debug)]
pub(crate) enum PolicyTreeRowKind {
    Group {
        title: String,
        status: GroupStatus,
    },
    Policy {
        indent: usize,
        key: String,
        value: PolicyValueSummary,
        status: RowStatus,
    },
    Value {
        indent: usize,
        value: PolicyValueSummary,
        status: RowStatus,
        extension_name: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyValueSummary {
    kind: PolicyValueKind,
    policy_label: Cow<'static, str>,
    child_label: Cow<'static, str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyValueKind {
    Bool,
    Integer,
    String,
    List,
    Object,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupStatus {
    All,
    Some,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowStatus {
    Applied,
    Added,
    Edited,
    Deleted,
    NotApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditableValueKind {
    Integer,
    String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditablePolicyValue {
    pub(crate) kind: EditableValueKind,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewListItemTarget {
    pub(crate) insert_after: RowId,
    pub(crate) indent: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowId(RowTarget);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowTarget {
    Group(PolicyGroupId),
    Policy(PolicyRowId),
    Path {
        policy: PolicyRowId,
        path: RowPath,
    },
    ListItem {
        policy: PolicyRowId,
        path: RowPath,
        current_index: Option<usize>,
        restore: Option<Arc<ListRestore>>,
    },
    Display {
        policy: PolicyRowId,
        path: RowPath,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyRowId {
    group: PolicyGroupId,
    target: StageTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Field(Arc<str>),
}

type RowPath = Arc<[PathSegment]>;

#[derive(Clone, Copy)]
struct BuildContext<'a> {
    manifest: &'a Manifest,
    browser: Browser,
    defaults: Option<&'a PolicySet>,
}

struct ChildRows<'a> {
    policy: PolicyRowId,
    top_key: &'a str,
    indent: usize,
    path: RowPath,
    values: PolicyValues<'a>,
}

#[derive(Clone, Copy)]
struct PolicyValues<'a> {
    baseline: Option<&'a PolicyValue>,
    current: Option<&'a PolicyValue>,
    default: Option<&'a PolicyValue>,
}

struct DisplayListItem<'a> {
    value: &'a PolicyValue,
    status: RowStatus,
    current_index: Option<usize>,
    restore: Option<Arc<ListRestore>>,
}

#[derive(Debug, Clone)]
struct ListRestore {
    source: RestoreSource,
    index: usize,
    value: Arc<PolicyValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreSource {
    Baseline,
    Default,
}

impl PartialEq for ListRestore {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source && self.index == other.index
    }
}

impl Eq for ListRestore {}

struct ObjectChildSpec<'a> {
    key: &'a str,
    values: PolicyValues<'a>,
    status: RowStatus,
}

impl PolicyTree {
    pub(crate) fn build(
        manifest: &Manifest,
        browser: Browser,
        stage: &PolicyStage,
        defaults: Option<&PolicySet>,
    ) -> Self {
        let context = BuildContext {
            manifest,
            browser,
            defaults,
        };
        let mut rows = Vec::new();

        push_custom_rows(&mut rows, context, stage);
        for group in manifest.policy_groups(browser) {
            let group_id = PolicyGroupId::manifest(&group.id);
            rows.push(PolicyTreeRow::group(
                group.name.clone(),
                group_status(stage.rows(&group_id)),
                RowTarget::Group(group_id.clone()),
            ));
            for row in stage.rows(&group_id) {
                push_policy_row(&mut rows, context, row);
            }
        }

        Self { rows }
    }

    pub(crate) fn rows(&self) -> &[PolicyTreeRow] {
        &self.rows
    }

    pub(crate) fn row(&self, id: &RowId) -> Option<&PolicyTreeRow> {
        self.rows.iter().find(|row| row.id() == id)
    }

    pub(crate) fn row_index(&self, id: &RowId) -> Option<usize> {
        self.rows.iter().position(|row| row.id() == id)
    }

    pub(crate) fn visible_indices(&self, query: &str) -> Vec<usize> {
        let Some(filter) = PolicyFilter::new(query) else {
            return (0..self.rows.len()).collect();
        };

        filter.visible_indices(&self.rows)
    }

    pub(crate) fn group_cursor(&self, cursor: &RowId, delta: i16) -> Option<RowId> {
        let cursor_index = self.row_index(cursor)?;
        let mut first = None;
        let mut last = None;
        let mut before = None;
        let mut after = None;

        for (index, row) in self.rows.iter().enumerate() {
            if !row.is_group() {
                continue;
            }

            first.get_or_insert(index);
            last = Some(index);
            if index < cursor_index {
                before = Some(index);
            } else if index > cursor_index && after.is_none() {
                after = Some(index);
            }
        }

        let next_index = if delta.is_negative() {
            before.or(last)
        } else {
            after.or(first)
        }?;

        self.rows.get(next_index).map(|row| row.id().clone())
    }

    pub(crate) fn filtered_group_cursor(
        &self,
        query: &str,
        cursor: &RowId,
        delta: i16,
    ) -> Option<RowId> {
        let cursor_index = self.row_index(cursor)?;
        let visible = self.visible_indices(query);
        let next_index = if delta.is_negative() {
            visible.into_iter().rev().find(|index| {
                *index < cursor_index && self.rows.get(*index).is_some_and(PolicyTreeRow::is_group)
            })
        } else {
            visible.into_iter().find(|index| {
                *index > cursor_index && self.rows.get(*index).is_some_and(PolicyTreeRow::is_group)
            })
        }?;

        self.rows.get(next_index).map(|row| row.id().clone())
    }
}

pub(crate) fn remove_at(stage: &mut PolicyStage, target: &RowId) -> bool {
    match target.target() {
        RowTarget::Policy(policy) => stage.delete_row(&policy.group, policy.target),
        RowTarget::Path { policy, path } => update_policy_value(stage, policy, |value| {
            remove_path(value, path).then_some(())
        }),
        RowTarget::ListItem {
            policy,
            path,
            current_index: Some(index),
            ..
        } => update_policy_value(stage, policy, |value| {
            remove_list_item(value, path, *index).then_some(())
        }),
        RowTarget::Group(_)
        | RowTarget::Display { .. }
        | RowTarget::ListItem {
            current_index: None,
            ..
        } => false,
    }
}

pub(crate) fn remove_group_at(stage: &mut PolicyStage, tree: &PolicyTree, cursor: &RowId) -> bool {
    let RowTarget::Group(group) = cursor.target() else {
        return false;
    };

    let targets = group_policy_targets(tree, group);
    stage.delete_rows(targets)
}

pub(crate) fn toggle_group_at(stage: &mut PolicyStage, tree: &PolicyTree, cursor: &RowId) -> bool {
    let RowTarget::Group(group) = cursor.target() else {
        return false;
    };
    let mut count = 0usize;
    let mut applied = 0usize;
    let mut targets = Vec::new();
    for row in tree.rows() {
        let RowTarget::Policy(policy) = row.target() else {
            continue;
        };
        if &policy.group != group {
            continue;
        }

        count += 1;
        applied += usize::from(matches!(
            row.kind,
            PolicyTreeRowKind::Policy {
                status: RowStatus::Applied | RowStatus::Edited | RowStatus::Added,
                ..
            }
        ));
        targets.push((policy.group.clone(), policy.target));
    }
    if count == 0 {
        return false;
    }

    let apply = applied != count;
    stage.set_rows_applied(targets, apply)
}

fn group_policy_targets(
    tree: &PolicyTree,
    group: &PolicyGroupId,
) -> Vec<(PolicyGroupId, StageTarget)> {
    tree.rows()
        .iter()
        .filter_map(|row| match row.target() {
            RowTarget::Policy(policy) if &policy.group == group => {
                Some((policy.group.clone(), policy.target))
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn toggle_policy_at(stage: &mut PolicyStage, target: &RowId) -> bool {
    match target.target() {
        RowTarget::Policy(policy) => {
            if stage.value_at(&policy.group, policy.target).is_none() {
                return stage.set_row_applied(&policy.group, policy.target, true);
            }
            stage.set_row_applied(&policy.group, policy.target, false)
        }
        RowTarget::ListItem {
            policy,
            path,
            current_index: Some(index),
            ..
        } => update_policy_value(stage, policy, |value| {
            remove_list_item(value, path, *index).then_some(())
        }),
        RowTarget::ListItem {
            policy,
            path,
            current_index: None,
            restore: Some(restore),
        } => update_policy_value(stage, policy, |value| {
            insert_list_item(value, path, restore.index, restore.value.as_ref().clone())
                .then_some(())
        }),
        RowTarget::ListItem {
            current_index: None,
            restore: None,
            ..
        } => false,
        RowTarget::Group(_) | RowTarget::Path { .. } | RowTarget::Display { .. } => false,
    }
}

pub(crate) fn toggle_bool_at(stage: &mut PolicyStage, cursor: &RowId) -> bool {
    let Some(value) = target_value(stage, cursor) else {
        return false;
    };
    let PolicyValue::Bool(value) = value else {
        return false;
    };
    let Some(policy) = policy_for_target(cursor.target()) else {
        return false;
    };

    set_policy_target_value(stage, policy, cursor.target(), PolicyValue::Bool(!value))
}

pub(crate) fn editable_value_at(
    stage: &PolicyStage,
    cursor: &RowId,
) -> Option<EditablePolicyValue> {
    if matches!(
        cursor.target(),
        RowTarget::ListItem {
            current_index: None,
            ..
        }
    ) {
        return None;
    }

    editable_value(target_value(stage, cursor)?)
}

pub(crate) fn set_value_at(stage: &mut PolicyStage, cursor: &RowId, value: PolicyValue) -> bool {
    let Some(policy) = policy_for_target(cursor.target()) else {
        return false;
    };

    set_policy_target_value(stage, policy, cursor.target(), value)
}

pub(crate) fn new_list_item_target_at(
    tree: &PolicyTree,
    stage: &PolicyStage,
    cursor: &RowId,
) -> Option<NewListItemTarget> {
    let policy = policy_for_target(cursor.target())?;
    let value = target_value(stage, cursor)?;
    if !matches!(value, PolicyValue::List(_))
        && !matches!(cursor.target(), RowTarget::ListItem { .. })
    {
        return None;
    }

    let path = list_path_ref(cursor.target())?;
    let insert_after = last_list_item_id(tree, policy, path).unwrap_or_else(|| cursor.clone());
    let indent = list_item_indent(cursor.target())?;

    Some(NewListItemTarget {
        insert_after,
        indent,
    })
}

pub(crate) fn add_list_item_value_at(
    tree: &PolicyTree,
    stage: &mut PolicyStage,
    cursor: &RowId,
    value: PolicyValue,
) -> Option<RowId> {
    target_value(stage, cursor)?;
    let policy = policy_for_target(cursor.target())?.clone();
    let path = list_path(cursor.target())?;
    let index = append_list_item(stage, &policy, path.as_ref(), value)?;
    let rebuilt_marker = list_item_id(tree, &policy, path.as_ref(), index);

    rebuilt_marker.or_else(|| {
        Some(RowId::new(RowTarget::ListItem {
            policy,
            path,
            current_index: Some(index),
            restore: None,
        }))
    })
}

pub(crate) fn key_cursor(tree: &PolicyTree, key: &str) -> Option<RowId> {
    tree.rows.iter().find_map(|row| {
        matches!(
            &row.kind,
            PolicyTreeRowKind::Policy {
                key: row_key,
                ..
            } if row_key == key
        )
        .then(|| row.id().clone())
    })
}

fn push_custom_rows(rows: &mut Vec<PolicyTreeRow>, context: BuildContext<'_>, stage: &PolicyStage) {
    let group_id = PolicyGroupId::Custom;
    let status = group_status(stage.rows(&group_id));
    if stage.rows(&group_id).next().is_none() {
        return;
    }

    rows.push(PolicyTreeRow::group(
        CUSTOM_GROUP.to_owned(),
        status,
        RowTarget::Group(group_id.clone()),
    ));
    for row in stage.rows(&group_id) {
        push_policy_row(rows, context, row);
    }
}

fn push_policy_row(
    rows: &mut Vec<PolicyTreeRow>,
    context: BuildContext<'_>,
    row: PolicyItemRow<'_>,
) {
    let policy = PolicyRowId {
        group: row.group.clone(),
        target: row.target,
    };
    let current = row.current.is_applied().then_some(row.current.value());
    let baseline = row
        .base
        .filter(|base| base.is_applied())
        .map(PolicyItem::value);
    let default = context
        .defaults
        .and_then(|defaults| defaults.get(row.current.key()))
        .or_else(|| (!row.current.is_applied()).then_some(row.current.value()));
    let value = current.or(baseline).or(default);
    let Some(value) = value else {
        return;
    };
    let summary = PolicyValueSummary::new(value);
    let search_text = policy_search_text(row.current.key(), &summary);

    rows.push(PolicyTreeRow {
        kind: PolicyTreeRowKind::Policy {
            indent: 1,
            key: row.current.key().to_owned(),
            value: summary,
            status: row.status.into(),
        },
        id: RowId::new(RowTarget::Policy(policy.clone())),
        search_text,
    });
    push_child_rows(
        rows,
        context,
        ChildRows {
            policy,
            top_key: row.current.key(),
            indent: 2,
            path: empty_path(),
            values: PolicyValues {
                baseline,
                current,
                default,
            },
        },
    );
}

fn push_child_rows(rows: &mut Vec<PolicyTreeRow>, context: BuildContext<'_>, child: ChildRows<'_>) {
    match (
        child.values.baseline,
        child.values.current,
        child.values.default,
    ) {
        (baseline, current, default)
            if value_is_list(baseline) || value_is_list(current) || value_is_list(default) =>
        {
            let baseline = baseline.and_then(as_list);
            let current = current.and_then(as_list);
            let default = default.and_then(as_list);

            for item in display_list_items(baseline, current, default) {
                rows.push(PolicyTreeRow::value_row(
                    child.indent,
                    item.value,
                    item.status,
                    extension_name(context.manifest, context.browser, child.top_key, item.value),
                    RowTarget::ListItem {
                        policy: child.policy.clone(),
                        path: Arc::clone(&child.path),
                        current_index: item.current_index,
                        restore: item.restore,
                    },
                ));
            }
        }
        (None, None, Some(PolicyValue::Object(default))) => {
            for (key, value) in default {
                push_object_child(
                    rows,
                    context,
                    &child,
                    ObjectChildSpec {
                        key,
                        values: PolicyValues {
                            baseline: None,
                            current: None,
                            default: Some(value),
                        },
                        status: RowStatus::NotApplied,
                    },
                );
            }
        }
        (baseline, current, default) if value_is_object(baseline) || value_is_object(current) => {
            let baseline = baseline.and_then(as_object);
            let current = current.and_then(as_object);
            let default = default.and_then(as_object);

            for key in visible_object_keys(baseline, current, default) {
                let baseline_value = baseline.and_then(|values| values.get(key));
                let current_value = current.and_then(|values| values.get(key));
                let default_value = default.and_then(|values| values.get(key));
                let status = row_status(PolicyValues {
                    baseline: baseline_value,
                    current: current_value,
                    default: default_value,
                });

                push_object_child(
                    rows,
                    context,
                    &child,
                    ObjectChildSpec {
                        key,
                        values: PolicyValues {
                            baseline: baseline_value,
                            current: current_value,
                            default: default_value,
                        },
                        status,
                    },
                );
            }
        }
        (_, _, _) => {}
    }
}

fn push_object_child(
    rows: &mut Vec<PolicyTreeRow>,
    context: BuildContext<'_>,
    child: &ChildRows<'_>,
    object: ObjectChildSpec<'_>,
) {
    let key = object.key;
    let values = object.values;
    let Some(value) = values.current.or(values.baseline).or(values.default) else {
        return;
    };
    let summary = PolicyValueSummary::new(value);
    let search_text = policy_search_text(key, &summary);
    let path = extend_path(&child.path, PathSegment::Field(Arc::from(key)));
    let target = if values.current.is_some() {
        RowTarget::Path {
            policy: child.policy.clone(),
            path: Arc::clone(&path),
        }
    } else {
        RowTarget::Display {
            policy: child.policy.clone(),
            path: Arc::clone(&path),
        }
    };

    rows.push(PolicyTreeRow {
        kind: PolicyTreeRowKind::Policy {
            indent: child.indent,
            key: key.to_owned(),
            value: summary,
            status: object.status,
        },
        id: RowId::new(target),
        search_text,
    });
    push_child_rows(
        rows,
        context,
        ChildRows {
            policy: child.policy.clone(),
            top_key: child.top_key,
            indent: child.indent + 1,
            path,
            values,
        },
    );
}

fn group_status<'a>(rows: impl IntoIterator<Item = PolicyItemRow<'a>>) -> GroupStatus {
    let mut total = 0usize;
    let mut selected = 0usize;
    for row in rows {
        total += 1;
        if matches!(
            row.status,
            PolicyItemStatus::Applied | PolicyItemStatus::Added | PolicyItemStatus::Edited
        ) {
            selected += 1;
        }
    }

    match (total, selected) {
        (0, _) | (_, 0) => GroupStatus::None,
        (total, selected) if total == selected => GroupStatus::All,
        (_, _) => GroupStatus::Some,
    }
}

fn row_status(values: PolicyValues<'_>) -> RowStatus {
    match (values.baseline, values.current, values.default) {
        (None, None, Some(_)) => RowStatus::NotApplied,
        (baseline, current, _) => diff::status(baseline, current).into(),
    }
}

fn display_list_items<'a>(
    baseline: Option<&'a [PolicyValue]>,
    current: Option<&'a [PolicyValue]>,
    default: Option<&'a [PolicyValue]>,
) -> Vec<DisplayListItem<'a>> {
    let mut items = diff::list_items(baseline, current)
        .into_iter()
        .map(|item| DisplayListItem {
            value: item.value,
            status: item.status.into(),
            current_index: item.current_index,
            restore: item.current_index.is_none().then(|| {
                Arc::new(ListRestore {
                    source: RestoreSource::Baseline,
                    index: item
                        .baseline_index
                        .expect("deleted list diff items always carry a baseline index"),
                    value: Arc::new(item.value.clone()),
                })
            }),
        })
        .collect::<Vec<_>>();
    let mut visible_counts = list_value_counts(items.iter().map(|item| item.value));

    for (default_index, default_value) in default.unwrap_or_default().iter().enumerate() {
        let count = visible_counts.entry(default_value).or_default();
        if *count > 0 {
            *count -= 1;
            continue;
        }

        items.push(DisplayListItem {
            value: default_value,
            status: RowStatus::NotApplied,
            current_index: None,
            restore: Some(Arc::new(ListRestore {
                source: RestoreSource::Default,
                index: default_index,
                value: Arc::new(default_value.clone()),
            })),
        });
    }

    items
}

fn list_value_counts<'a>(
    values: impl IntoIterator<Item = &'a PolicyValue>,
) -> BTreeMap<&'a PolicyValue, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }

    counts
}

fn target_value<'a>(stage: &'a PolicyStage, cursor: &'a RowId) -> Option<&'a PolicyValue> {
    let policy = policy_for_target(cursor.target())?;
    let policy_value = stage.item_value_at(&policy.group, policy.target)?;

    Some(match cursor.target() {
        RowTarget::Policy(_) => policy_value,
        RowTarget::Path { path, .. } | RowTarget::Display { path, .. } => {
            path_value(policy_value, path)?
        }
        RowTarget::ListItem {
            path,
            current_index: Some(index),
            ..
        } => list_parent(policy_value, path)?.get(*index)?,
        RowTarget::ListItem {
            current_index: None,
            restore: Some(restore),
            ..
        } => restore.value.as_ref(),
        RowTarget::ListItem {
            current_index: None,
            restore: None,
            ..
        } => return None,
        RowTarget::Group(_) => return None,
    })
}

fn set_policy_target_value(
    stage: &mut PolicyStage,
    policy: &PolicyRowId,
    target: &RowTarget,
    value: PolicyValue,
) -> bool {
    match target {
        RowTarget::Policy(_) => stage.set_row_value(&policy.group, policy.target, value),
        RowTarget::Path { path, .. } => update_policy_value(stage, policy, |current| {
            set_path(current, path, value).then_some(())
        }),
        RowTarget::ListItem {
            path,
            current_index: Some(index),
            ..
        } => update_policy_value(stage, policy, |current| {
            set_list_item(current, path, *index, value).then_some(())
        }),
        RowTarget::Group(_)
        | RowTarget::Display { .. }
        | RowTarget::ListItem {
            current_index: None,
            ..
        } => false,
    }
}

fn update_policy_value(
    stage: &mut PolicyStage,
    policy: &PolicyRowId,
    update: impl FnOnce(&mut PolicyValue) -> Option<()>,
) -> bool {
    let Some(value) = stage.item_value_at(&policy.group, policy.target) else {
        return false;
    };
    let mut updated = value.clone();
    if update(&mut updated).is_none() {
        return false;
    }

    stage.set_row_value(&policy.group, policy.target, updated)
}

fn append_list_item(
    stage: &mut PolicyStage,
    policy: &PolicyRowId,
    path: &[PathSegment],
    item: PolicyValue,
) -> Option<usize> {
    let mut index = None;
    update_policy_value(stage, policy, |value| {
        let values = list_parent_mut(value, path)?;
        index = Some(values.len());
        values.push(item);
        Some(())
    })
    .then_some(index)
    .flatten()
}

fn remove_path(value: &mut PolicyValue, path: &[PathSegment]) -> bool {
    let Some((segment, rest)) = path.split_first() else {
        return false;
    };

    match (segment, value, rest.is_empty()) {
        (PathSegment::Field(field), PolicyValue::Object(values), true) => {
            values.remove(field.as_ref()).is_some()
        }
        (PathSegment::Field(field), PolicyValue::Object(values), false) => values
            .get_mut(field.as_ref())
            .is_some_and(|value| remove_path(value, rest)),
        (_, _, _) => false,
    }
}

fn remove_list_item(value: &mut PolicyValue, path: &[PathSegment], index: usize) -> bool {
    let Some(values) = list_parent_mut(value, path) else {
        return false;
    };
    if index >= values.len() {
        return false;
    }

    values.remove(index);
    true
}

fn insert_list_item(
    value: &mut PolicyValue,
    path: &[PathSegment],
    index: usize,
    item: PolicyValue,
) -> bool {
    let Some(values) = list_parent_mut(value, path) else {
        return false;
    };

    values.insert(index.min(values.len()), item);
    true
}

fn set_list_item(
    value: &mut PolicyValue,
    path: &[PathSegment],
    index: usize,
    item: PolicyValue,
) -> bool {
    let Some(values) = list_parent_mut(value, path) else {
        return false;
    };
    let Some(value) = values.get_mut(index) else {
        return false;
    };

    *value = item;
    true
}

fn set_path(value: &mut PolicyValue, path: &[PathSegment], item: PolicyValue) -> bool {
    let Some((segment, rest)) = path.split_first() else {
        *value = item;
        return true;
    };

    match (segment, value, rest.is_empty()) {
        (PathSegment::Field(field), PolicyValue::Object(values), true) => {
            values.insert(field.to_string(), item);
            true
        }
        (PathSegment::Field(field), PolicyValue::Object(values), false) => values
            .get_mut(field.as_ref())
            .is_some_and(|value| set_path(value, rest, item)),
        (_, _, _) => false,
    }
}

fn list_parent_mut<'a>(
    value: &'a mut PolicyValue,
    path: &[PathSegment],
) -> Option<&'a mut Vec<PolicyValue>> {
    if path.is_empty() {
        return as_list_mut(value);
    }

    let parent = path_value_mut(value, path)?;
    as_list_mut(parent)
}

fn list_parent<'a>(value: &'a PolicyValue, path: &[PathSegment]) -> Option<&'a [PolicyValue]> {
    if path.is_empty() {
        return as_list(value);
    }

    path_value(value, path).and_then(as_list)
}

fn path_value<'a>(value: &'a PolicyValue, path: &[PathSegment]) -> Option<&'a PolicyValue> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(value);
    };

    match (segment, value) {
        (PathSegment::Field(field), PolicyValue::Object(values)) => values
            .get(field.as_ref())
            .and_then(|value| path_value(value, rest)),
        (_, _) => None,
    }
}

fn path_value_mut<'a>(
    value: &'a mut PolicyValue,
    path: &[PathSegment],
) -> Option<&'a mut PolicyValue> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(value);
    };

    match (segment, value) {
        (PathSegment::Field(field), PolicyValue::Object(values)) => values
            .get_mut(field.as_ref())
            .and_then(|value| path_value_mut(value, rest)),
        (_, _) => None,
    }
}

fn policy_for_target(target: &RowTarget) -> Option<&PolicyRowId> {
    match target {
        RowTarget::Policy(policy)
        | RowTarget::Path { policy, .. }
        | RowTarget::ListItem { policy, .. }
        | RowTarget::Display { policy, .. } => Some(policy),
        RowTarget::Group(_) => None,
    }
}

fn list_path(target: &RowTarget) -> Option<RowPath> {
    match target {
        RowTarget::Policy(_) => Some(empty_path()),
        RowTarget::Path { path, .. } | RowTarget::ListItem { path, .. } => Some(Arc::clone(path)),
        RowTarget::Group(_) | RowTarget::Display { .. } => None,
    }
}

fn empty_path() -> RowPath {
    Arc::from([])
}

fn extend_path(path: &[PathSegment], segment: PathSegment) -> RowPath {
    let mut extended = Vec::with_capacity(path.len() + 1);
    extended.extend_from_slice(path);
    extended.push(segment);

    Arc::from(extended)
}

fn list_path_ref(target: &RowTarget) -> Option<&[PathSegment]> {
    match target {
        RowTarget::Policy(_) => Some(&[]),
        RowTarget::Path { path, .. } | RowTarget::ListItem { path, .. } => Some(path.as_ref()),
        RowTarget::Group(_) | RowTarget::Display { .. } => None,
    }
}

fn list_item_indent(target: &RowTarget) -> Option<usize> {
    match target {
        RowTarget::Policy(_) => Some(2),
        RowTarget::Path { path, .. } | RowTarget::ListItem { path, .. } => Some(path.len() + 2),
        RowTarget::Group(_) | RowTarget::Display { .. } => None,
    }
}

fn last_list_item_id(
    tree: &PolicyTree,
    policy: &PolicyRowId,
    path: &[PathSegment],
) -> Option<RowId> {
    tree.rows.iter().rev().find_map(|row| {
        matches!(
            row.target(),
            RowTarget::ListItem {
                policy: row_policy,
                path: row_path,
                ..
            } if row_policy == policy && row_path.as_ref() == path
        )
        .then(|| row.id().clone())
    })
}

fn list_item_id(
    tree: &PolicyTree,
    policy: &PolicyRowId,
    path: &[PathSegment],
    current_index: usize,
) -> Option<RowId> {
    tree.rows.iter().find_map(|row| {
        matches!(
            row.target(),
            RowTarget::ListItem {
                policy: row_policy,
                path: row_path,
                current_index: Some(row_index),
                ..
            } if row_policy == policy && row_path.as_ref() == path && *row_index == current_index
        )
        .then(|| row.id().clone())
    })
}

fn editable_value(value: &PolicyValue) -> Option<EditablePolicyValue> {
    match value {
        PolicyValue::Integer(value) => Some(EditablePolicyValue {
            kind: EditableValueKind::Integer,
            value: value.to_string(),
        }),
        PolicyValue::String(value) => Some(EditablePolicyValue {
            kind: EditableValueKind::String,
            value: value.clone(),
        }),
        PolicyValue::Bool(_)
        | PolicyValue::List(_)
        | PolicyValue::Object(_)
        | PolicyValue::Null => None,
    }
}

fn extension_name<'a>(
    manifest: &'a Manifest,
    browser: Browser,
    top_key: &str,
    value: &PolicyValue,
) -> Option<&'a str> {
    if top_key != EXTENSION_INSTALL_FORCELIST {
        return None;
    }
    let PolicyValue::String(extension_id) = value else {
        return None;
    };

    manifest.extension_name(browser, extension_id)
}

fn visible_object_keys<'a>(
    baseline: Option<&'a PolicySet>,
    current: Option<&'a PolicySet>,
    default: Option<&'a PolicySet>,
) -> Vec<&'a str> {
    baseline
        .into_iter()
        .flat_map(|values| values.keys().map(String::as_str))
        .chain(
            current
                .into_iter()
                .flat_map(|values| values.keys().map(String::as_str)),
        )
        .chain(
            default
                .into_iter()
                .flat_map(|values| values.keys().map(String::as_str)),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn value_is_list(value: Option<&PolicyValue>) -> bool {
    matches!(value, Some(PolicyValue::List(_)))
}

fn value_is_object(value: Option<&PolicyValue>) -> bool {
    matches!(value, Some(PolicyValue::Object(_)))
}

fn as_list(value: &PolicyValue) -> Option<&[PolicyValue]> {
    match value {
        PolicyValue::List(values) => Some(values),
        PolicyValue::Bool(_)
        | PolicyValue::Integer(_)
        | PolicyValue::String(_)
        | PolicyValue::Object(_)
        | PolicyValue::Null => None,
    }
}

fn as_list_mut(value: &mut PolicyValue) -> Option<&mut Vec<PolicyValue>> {
    match value {
        PolicyValue::List(values) => Some(values),
        PolicyValue::Bool(_)
        | PolicyValue::Integer(_)
        | PolicyValue::String(_)
        | PolicyValue::Object(_)
        | PolicyValue::Null => None,
    }
}

fn as_object(value: &PolicyValue) -> Option<&PolicySet> {
    match value {
        PolicyValue::Object(values) => Some(values),
        PolicyValue::Bool(_)
        | PolicyValue::Integer(_)
        | PolicyValue::String(_)
        | PolicyValue::List(_)
        | PolicyValue::Null => None,
    }
}

fn policy_search_text(key: &str, value: &PolicyValueSummary) -> String {
    format!("{key}\n{}", value.search_label()).to_lowercase()
}

fn value_search_text(value: &PolicyValueSummary, extension_name: Option<&str>) -> String {
    match extension_name {
        Some(extension_name) => {
            format!("{}\n{extension_name}", value.search_label()).to_lowercase()
        }
        None => value.search_label().to_lowercase(),
    }
}

struct PolicyFilter {
    query: String,
}

impl PolicyFilter {
    fn new(query: &str) -> Option<Self> {
        let query = query.trim();
        (!query.is_empty()).then(|| Self {
            query: query.to_lowercase(),
        })
    }

    fn visible_indices(&self, rows: &[PolicyTreeRow]) -> Vec<usize> {
        let mut included = vec![false; rows.len()];
        let mut current_group = None;
        let mut group_matches = false;
        let mut policy_parents: Vec<Option<usize>> = Vec::new();
        let mut active_match_indent = None;

        for (index, row) in rows.iter().enumerate() {
            match &row.kind {
                PolicyTreeRowKind::Group { .. } => {
                    current_group = Some(index);
                    group_matches = self.matches(row);
                    policy_parents.clear();
                    active_match_indent = None;
                    included[index] = group_matches;
                }
                PolicyTreeRowKind::Policy { indent, .. } => {
                    truncate_parents(&mut policy_parents, *indent);
                    clear_inactive_match(&mut active_match_indent, *indent);
                    let own_match = self.matches(row);
                    let inherited_match = group_matches || active_match_indent.is_some();
                    if own_match || inherited_match {
                        included[index] = true;
                        include_context(&mut included, current_group, &policy_parents);
                    }
                    if own_match {
                        active_match_indent =
                            Some(active_match_indent.map_or(*indent, |active| active.min(*indent)));
                    }
                    set_parent(&mut policy_parents, *indent, index);
                }
                PolicyTreeRowKind::Value { indent, .. } => {
                    truncate_parents(&mut policy_parents, *indent);
                    clear_inactive_match(&mut active_match_indent, *indent);
                    let own_match = self.matches(row);
                    if own_match || group_matches || active_match_indent.is_some() {
                        included[index] = true;
                    }
                    if own_match {
                        include_context(&mut included, current_group, &policy_parents);
                    }
                }
            }
        }

        included
            .into_iter()
            .enumerate()
            .filter_map(|(index, included)| included.then_some(index))
            .collect()
    }

    fn matches(&self, row: &PolicyTreeRow) -> bool {
        row.search_text.contains(&self.query)
    }
}

fn truncate_parents(parents: &mut Vec<Option<usize>>, indent: usize) {
    if parents.len() > indent {
        parents.truncate(indent);
    }
}

fn set_parent(parents: &mut Vec<Option<usize>>, indent: usize, index: usize) {
    if parents.len() <= indent {
        parents.resize(indent + 1, None);
    }
    parents[indent] = Some(index);
}

fn clear_inactive_match(active_match_indent: &mut Option<usize>, indent: usize) {
    if active_match_indent.is_some_and(|active| active >= indent) {
        *active_match_indent = None;
    }
}

fn include_context(included: &mut [bool], group: Option<usize>, parents: &[Option<usize>]) {
    if let Some(group) = group {
        included[group] = true;
    }
    for parent in parents.iter().flatten() {
        included[*parent] = true;
    }
}

impl PolicyTreeRow {
    pub(crate) fn is_group(&self) -> bool {
        matches!(self.kind, PolicyTreeRowKind::Group { .. })
    }

    fn group(title: String, status: GroupStatus, target: RowTarget) -> Self {
        let search_text = title.to_lowercase();

        Self {
            kind: PolicyTreeRowKind::Group { title, status },
            id: RowId::new(target),
            search_text,
        }
    }

    pub(crate) const fn id(&self) -> &RowId {
        &self.id
    }

    const fn target(&self) -> &RowTarget {
        &self.id.0
    }

    fn value_row(
        indent: usize,
        value: &PolicyValue,
        status: RowStatus,
        extension_name: Option<&str>,
        target: RowTarget,
    ) -> Self {
        let value = PolicyValueSummary::new(value);
        let search_text = value_search_text(&value, extension_name);

        Self {
            kind: PolicyTreeRowKind::Value {
                indent,
                value,
                status,
                extension_name: extension_name.map(ToOwned::to_owned),
            },
            id: RowId::new(target),
            search_text,
        }
    }
}

impl RowId {
    const fn new(target: RowTarget) -> Self {
        Self(target)
    }

    const fn target(&self) -> &RowTarget {
        &self.0
    }
}

impl PolicyValueSummary {
    fn new(value: &PolicyValue) -> Self {
        let kind = PolicyValueKind::from(value);
        let (policy_label, child_label) = match value {
            PolicyValue::Bool(true) => (Cow::Borrowed("true"), Cow::Borrowed("true")),
            PolicyValue::Bool(false) => (Cow::Borrowed("false"), Cow::Borrowed("false")),
            PolicyValue::Integer(value) => {
                let label = value.to_string();
                (Cow::Owned(label.clone()), Cow::Owned(label))
            }
            PolicyValue::String(value) => {
                (Cow::Owned(value.clone()), Cow::Owned(format!("{value:?}")))
            }
            PolicyValue::List(values) => (
                Cow::Borrowed(""),
                Cow::Owned(format!("{} items", values.len())),
            ),
            PolicyValue::Object(values) => (
                Cow::Owned(values.len().to_string()),
                Cow::Owned(format!("{} keys", values.len())),
            ),
            PolicyValue::Null => (Cow::Borrowed("null"), Cow::Borrowed("null")),
        };

        Self {
            kind,
            policy_label,
            child_label,
        }
    }

    pub(crate) const fn is_list(&self) -> bool {
        matches!(self.kind, PolicyValueKind::List)
    }

    pub(crate) fn policy_label(&self) -> &str {
        self.policy_label.as_ref()
    }

    pub(crate) fn child_label(&self) -> &str {
        self.child_label.as_ref()
    }

    fn search_label(&self) -> &str {
        &self.child_label
    }
}

impl From<&PolicyValue> for PolicyValueKind {
    fn from(value: &PolicyValue) -> Self {
        match value {
            PolicyValue::Bool(_) => Self::Bool,
            PolicyValue::Integer(_) => Self::Integer,
            PolicyValue::String(_) => Self::String,
            PolicyValue::List(_) => Self::List,
            PolicyValue::Object(_) => Self::Object,
            PolicyValue::Null => Self::Null,
        }
    }
}

impl From<DiffStatus> for RowStatus {
    fn from(status: DiffStatus) -> Self {
        match status {
            DiffStatus::Applied => Self::Applied,
            DiffStatus::Added => Self::Added,
            DiffStatus::Edited => Self::Edited,
            DiffStatus::Deleted => Self::Deleted,
        }
    }
}

impl From<PolicyItemStatus> for RowStatus {
    fn from(status: PolicyItemStatus) -> Self {
        match status {
            PolicyItemStatus::Applied => Self::Applied,
            PolicyItemStatus::Added => Self::Added,
            PolicyItemStatus::Edited => Self::Edited,
            PolicyItemStatus::Deleted => Self::Deleted,
            PolicyItemStatus::NotApplied => Self::NotApplied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_indices_return_all_rows_for_blank_queries() {
        let tree = PolicyTree {
            rows: vec![
                group("Privacy"),
                policy(1, "HomepageLocation", string("https://example.com")),
                value(2, string("child")),
            ],
        };

        assert_eq!(tree.visible_indices(""), vec![0, 1, 2]);
        assert_eq!(tree.visible_indices(" \t\n "), vec![0, 1, 2]);
    }

    #[test]
    fn value_match_includes_ancestors_without_siblings() {
        let tree = PolicyTree {
            rows: vec![
                group("Security"),
                policy(1, "RootObject", object()),
                policy(2, "MatchingChild", object()),
                value(3, integer(1234)),
                value(3, integer(5678)),
                policy(2, "SiblingChild", object()),
                value(3, integer(9999)),
            ],
        };

        let visible = tree
            .visible_indices("1234")
            .into_iter()
            .map(|index| label(&tree.rows[index]))
            .collect::<Vec<_>>();

        assert_eq!(
            visible,
            vec![
                "group:Security",
                "policy:RootObject",
                "policy:MatchingChild",
                "value:1234",
            ],
        );
    }

    #[test]
    fn recommended_default_list_item_remains_visible_after_current_removal() -> anyhow::Result<()> {
        let manifest = Manifest::load()?;
        let browser = Browser::Chrome;
        let defaults = manifest.balanced_preset(browser);
        let Some(PolicyValue::List(default_extensions)) = defaults.get(EXTENSION_INSTALL_FORCELIST)
        else {
            return Err(anyhow::anyhow!("balanced preset should include extensions"));
        };
        let Some(removed_extension) = default_extensions.first().cloned() else {
            return Err(anyhow::anyhow!("balanced preset should include extensions"));
        };

        let mut current = defaults.clone();
        let Some(PolicyValue::List(current_extensions)) =
            current.get_mut(EXTENSION_INSTALL_FORCELIST)
        else {
            return Err(anyhow::anyhow!("balanced preset should include extensions"));
        };
        current_extensions.remove(0);

        let baseline = PolicySet::new();
        let stage = PolicyStage::with_current(&manifest, browser, &baseline, &current);
        let tree = PolicyTree::build(&manifest, browser, &stage, Some(&defaults));
        let removed_label = PolicyValueSummary::new(&removed_extension)
            .child_label()
            .to_owned();

        assert!(
            tree.rows.iter().any(|row| matches!(
                (&row.kind, row.target()),
                (
                    PolicyTreeRowKind::Value {
                        value,
                        status: RowStatus::NotApplied,
                        ..
                    },
                    RowTarget::ListItem {
                        current_index: None,
                        restore: Some(_),
                        ..
                    },
                ) if value.child_label() == removed_label
            )),
            "removed recommended extension should remain visible as a restorable row",
        );

        Ok(())
    }

    #[test]
    fn deleted_list_item_is_not_editable() -> anyhow::Result<()> {
        let manifest = Manifest::load()?;
        let browser = Browser::Chrome;
        let mut baseline = PolicySet::new();
        baseline.insert(
            EXTENSION_INSTALL_FORCELIST.to_owned(),
            PolicyValue::List(vec![string("removed-extension")]),
        );
        let mut current = PolicySet::new();
        current.insert(
            EXTENSION_INSTALL_FORCELIST.to_owned(),
            PolicyValue::List(vec![]),
        );

        let stage = PolicyStage::with_current(&manifest, browser, &baseline, &current);
        let tree = PolicyTree::build(&manifest, browser, &stage, None);
        let deleted = tree
            .rows
            .iter()
            .find(|row| {
                matches!(
                    (&row.kind, row.target()),
                    (
                        PolicyTreeRowKind::Value {
                            status: RowStatus::Deleted,
                            ..
                        },
                        RowTarget::ListItem {
                            current_index: None,
                            restore: Some(_),
                            ..
                        },
                    )
                )
            })
            .map(|row| row.id().clone())
            .ok_or_else(|| anyhow::anyhow!("deleted list item should be visible"))?;

        assert!(editable_value_at(&stage, &deleted).is_none());

        Ok(())
    }

    fn group(title: &str) -> PolicyTreeRow {
        PolicyTreeRow::group(
            title.to_owned(),
            GroupStatus::None,
            RowTarget::Group(PolicyGroupId::manifest(title)),
        )
    }

    fn policy(indent: usize, key: &str, raw_value: PolicyValue) -> PolicyTreeRow {
        let value = PolicyValueSummary::new(&raw_value);
        PolicyTreeRow {
            kind: PolicyTreeRowKind::Policy {
                indent,
                key: key.to_owned(),
                value: value.clone(),
                status: RowStatus::NotApplied,
            },
            id: RowId::new(RowTarget::Policy(PolicyRowId {
                group: PolicyGroupId::Custom,
                target: StageTarget::Base(crate::policy_stage::BaseIndex(0)),
            })),
            search_text: policy_search_text(key, &value),
        }
    }

    fn value(indent: usize, raw_value: PolicyValue) -> PolicyTreeRow {
        PolicyTreeRow::value_row(
            indent,
            &raw_value,
            RowStatus::NotApplied,
            None,
            RowTarget::ListItem {
                policy: PolicyRowId {
                    group: PolicyGroupId::Custom,
                    target: StageTarget::Base(crate::policy_stage::BaseIndex(0)),
                },
                path: empty_path(),
                current_index: Some(indent),
                restore: None,
            },
        )
    }

    fn label(row: &PolicyTreeRow) -> String {
        match &row.kind {
            PolicyTreeRowKind::Group { title, .. } => format!("group:{title}"),
            PolicyTreeRowKind::Policy { key, .. } => format!("policy:{key}"),
            PolicyTreeRowKind::Value { value, .. } => {
                format!("value:{}", value.child_label().trim_matches('"'))
            }
        }
    }

    fn integer(value: i64) -> PolicyValue {
        PolicyValue::Integer(value)
    }

    fn object() -> PolicyValue {
        PolicyValue::Object(BTreeMap::new())
    }

    fn string(value: &str) -> PolicyValue {
        PolicyValue::String(value.to_owned())
    }
}
