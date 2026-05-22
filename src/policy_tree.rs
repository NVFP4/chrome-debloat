use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::chromium::Browser;
use crate::chromium::policy::{PolicySet, PolicyValue};
use crate::diff::{self, DiffStatus};
use crate::manifest::{EXTENSION_INSTALL_FORCELIST, Manifest, PolicyGroup, PolicySetting};

pub(crate) const CUSTOM_GROUP: &str = "Custom";

#[derive(Debug)]
pub(crate) struct PolicyTree {
    rows: Vec<PolicyTreeRow>,
}

#[derive(Debug)]
pub(crate) struct PolicyTreeRow {
    pub kind: PolicyTreeRowKind,
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
    policy_label: String,
    child_label: String,
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
    pub kind: EditableValueKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyValueUpdate {
    pub target: RowId,
    pub value: PolicyValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicySetCursor {
    pub policies: PolicySet,
    pub cursor: RowId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewListItemTarget {
    pub insert_after: RowId,
    pub indent: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowId(RowTarget);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowTarget {
    Group(GroupTarget),
    Policy {
        key: String,
    },
    Path {
        key: String,
        path: Vec<PathSegment>,
    },
    ListItem {
        key: String,
        path: Vec<PathSegment>,
        current_index: Option<usize>,
        restore_index: usize,
    },
    Display {
        key: String,
        path: Vec<PathSegment>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupTarget {
    Custom,
    Manifest(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Field(String),
}

#[derive(Debug, Clone)]
struct ListAddTarget {
    key: String,
    path: Vec<PathSegment>,
    fallback_value: Option<PolicyValue>,
}

struct OrderedGroup<'a> {
    group: &'a PolicyGroup,
    settings: Vec<&'a PolicySetting>,
}

#[derive(Clone, Copy)]
struct BuildContext<'a> {
    manifest: &'a Manifest,
    browser: Browser,
}

struct ChildRows<'a, 'k> {
    indent: usize,
    top_key: &'k str,
    path: Vec<PathSegment>,
    values: PolicyValues<'a>,
}

struct ListItemInsert<'a> {
    key: &'a str,
    path: &'a [PathSegment],
    baseline_values: Option<&'a [PolicyValue]>,
    index: usize,
    value: PolicyValue,
}

#[derive(Clone, Copy)]
struct PolicyValues<'a> {
    baseline: Option<&'a PolicyValue>,
    current: Option<&'a PolicyValue>,
    default: Option<&'a PolicyValue>,
}

impl PolicyTree {
    pub(crate) fn build(
        manifest: &Manifest,
        browser: Browser,
        baseline: &PolicySet,
        current: &PolicySet,
    ) -> Self {
        let groups = ordered_groups(manifest, browser);
        let active_keys = active_group_keys(manifest, browser);
        let custom_keys = custom_keys(&active_keys, baseline, current);
        let context = BuildContext { manifest, browser };
        let mut rows = Vec::new();

        if !custom_keys.is_empty() {
            rows.push(PolicyTreeRow::group(
                CUSTOM_GROUP.to_owned(),
                custom_group_status(&custom_keys, current),
                RowTarget::Group(GroupTarget::Custom),
            ));
            for key in custom_keys {
                push_policy_rows(
                    &mut rows,
                    context,
                    1,
                    &key,
                    PolicyValues {
                        baseline: baseline.get(&key),
                        current: current.get(&key),
                        default: None,
                    },
                );
            }
        }

        for ordered_group in groups {
            rows.push(PolicyTreeRow::group(
                ordered_group.group.name.clone(),
                manifest_group_status(&ordered_group.settings, current),
                RowTarget::Group(GroupTarget::Manifest(ordered_group.group.id.clone())),
            ));

            for setting in ordered_group.settings {
                push_policy_rows(
                    &mut rows,
                    context,
                    1,
                    &setting.key,
                    PolicyValues {
                        baseline: baseline.get(&setting.key),
                        current: current.get(&setting.key),
                        default: Some(&setting.value),
                    },
                );
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
        let group_rows = group_row_indexes(self);
        let cursor_index = self.row_index(cursor)?;
        let next_index = if delta.is_negative() {
            group_rows
                .iter()
                .rev()
                .copied()
                .find(|row| *row < cursor_index)
                .or_else(|| group_rows.last().copied())
        } else {
            group_rows
                .iter()
                .copied()
                .find(|row| *row > cursor_index)
                .or_else(|| group_rows.first().copied())
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
        let group_rows: Vec<usize> = self
            .visible_indices(query)
            .into_iter()
            .filter(|index| self.rows.get(*index).is_some_and(PolicyTreeRow::is_group))
            .collect();
        let next_index = if delta.is_negative() {
            group_rows
                .iter()
                .rev()
                .copied()
                .find(|index| *index < cursor_index)
        } else {
            group_rows
                .iter()
                .copied()
                .find(|index| *index > cursor_index)
        }?;

        self.rows.get(next_index).map(|row| row.id().clone())
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
        let mut matched_subtrees: Vec<usize> = Vec::new();

        for (index, row) in rows.iter().enumerate() {
            match &row.kind {
                PolicyTreeRowKind::Group { .. } => {
                    current_group = Some(index);
                    group_matches = self.matches(row);
                    policy_parents.clear();
                    matched_subtrees.clear();

                    included[index] = group_matches;
                }
                PolicyTreeRowKind::Policy { indent, .. } => {
                    truncate_parents(&mut policy_parents, *indent);
                    retain_active_subtrees(&mut matched_subtrees, *indent);

                    let own_match = self.matches(row);
                    let inherited_match = group_matches || !matched_subtrees.is_empty();
                    if own_match || inherited_match {
                        included[index] = true;
                        include_context(&mut included, current_group, &policy_parents);
                    }
                    if own_match {
                        matched_subtrees.push(*indent);
                    }

                    set_parent(&mut policy_parents, *indent, index);
                }
                PolicyTreeRowKind::Value { indent, .. } => {
                    truncate_parents(&mut policy_parents, *indent);
                    retain_active_subtrees(&mut matched_subtrees, *indent);

                    let own_match = self.matches(row);
                    if own_match || group_matches || !matched_subtrees.is_empty() {
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

fn retain_active_subtrees(subtrees: &mut Vec<usize>, indent: usize) {
    subtrees.retain(|subtree_indent| *subtree_indent < indent);
}

fn include_context(included: &mut [bool], group: Option<usize>, parents: &[Option<usize>]) {
    if let Some(group) = group {
        included[group] = true;
    }
    for parent in parents.iter().flatten() {
        included[*parent] = true;
    }
}

pub(crate) fn remove_at(current: &PolicySet, target: &RowId) -> Option<PolicySet> {
    let target = target.target().clone();
    let mut updated = current.clone();

    match target {
        RowTarget::Policy { key } => {
            updated.remove(&key)?;
        }
        RowTarget::Path { key, path } => {
            let value = updated.get_mut(&key)?;
            if !remove_path(value, &path) {
                return None;
            }
        }
        RowTarget::ListItem {
            key,
            path,
            current_index: Some(index),
            ..
        } => {
            if !remove_list_item(&mut updated, &key, &path, index) {
                return None;
            }
        }
        RowTarget::Group(_) | RowTarget::Display { .. } => return None,
        RowTarget::ListItem {
            current_index: None,
            ..
        } => return None,
    }

    (updated != *current).then_some(updated)
}

pub(crate) fn toggle_group_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    cursor: &RowId,
) -> Option<PolicySet> {
    let target = match cursor.target() {
        RowTarget::Group(target) => target.clone(),
        RowTarget::Policy { .. }
        | RowTarget::Path { .. }
        | RowTarget::ListItem { .. }
        | RowTarget::Display { .. } => return None,
    };

    let updated = match target {
        GroupTarget::Custom => toggle_custom_group(manifest, browser, baseline, current),
        GroupTarget::Manifest(group_id) => {
            toggle_manifest_group(manifest, browser, baseline, current, &group_id)
        }
    };

    (updated != *current).then_some(updated)
}

pub(crate) fn toggle_policy_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    cursor: &RowId,
) -> Option<PolicySet> {
    let target = cursor.target();
    let mut updated = current.clone();

    match target {
        RowTarget::Policy { key } if current.contains_key(key) => {
            updated.remove(key);
        }
        RowTarget::Policy { key } => {
            updated.insert(
                key.clone(),
                target_policy_value(manifest, browser, baseline, current, target)?.clone(),
            );
        }
        RowTarget::ListItem {
            key,
            path,
            current_index: Some(index),
            ..
        } => {
            remove_list_item(&mut updated, key, path, *index);
        }
        RowTarget::ListItem {
            key,
            path,
            current_index: None,
            restore_index,
        } => {
            let baseline_values = list_parent(baseline, key, path);
            insert_list_item(
                &mut updated,
                ListItemInsert {
                    key,
                    path,
                    baseline_values,
                    index: *restore_index,
                    value: target_policy_value(manifest, browser, baseline, current, target)?
                        .clone(),
                },
            );
        }
        RowTarget::Group(_) | RowTarget::Path { .. } | RowTarget::Display { .. } => return None,
    }

    (updated != *current).then_some(updated)
}

pub(crate) fn toggle_bool_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    cursor: &RowId,
) -> Option<PolicySet> {
    let target = cursor.target();
    let PolicyValue::Bool(value) =
        target_policy_value(manifest, browser, baseline, current, target)?
    else {
        return None;
    };

    let mut updated = current.clone();

    set_target_value(&mut updated, target, PolicyValue::Bool(!value))
        .then_some(updated)
        .filter(|updated| updated != current)
}

pub(crate) fn editable_value_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    cursor: &RowId,
) -> Option<EditablePolicyValue> {
    let target = cursor.target();

    if !target.is_value_target() {
        return None;
    }

    editable_value(target_policy_value(
        manifest, browser, baseline, current, target,
    )?)
}

pub(crate) fn set_value_at(current: &PolicySet, update: PolicyValueUpdate) -> Option<PolicySet> {
    let target = update.target.target();
    let mut updated = current.clone();

    set_target_value(&mut updated, target, update.value)
        .then_some(updated)
        .filter(|updated| updated != current)
}

pub(crate) fn new_list_item_target_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    cursor: &RowId,
) -> Option<NewListItemTarget> {
    let target = list_add_target(manifest, browser, baseline, current, cursor.target())?;
    let indent = list_item_indent(cursor.target())?;
    let tree = PolicyTree::build(manifest, browser, baseline, current);
    let insert_after =
        last_list_item_id(&tree, &target.key, &target.path).unwrap_or_else(|| cursor.clone());

    Some(NewListItemTarget {
        insert_after,
        indent,
    })
}

pub(crate) fn add_list_item_value_at(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    update: PolicyValueUpdate,
) -> Option<PolicySetCursor> {
    let target = list_add_target(manifest, browser, baseline, current, update.target.target())?;
    let mut updated = current.clone();
    let index = insert_list_item_value(&mut updated, &target, update.value)?;

    if updated == *current {
        return None;
    }

    let tree = PolicyTree::build(manifest, browser, baseline, &updated);
    let cursor = list_item_id(&tree, &target.key, &target.path, index)?;

    Some(PolicySetCursor {
        policies: updated,
        cursor,
    })
}

pub(crate) fn key_cursor(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    key: &str,
) -> Option<RowId> {
    let tree = PolicyTree::build(manifest, browser, baseline, current);

    tree.rows.iter().find_map(|row| {
        matches!(row.target(), RowTarget::Policy { key: row_key } if row_key == key)
            .then(|| row.id().clone())
    })
}

fn ordered_groups(manifest: &Manifest, browser: Browser) -> Vec<OrderedGroup<'_>> {
    let mut claimed_keys = BTreeSet::new();

    manifest
        .policy_groups(browser)
        .map(|group| {
            let settings = group
                .settings
                .iter()
                .filter(|setting| claimed_keys.insert(setting.key.as_str()))
                .collect();

            OrderedGroup { group, settings }
        })
        .collect()
}

fn active_group_keys(manifest: &Manifest, browser: Browser) -> BTreeSet<&str> {
    manifest
        .policy_groups(browser)
        .flat_map(|group| group.settings.iter().map(|setting| setting.key.as_str()))
        .collect()
}

fn custom_keys(
    active_group_keys: &BTreeSet<&str>,
    baseline: &PolicySet,
    current: &PolicySet,
) -> Vec<String> {
    diff::visible_policy_keys(baseline, current)
        .into_iter()
        .filter(|key| !active_group_keys.contains(key))
        .map(ToOwned::to_owned)
        .collect()
}

fn group_row_indexes(tree: &PolicyTree) -> Vec<usize> {
    tree.rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| matches!(row.target(), RowTarget::Group(_)).then_some(index))
        .collect()
}

fn list_item_indent(target: &RowTarget) -> Option<usize> {
    match target {
        RowTarget::Policy { .. } => Some(2),
        RowTarget::Path { path, .. } | RowTarget::ListItem { path, .. } => Some(path.len() + 2),
        RowTarget::Group(_) | RowTarget::Display { .. } => None,
    }
}

fn last_list_item_id(tree: &PolicyTree, key: &str, path: &[PathSegment]) -> Option<RowId> {
    tree.rows.iter().rev().find_map(|row| {
        matches!(
            row.target(),
            RowTarget::ListItem {
                key: row_key,
                path: row_path,
                ..
            } if row_key == key && row_path.as_slice() == path
        )
        .then(|| row.id().clone())
    })
}

fn list_item_id(
    tree: &PolicyTree,
    key: &str,
    path: &[PathSegment],
    current_index: usize,
) -> Option<RowId> {
    tree.rows.iter().find_map(|row| {
        matches!(
            row.target(),
            RowTarget::ListItem {
                key: row_key,
                path: row_path,
                current_index: Some(row_index),
                ..
            } if row_key == key && row_path.as_slice() == path && *row_index == current_index
        )
        .then(|| row.id().clone())
    })
}

fn list_add_target(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    target: &RowTarget,
) -> Option<ListAddTarget> {
    let row_value = target_policy_value(manifest, browser, baseline, current, target)?;
    let fallback_value = |key: &str| {
        current
            .get(key)
            .or_else(|| baseline.get(key))
            .or_else(|| manifest_policy_value(manifest, browser, key))
            .cloned()
    };

    match (target, row_value) {
        (RowTarget::Policy { key }, PolicyValue::List(values)) => Some(ListAddTarget {
            key: key.clone(),
            path: Vec::new(),
            fallback_value: Some(PolicyValue::List(values.clone())),
        }),
        (RowTarget::Path { key, path }, PolicyValue::List(_)) => Some(ListAddTarget {
            key: key.clone(),
            path: path.clone(),
            fallback_value: fallback_value(key),
        }),
        (RowTarget::ListItem { key, path, .. }, _) => Some(ListAddTarget {
            key: key.clone(),
            path: path.clone(),
            fallback_value: fallback_value(key),
        }),
        (
            RowTarget::Group(_)
            | RowTarget::Policy { .. }
            | RowTarget::Path { .. }
            | RowTarget::Display { .. },
            _,
        ) => None,
    }
}

fn manifest_policy_value<'a>(
    manifest: &'a Manifest,
    browser: Browser,
    key: &str,
) -> Option<&'a PolicyValue> {
    manifest
        .policy_groups(browser)
        .flat_map(|group| group.settings.iter())
        .find(|setting| setting.key == key)
        .map(|setting| &setting.value)
}

fn target_policy_value<'a>(
    manifest: &'a Manifest,
    browser: Browser,
    baseline: &'a PolicySet,
    current: &'a PolicySet,
    target: &RowTarget,
) -> Option<&'a PolicyValue> {
    match target {
        RowTarget::Policy { key } => current
            .get(key)
            .or_else(|| baseline.get(key))
            .or_else(|| manifest_policy_value(manifest, browser, key)),
        RowTarget::Path { key, path } | RowTarget::Display { key, path } => {
            policy_path_value(current, key, path)
                .or_else(|| policy_path_value(baseline, key, path))
                .or_else(|| manifest_path_value(manifest, browser, key, path))
        }
        RowTarget::ListItem {
            key,
            path,
            current_index: Some(index),
            ..
        } => list_parent(current, key, path)?.get(*index),
        RowTarget::ListItem {
            key,
            path,
            current_index: None,
            restore_index,
        } => list_parent(baseline, key, path)
            .or_else(|| manifest_list_parent(manifest, browser, key, path))?
            .get(*restore_index),
        RowTarget::Group(_) => None,
    }
}

fn policy_path_value<'a>(
    policies: &'a PolicySet,
    key: &str,
    path: &[PathSegment],
) -> Option<&'a PolicyValue> {
    path_value(policies.get(key)?, path)
}

