#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ActivateDialogButton,
    BackspacePolicyEdit,
    BackspaceFilter,
    BeginPolicyEdit,
    BeginFilter,
    CancelPolicyEdit,
    CancelFilter,
    CloseDialog,
    CommitPolicyEdit,
    CommitFilter,
    ConfirmApply,
    ConfirmQuit,
    ConfirmRevert,
    ConfirmUninstall,
    InputFilter(char),
    InputPolicyEdit(char),
    LocateExportFile,
    MovePolicyGroup(ActionStep),
    MovePolicyCursor(ActionStep),
    MovePolicyType(ActionStep),
    MoveDialogFocus(ActionStep),
    NewPolicyItem,
    Noop,
    OpenApplyDialog,
    OpenExportDialog,
    OpenReportIssue,
    OpenRevertDialog,
    OpenUninstallDialog,
    Paste(String),
    PolicyCursorEnd,
    PolicyCursorStart,
    Quit,
    Redraw,
    Redo,
    ScrollHelp { step: ActionStep, max_scroll: u16 },
    SelectTab(BrowserTabIndex),
    StagePolicyRemoval,
    Tick,
    ToggleHelp,
    TogglePolicyPresence,
    Undo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionStep(i16);

impl ActionStep {
    pub const PREVIOUS: Self = Self(-1);
    pub const NEXT: Self = Self(1);
    pub const PREVIOUS_POLICY_PAGE: Self = Self(-8);
    pub const NEXT_POLICY_PAGE: Self = Self(8);
    pub const PREVIOUS_HELP_PAGE: Self = Self(-6);
    pub const NEXT_HELP_PAGE: Self = Self(6);

    pub const fn offset(self) -> i16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserTabIndex(usize);

impl BrowserTabIndex {
    pub fn from_digit(character: char) -> Option<Self> {
        let digit = character.to_digit(10)?;
        if !(1..=9).contains(&digit) {
            return None;
        }

        Some(Self((digit - 1) as usize))
    }

    pub const fn get(self) -> usize {
        self.0
    }
}
