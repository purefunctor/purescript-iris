use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::coercible;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::state::{CheckState, Wanted};

pub fn solve<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Option<Outcome>>
where
    Q: ExternalQueries,
{
    if context.known.coercible == Some(head.class) {
        return coercible::solve(state, context, wanted, head).map(Some);
    }
    Ok(None)
}