fn manifest_path_value<'a>(
    manifest: &'a Manifest,
    browser: Browser,
    key: &str,
    path: &[PathSegment],
) -> Option<&'a PolicyValue> {
    path_value(manifest_policy_value(manifest, browser, key)?, path)
}

fn manifest_list_parent<'a>(
    manifest: &'a Manifest,
    browser: Browser,
    key: &str,
    path: &[PathSegment],
) -> Option<&'a [PolicyValue]> {
    manifest_path_value(manifest, browser, key, path)?.as_list()
}

fn path_value<'a>(value: &'a PolicyValue, path: &[PathSegment]) -> Option<&'a PolicyValue> {
    let Some((segment, rest)) = path.split_first() else {
        return Some(value);
    };

    match (segment, value) {
        (PathSegment::Field(field), PolicyValue::Object(values)) => {
            values.get(field).and_then(|value| path_value(value, rest))
        }
        (_, _) => None,
    }
}

fn manifest_group_status(settings: &[&PolicySetting], current: &PolicySet) -> GroupStatus {
    group_status(
        settings.len(),
        settings
            .iter()
            .filter(|setting| current.contains_key(&setting.key))
            .count(),
    )
}

fn custom_group_status(keys: &[String], current: &PolicySet) -> GroupStatus {
    group_status(
        keys.len(),
        keys.iter().filter(|key| current.contains_key(*key)).count(),
    )
}

