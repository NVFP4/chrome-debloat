use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::chromium::policy::{PolicySet, PolicyValue};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffCounts {
    pub added: u16,
    pub edited: u16,
    pub deleted: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Applied,
    Added,
    Edited,
    Deleted,
}

#[derive(Debug, Clone, Copy)]
pub struct ListItemDiff<'a> {
    pub value: &'a PolicyValue,
    pub status: DiffStatus,
    pub baseline_index: Option<usize>,
    pub current_index: Option<usize>,
}

impl DiffCounts {
    pub const fn is_empty(self) -> bool {
        self.added == 0 && self.edited == 0 && self.deleted == 0
    }
}

pub fn counts(baseline: &PolicySet, current: &PolicySet) -> DiffCounts {
    let added = current
        .keys()
        .filter(|key| !baseline.contains_key(*key))
        .count();
    let edited = current
        .iter()
        .filter(|(key, value)| baseline.get(*key).is_some_and(|base| base != *value))
        .count();
    let deleted = baseline
        .keys()
        .filter(|key| !current.contains_key(*key))
        .count();

    DiffCounts {
        added: clamp_count(added),
        edited: clamp_count(edited),
        deleted: clamp_count(deleted),
    }
}

pub fn status(baseline: Option<&PolicyValue>, current: Option<&PolicyValue>) -> DiffStatus {
    match (baseline, current) {
        (None, Some(_)) => DiffStatus::Added,
        (Some(_), None) => DiffStatus::Deleted,
        (Some(baseline), Some(current)) if baseline == current => DiffStatus::Applied,
        (Some(_), Some(_)) => DiffStatus::Edited,
        (None, None) => DiffStatus::Applied,
    }
}

pub fn visible_policy_keys<'a>(
    baseline: &'a PolicySet,
    current: &'a PolicySet,
) -> BTreeSet<&'a str> {
    baseline
        .keys()
        .map(String::as_str)
        .chain(current.keys().map(String::as_str))
        .collect()
}

pub fn list_items<'a>(
    baseline: Option<&'a [PolicyValue]>,
    current: Option<&'a [PolicyValue]>,
) -> Vec<ListItemDiff<'a>> {
    let baseline = baseline.unwrap_or_default();
    let current = current.unwrap_or_default();
    let mut current_by_value: BTreeMap<&PolicyValue, VecDeque<usize>> = BTreeMap::new();
    let mut matched_current = vec![false; current.len()];

    for (current_index, value) in current.iter().enumerate() {
        current_by_value
            .entry(value)
            .or_default()
            .push_back(current_index);
    }

    let mut items = Vec::new();
    for (baseline_index, value) in baseline.iter().enumerate() {
        let current_index = current_by_value
            .get_mut(value)
            .and_then(VecDeque::pop_front);

        if let Some(current_index) = current_index {
            matched_current[current_index] = true;
            items.push(ListItemDiff {
                value,
                status: DiffStatus::Applied,
                baseline_index: Some(baseline_index),
                current_index: Some(current_index),
            });
        } else {
            items.push(ListItemDiff {
                value,
                status: DiffStatus::Deleted,
                baseline_index: Some(baseline_index),
                current_index: None,
            });
        }
    }

    items.extend(
        current
            .iter()
            .enumerate()
            .filter_map(|(current_index, value)| {
                (!matched_current[current_index]).then_some(ListItemDiff {
                    value,
                    status: DiffStatus::Added,
                    baseline_index: None,
                    current_index: Some(current_index),
                })
            }),
    );
    items
}

fn clamp_count(count: usize) -> u16 {
    count.min(usize::from(u16::MAX)) as u16
}
