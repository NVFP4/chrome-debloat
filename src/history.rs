use std::collections::BTreeSet;

use crate::chromium::policy::{PolicySet, PolicyValue};

#[derive(Debug, Default)]
pub struct EditHistory {
    patches: Vec<PolicyPatch>,
    cursor: usize,
    current: Option<PolicySet>,
    first: Option<PolicySet>,
}

#[derive(Debug)]
struct PolicyPatch {
    changes: Vec<PolicyChange>,
}

#[derive(Debug)]
struct PolicyChange {
    key: String,
    before: Option<PolicyValue>,
    after: Option<PolicyValue>,
}

impl EditHistory {
    pub fn push(&mut self, baseline: &PolicySet, policies: PolicySet) {
        let patch = PolicyPatch::between(self.current(baseline), &policies);
        if patch.is_empty() {
            return;
        }

        self.patches.truncate(self.cursor);
        if self.cursor == 0 || self.first.is_none() {
            self.first = Some(policies.clone());
        }
        self.patches.push(patch);
        self.cursor = self.patches.len();
        self.current = Some(policies);
    }

    pub fn undo(&mut self, baseline: &PolicySet) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let mut current = self.current.take().unwrap_or_else(|| baseline.clone());
        self.cursor -= 1;
        self.patches[self.cursor].apply_reverse(&mut current);
        if self.cursor == 0 {
            self.current = None;
        } else {
            self.current = Some(current);
        }

        true
    }

    pub fn redo(&mut self, baseline: &PolicySet) -> bool {
        if self.cursor >= self.patches.len() {
            return false;
        }

        let mut current = self.current.take().unwrap_or_else(|| baseline.clone());
        self.patches[self.cursor].apply(&mut current);
        self.cursor += 1;
        self.current = Some(current);

        true
    }

    pub fn revert(&mut self) -> bool {
        let changed = self.cursor != 0;

        self.patches.clear();
        self.cursor = 0;
        self.current = None;
        self.first = None;

        changed
    }

    pub fn current<'a>(&'a self, baseline: &'a PolicySet) -> &'a PolicySet {
        self.current.as_ref().unwrap_or(baseline)
    }

    pub fn current_differs_from_first(&self) -> bool {
        let Some(current) = &self.current else {
            return false;
        };

        self.first.as_ref() != Some(current)
    }
}

impl PolicyPatch {
    fn between(before: &PolicySet, after: &PolicySet) -> Self {
        let keys = before
            .keys()
            .chain(after.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let changes = keys
            .into_iter()
            .filter_map(|key| {
                let before = before.get(&key);
                let after = after.get(&key);
                (before != after).then(|| PolicyChange {
                    key,
                    before: before.cloned(),
                    after: after.cloned(),
                })
            })
            .collect();

        Self { changes }
    }

    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    fn apply(&self, policies: &mut PolicySet) {
        for change in &self.changes {
            apply_value(policies, &change.key, change.after.clone());
        }
    }

    fn apply_reverse(&self, policies: &mut PolicySet) {
        for change in self.changes.iter().rev() {
            apply_value(policies, &change.key, change.before.clone());
        }
    }
}

fn apply_value(policies: &mut PolicySet, key: &str, value: Option<PolicyValue>) {
    match value {
        Some(value) => {
            policies.insert(key.to_owned(), value);
        }
        None => {
            policies.remove(key);
        }
    }
}