fn group_status(total: usize, selected: usize) -> GroupStatus {
    match (total, selected) {
        (0, _) | (_, 0) => GroupStatus::None,
        (total, selected) if total == selected => GroupStatus::All,
        (_, _) => GroupStatus::Some,
    }
}

fn push_policy_rows<'a>(
    rows: &mut Vec<PolicyTreeRow>,
    context: BuildContext<'a>,
    indent: usize,
    key: &str,
    values: PolicyValues<'a>,
) {
    let Some(value) = values.current.or(values.baseline).or(values.default) else {
        return;
    };
    let status = row_status(values);
    let path = Vec::new();
    let value_summary = PolicyValueSummary::new(value);
    let search_text = policy_search_text(key, &value_summary);

    rows.push(PolicyTreeRow {
        kind: PolicyTreeRowKind::Policy {
            indent,
            key: key.to_owned(),
            value: value_summary,
            status,
        },
        id: RowId::new(RowTarget::Policy {
            key: key.to_owned(),
        }),
        search_text,
    });

    push_child_rows(
        rows,
        context,
        ChildRows {
            indent: indent + 1,
            top_key: key,
            path,
            values,
        },
    );
}

fn push_child_rows<'a>(
    rows: &mut Vec<PolicyTreeRow>,
    context: BuildContext<'a>,
    child: ChildRows<'a, '_>,
) {
    match (
        child.values.baseline,
        child.values.current,
        child.values.default,
    ) {
        (None, None, Some(PolicyValue::List(default))) => {
            for (index, value) in default.iter().enumerate() {
                rows.push(PolicyTreeRow::value_row(
                    child.indent,
                    value,
                    RowStatus::NotApplied,
                    extension_name(context.manifest, context.browser, child.top_key, value),
                    RowTarget::ListItem {
                        key: child.top_key.to_owned(),
                        path: child.path.clone(),
                        current_index: None,
                        restore_index: index,
                    },
                ));
            }
        }
        (baseline, current, _) if value_is_list(baseline) || value_is_list(current) => {
            let baseline = baseline.and_then(PolicyValue::as_list);
            let current = current.and_then(PolicyValue::as_list);
            for item in diff::list_items(baseline, current) {
                let restore_index = item
                    .baseline_index
                    .or(item.current_index)
                    .unwrap_or_default();
                let target = RowTarget::ListItem {
                    key: child.top_key.to_owned(),
                    path: child.path.clone(),
                    current_index: item.current_index,
                    restore_index,
                };
                rows.push(PolicyTreeRow::value_row(
                    child.indent,
                    item.value,
                    item.status.into(),
                    extension_name(context.manifest, context.browser, child.top_key, item.value),
                    target,
                ));
            }
        }
        (None, None, Some(PolicyValue::Object(default))) => {
            for (key, value) in default {
                let value_summary = PolicyValueSummary::new(value);
                let search_text = policy_search_text(key, &value_summary);
                rows.push(PolicyTreeRow {
                    kind: PolicyTreeRowKind::Policy {
                        indent: child.indent,
                        key: key.clone(),
                        value: value_summary,
                        status: RowStatus::NotApplied,
                    },
                    id: RowId::new(display_target(
                        child.top_key,
                        &child.path,
                        PathSegment::Field(key.clone()),
                    )),
                    search_text,
                });

                let mut child_path = child.path.clone();
                child_path.push(PathSegment::Field(key.clone()));
                push_child_rows(
                    rows,
                    context,
                    ChildRows {
                        indent: child.indent + 1,
                        top_key: child.top_key,
                        path: child_path,
                        values: PolicyValues {
                            baseline: None,
                            current: None,
                            default: Some(value),
                        },
                    },
                );
            }
        }
        (baseline, current, default) if value_is_object(baseline) || value_is_object(current) => {
            let baseline = baseline.and_then(PolicyValue::as_object);
            let current = current.and_then(PolicyValue::as_object);
            let default = default.and_then(PolicyValue::as_object);

            for key in visible_object_keys(baseline, current, default) {
                let baseline_value = baseline.and_then(|values| values.get(&key));
                let current_value = current.and_then(|values| values.get(&key));
                let default_value = default.and_then(|values| values.get(&key));
                let Some(value) = current_value.or(baseline_value).or(default_value) else {
                    continue;
                };
                let value_summary = PolicyValueSummary::new(value);
                let search_text = policy_search_text(&key, &value_summary);
                let child_segment = PathSegment::Field(key.clone());
                let target = if current_value.is_some() {
                    child_target(child.top_key, &child.path, child_segment.clone())
                } else {
                    display_target(child.top_key, &child.path, child_segment.clone())
                };

                rows.push(PolicyTreeRow {
                    kind: PolicyTreeRowKind::Policy {
                        indent: child.indent,
                        key: key.clone(),
                        value: value_summary,
                        status: row_status(PolicyValues {
                            baseline: baseline_value,
                            current: current_value,
                            default: default_value,
                        }),
                    },
                    id: RowId::new(target),
                    search_text,
                });

                let mut child_path = child.path.clone();
                child_path.push(child_segment);
                push_child_rows(
                    rows,
                    context,
                    ChildRows {
                        indent: child.indent + 1,
                        top_key: child.top_key,
                        path: child_path,
                        values: PolicyValues {
                            baseline: baseline_value,
                            current: current_value,
                            default: default_value,
                        },
                    },
                );
            }
        }
        (_, _, _) => {}
    }
}

