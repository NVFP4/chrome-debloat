use crate::chromium::policy::PolicyValue;
use crate::policy_tree::{EditablePolicyValue, EditableValueKind, RowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyEditorState {
    pub(crate) cursor: RowId,
    target: PolicyEditorTarget,
    kind: EditableValueKind,
    pub(crate) buffer: String,
    pub(crate) invalid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PolicyEditorTarget {
    Existing,
    NewListItem { insert_after: RowId, indent: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewListItemEditor {
    pub(crate) source_cursor: RowId,
    pub(crate) insert_after: RowId,
    pub(crate) indent: usize,
}

impl PolicyEditorState {
    pub(crate) fn new(cursor: RowId, value: EditablePolicyValue) -> Self {
        Self {
            cursor,
            target: PolicyEditorTarget::Existing,
            kind: value.kind,
            buffer: value.value,
            invalid: false,
        }
    }

    pub(crate) fn string(cursor: RowId, value: String) -> Self {
        Self {
            cursor,
            target: PolicyEditorTarget::Existing,
            kind: EditableValueKind::String,
            buffer: value,
            invalid: false,
        }
    }

    pub(crate) fn integer(cursor: RowId) -> Self {
        Self {
            cursor,
            target: PolicyEditorTarget::Existing,
            kind: EditableValueKind::Integer,
            buffer: String::new(),
            invalid: false,
        }
    }

    pub(crate) fn list_item(source_cursor: RowId, insert_after: RowId, indent: usize) -> Self {
        Self {
            cursor: source_cursor,
            target: PolicyEditorTarget::NewListItem {
                insert_after,
                indent,
            },
            kind: EditableValueKind::String,
            buffer: String::new(),
            invalid: false,
        }
    }

    pub(crate) fn existing_cursor(&self) -> Option<&RowId> {
        match &self.target {
            PolicyEditorTarget::Existing => Some(&self.cursor),
            PolicyEditorTarget::NewListItem { .. } => None,
        }
    }

    pub(crate) fn new_list_item(&self) -> Option<NewListItemEditor> {
        match &self.target {
            PolicyEditorTarget::Existing => None,
            PolicyEditorTarget::NewListItem {
                insert_after,
                indent,
            } => Some(NewListItemEditor {
                source_cursor: self.cursor.clone(),
                insert_after: insert_after.clone(),
                indent: *indent,
            }),
        }
    }

    pub(crate) fn accepts(&self, character: char) -> bool {
        match self.kind {
            EditableValueKind::String => !character.is_control(),
            EditableValueKind::Integer => {
                character.is_ascii_digit() || character == '-' && self.buffer.is_empty()
            }
        }
    }

    pub(crate) fn policy_value(&self) -> Option<PolicyValue> {
        match self.kind {
            EditableValueKind::String => Some(PolicyValue::String(self.buffer.clone())),
            EditableValueKind::Integer => self.buffer.parse::<i64>().ok().map(PolicyValue::Integer),
        }
    }

    pub(crate) fn placeholder(&self) -> Option<&'static str> {
        (self.buffer.is_empty() && self.kind == EditableValueKind::String).then_some("Enter Value")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyKeyEditorState {
    pub(crate) buffer: String,
    pub(crate) invalid: bool,
    selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewPolicyType {
    Bool,
    Integer,
    List,
    String,
}

impl NewPolicyType {
    pub(crate) const OPTIONS: [Self; 4] = [Self::Bool, Self::Integer, Self::String, Self::List];
    const DEFAULT: Self = Self::Bool;

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Integer => "number",
            Self::List => "list",
            Self::String => "string",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Bool => 0,
            Self::Integer => 1,
            Self::String => 2,
            Self::List => 3,
        }
    }

    pub(crate) fn initial_value(self) -> PolicyValue {
        match self {
            Self::Bool => PolicyValue::Bool(false),
            Self::Integer => PolicyValue::Integer(0),
            Self::List => PolicyValue::List(Vec::new()),
            Self::String => PolicyValue::String(String::new()),
        }
    }
}

impl Default for PolicyKeyEditorState {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            invalid: false,
            selected: NewPolicyType::DEFAULT.index(),
        }
    }
}

impl PolicyKeyEditorState {
    pub(crate) fn accepts(character: char) -> bool {
        !character.is_control()
    }

    pub(crate) fn key(&self) -> Option<String> {
        let key = self.buffer.trim();
        (!key.is_empty()).then(|| key.to_owned())
    }

    pub(crate) fn selected_type(&self) -> NewPolicyType {
        NewPolicyType::OPTIONS[self.selected]
    }

    pub(crate) fn placeholder(&self) -> Option<&'static str> {
        self.buffer.is_empty().then_some("Enter Key Name")
    }

    pub(crate) fn move_selection(&mut self, delta: i16) -> bool {
        let current = self.selected as i32;
        let option_count = NewPolicyType::OPTIONS.len() as i32;
        let next = (current + i32::from(delta)).rem_euclid(option_count) as usize;
        let changed = self.selected != next || self.invalid;

        self.selected = next;
        self.invalid = false;
        changed
    }
}
