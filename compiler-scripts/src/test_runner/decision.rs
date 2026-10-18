/// Decision logic for test runner output, separated from rendering.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    pub tests_passed: bool,
    pub pending_count: usize,
    pub total_lines_changed: usize,
    pub showed_diffs: bool,
    pub ran_all: bool,
    pub max_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Clean,
    Failure(FailureDecision),
    Pending(PendingDecision),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureDecision {
    pub show_verbose_hint: bool,
    pub pending_note: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDecision {
    pub show_lines_changed: bool,
    pub next_action: NextAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    AcceptOrReject,
    ReviewSubset,
    ShowDiff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDisplayLimits {
    pub max_shown: usize,
}

pub fn decide_outcome(input: &DecisionInput) -> Outcome {
    let has_pending = input.pending_count > 0;

    if input.tests_passed && !has_pending {
        return Outcome::Clean;
    }

    if !input.tests_passed {
        return Outcome::Failure(FailureDecision {
            show_verbose_hint: true,
            pending_note: if has_pending { Some(input.pending_count) } else { None },
        });
    }

    let many_pending = input.ran_all && input.pending_count > 3;

    let next_action = if input.showed_diffs {
        NextAction::AcceptOrReject
    } else if many_pending {
        NextAction::ReviewSubset
    } else {
        NextAction::ShowDiff
    };

    Outcome::Pending(PendingDecision {
        show_lines_changed: input.total_lines_changed > 50,
        next_action,
    })
}

pub fn decide_snapshot_limits(input: &DecisionInput) -> SnapshotDisplayLimits {
    let max_shown = if input.showed_diffs { usize::MAX } else { input.max_count };

    SnapshotDisplayLimits { max_shown }
}