fn row_status(values: PolicyValues<'_>) -> RowStatus {
    match (values.baseline, values.current, values.default) {
        (None, None, Some(_)) => RowStatus::NotApplied,
        (baseline, current, _) => diff::status(baseline, current).into(),
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

fn value_is_list(value: Option<&PolicyValue>) -> bool {
    matches!(value, Some(PolicyValue::List(_)))
}

fn value_is_object(value: Option<&PolicyValue>) -> bool {
    matches!(value, Some(PolicyValue::Object(_)))
}

fn visible_object_keys(
    baseline: Option<&PolicySet>,
    current: Option<&PolicySet>,
    default: Option<&PolicySet>,
) -> Vec<String> {
    baseline
        .into_iter()
        .flat_map(|values| values.keys())
        .chain(current.into_iter().flat_map(|values| values.keys()))
        .chain(default.into_iter().flat_map(|values| values.keys()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

fn child_target(top_key: &str, parent_path: &[PathSegment], segment: PathSegment) -> RowTarget {
    let mut path = parent_path.to_vec();
    path.push(segment);

    RowTarget::Path {
        key: top_key.to_owned(),
        path,
    }
}

fn display_target(top_key: &str, parent_path: &[PathSegment], segment: PathSegment) -> RowTarget {
    let mut path = parent_path.to_vec();
    path.push(segment);

    RowTarget::Display {
        key: top_key.to_owned(),
        path,
    }
}

fn remove_path(value: &mut PolicyValue, path: &[PathSegment]) -> bool {
    let Some((segment, rest)) = path.split_first() else {
        return false;
    };

    match (segment, value, rest.is_empty()) {
        (PathSegment::Field(field), PolicyValue::Object(values), true) => {
            values.remove(field).is_some()
        }
        (PathSegment::Field(field), PolicyValue::Object(values), false) => values
            .get_mut(field)
            .is_some_and(|value| remove_path(value, rest)),
        (_, _, _) => false,
    }
}

fn remove_list_item(
    current: &mut PolicySet,
    key: &str,
    path: &[PathSegment],
    index: usize,
) -> bool {
    let Some(values) = list_parent_mut(current, key, path) else {
        return false;
    };
    if index >= values.len() {
        return false;
    }

    values.remove(index);
    true
}

fn insert_list_item(current: &mut PolicySet, item: ListItemInsert<'_>) -> bool {
    ensure_top_level_list(current, item.key, item.path);
    let Some(values) = list_parent_mut(current, item.key, item.path) else {
        return false;
    };

    let index = baseline_restore_index(values, item.baseline_values, item.index);
    values.insert(index, item.value);
    true
}

fn insert_list_item_value(
    current: &mut PolicySet,
    target: &ListAddTarget,
    value: PolicyValue,
) -> Option<usize> {
    ensure_list_policy(current, target)?;
    let values = list_parent_mut(current, &target.key, &target.path)?;
    let index = values.len();

    values.insert(index, value);
    Some(index)
}

fn ensure_list_policy(current: &mut PolicySet, target: &ListAddTarget) -> Option<()> {
    if current.contains_key(&target.key) {
        return Some(());
    }

    if let Some(value) = &target.fallback_value {
        current.insert(target.key.clone(), value.clone());
        return Some(());
    }

    if target.path.is_empty() {
        current.insert(target.key.clone(), PolicyValue::List(Vec::new()));
        return Some(());
    }

    None
}

fn set_list_item(
    current: &mut PolicySet,
    key: &str,
    path: &[PathSegment],
    index: usize,
    value: PolicyValue,
) -> bool {
    let Some(values) = list_parent_mut(current, key, path) else {
        return false;
    };
    if index >= values.len() {
        return false;
    }

    values[index] = value;
    true
}

fn ensure_top_level_list(current: &mut PolicySet, key: &str, path: &[PathSegment]) {
    if path.is_empty() && !current.contains_key(key) {
        current.insert(key.to_owned(), PolicyValue::List(Vec::new()));
    }
}

fn list_parent_mut<'a>(
    current: &'a mut PolicySet,
    key: &str,
    path: &[PathSegment],
) -> Option<&'a mut Vec<PolicyValue>> {
    let value = current.get_mut(key)?;

    if path.is_empty() {
        return value.as_list_mut();
    }

    list_at_path_mut(value, path)
}

fn list_parent<'a>(
    policies: &'a PolicySet,
    key: &str,
    path: &[PathSegment],
) -> Option<&'a [PolicyValue]> {
    let value = policies.get(key)?;

    if path.is_empty() {
        return value.as_list();
    }

    list_at_path(value, path)
}

fn baseline_restore_index(
    current: &[PolicyValue],
    baseline: Option<&[PolicyValue]>,
    restore_index: usize,
) -> usize {
    let Some(baseline) = baseline else {
        return restore_index.min(current.len());
    };

    let mut baseline_by_value: BTreeMap<&PolicyValue, VecDeque<usize>> = BTreeMap::new();
    for (baseline_index, value) in baseline.iter().enumerate() {
        baseline_by_value
            .entry(value)
            .or_default()
            .push_back(baseline_index);
    }

    current
        .iter()
        .enumerate()
        .find_map(|(current_index, value)| {
            let baseline_index = baseline_by_value
                .get_mut(value)
                .and_then(VecDeque::pop_front)?;
            (baseline_index > restore_index).then_some(current_index)
        })
        .unwrap_or(current.len())
}

fn list_at_path_mut<'a>(
    value: &'a mut PolicyValue,
    path: &[PathSegment],
) -> Option<&'a mut Vec<PolicyValue>> {
    let Some((segment, rest)) = path.split_first() else {
        return value.as_list_mut();
    };

    match (segment, value) {
        (PathSegment::Field(field), PolicyValue::Object(values)) => values
            .get_mut(field)
            .and_then(|value| list_at_path_mut(value, rest)),
        (_, _) => None,
    }
}

