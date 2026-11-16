use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::core::fold;
use crate::error::ErrorKind;
use crate::state::{CheckState, Wanted};

use super::{no_instance, solved};

pub fn solve_fail<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> Outcome
where
    Q: ExternalQueries,
{
    let &[message] = head.arguments.as_slice() else {
        return no_instance(state, context, wanted);
    };
    let message = fold::zonk(state, context, message);
    Outcome::Failed(ErrorKind::UserDefinedError { message })
}

pub fn solve_warn<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> Outcome
where
    Q: ExternalQueries,
{
    let &[message] = head.arguments.as_slice() else {
        return no_instance(state, context, wanted);
    };
    let message = fold::zonk(state, context, message);
    let crumbs = wanted.crumbs.clone();
    state.insert_error_at(ErrorKind::UserDefinedWarning { message }, crumbs);
    solved(state, wanted, head.class)
}