fn list_at_path<'a>(value: &'a PolicyValue, path: &[PathSegment]) -> Option<&'a [PolicyValue]> {
    let Some((segment, rest)) = path.split_first() else {
        return value.as_list();
    };

    match (segment, value) {
        (PathSegment::Field(field), PolicyValue::Object(values)) => values
            .get(field)
            .and_then(|value| list_at_path(value, rest)),
        (_, _) => None,
    }
}

fn set_target_value(current: &mut PolicySet, target: &RowTarget, value: PolicyValue) -> bool {
    match target {
        RowTarget::Policy { key } => {
            current.insert(key.clone(), value);
            true
        }
        RowTarget::Path { key, path } => current
            .get_mut(key)
            .is_some_and(|current| set_path(current, path, value)),
        RowTarget::ListItem {
            key,
            path,
            current_index: Some(index),
            ..
        } => set_list_item(current, key, path, *index, value),
        RowTarget::Group(_) | RowTarget::Display { .. } => false,
        RowTarget::ListItem {
            current_index: None,
            ..
        } => false,
    }
}

fn set_path(current: &mut PolicyValue, path: &[PathSegment], value: PolicyValue) -> bool {
    let Some((segment, rest)) = path.split_first() else {
        *current = value;
        return true;
    };

    match (segment, current, rest.is_empty()) {
        (PathSegment::Field(field), PolicyValue::Object(values), true) => {
            values.insert(field.clone(), value);
            true
        }
        (PathSegment::Field(field), PolicyValue::Object(values), false) => values
            .get_mut(field)
            .is_some_and(|current| set_path(current, rest, value)),
        (_, _, _) => false,
    }
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

fn toggle_manifest_group(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
    group_id: &str,
) -> PolicySet {
    let mut updated = current.clone();
    let groups = ordered_groups(manifest, browser);
    let Some(group) = groups.into_iter().find(|group| group.group.id == group_id) else {
        return updated;
    };
    let all_selected = group
        .settings
        .iter()
        .all(|setting| current.contains_key(&setting.key));

    if all_selected {
        for setting in group.settings {
            updated.remove(&setting.key);
        }
    } else {
        for setting in group.settings {
            updated.insert(
                setting.key.clone(),
                baseline.get(&setting.key).unwrap_or(&setting.value).clone(),
            );
        }
    }

    updated
}

fn toggle_custom_group(
    manifest: &Manifest,
    browser: Browser,
    baseline: &PolicySet,
    current: &PolicySet,
) -> PolicySet {
    let active_keys = active_group_keys(manifest, browser);
    let keys = custom_keys(&active_keys, baseline, current);
    let all_selected = keys.iter().all(|key| current.contains_key(key));
    let mut updated = current.clone();

    if all_selected {
        for key in keys {
            updated.remove(&key);
        }
    } else {
        for key in keys {
            if let Some(value) = baseline.get(&key) {
                updated.insert(key, value.clone());
            }
        }
    }

    updated
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
    ) -> PolicyTreeRow {
        let value_summary = PolicyValueSummary::new(value);
        let search_text = value_search_text(&value_summary, extension_name);

        PolicyTreeRow {
            kind: PolicyTreeRowKind::Value {
                indent,
                value: value_summary,
                status,
                extension_name: extension_name.map(ToOwned::to_owned),
            },
            id: RowId::new(target),
            search_text,
        }
    }
}

impl RowTarget {
    const fn is_value_target(&self) -> bool {
        matches!(
            self,
            Self::Policy { .. }
                | Self::Path { .. }
                | Self::ListItem {
                    current_index: Some(_),
                    ..
                }
        )
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
        let search_label = value.display_value();
        let policy_label = match value {
            PolicyValue::List(_) => String::new(),
            PolicyValue::Object(values) => values.len().to_string(),
            PolicyValue::Bool(_)
            | PolicyValue::Integer(_)
            | PolicyValue::String(_)
            | PolicyValue::Null => search_label.clone(),
        };
        let child_label = match value {
            PolicyValue::String(value) => format!("{value:?}"),
            PolicyValue::Bool(_)
            | PolicyValue::Integer(_)
            | PolicyValue::List(_)
            | PolicyValue::Object(_)
            | PolicyValue::Null => search_label.clone(),
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
        &self.policy_label
    }

    pub(crate) fn child_label(&self) -> &str {
        &self.child_label
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

impl PolicyValue {
    fn as_list(&self) -> Option<&[PolicyValue]> {
        match self {
            Self::List(values) => Some(values),
            Self::Bool(_) | Self::Integer(_) | Self::String(_) | Self::Object(_) | Self::Null => {
                None
            }
        }
    }

    fn as_list_mut(&mut self) -> Option<&mut Vec<PolicyValue>> {
        match self {
            Self::List(values) => Some(values),
            Self::Bool(_) | Self::Integer(_) | Self::String(_) | Self::Object(_) | Self::Null => {
                None
            }
        }
    }

    fn as_object(&self) -> Option<&PolicySet> {
        match self {
            Self::Object(values) => Some(values),
            Self::Bool(_) | Self::Integer(_) | Self::String(_) | Self::List(_) | Self::Null => None,
        }
    }
}
