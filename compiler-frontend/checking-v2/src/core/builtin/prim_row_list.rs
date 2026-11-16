use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::entail::{ConstraintHead, Outcome};
use crate::core::normalise;
use crate::state::{CheckState, Wanted};

use super::{equate, intern_symbol, no_instance};

pub fn solve_row_to_list<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[row, list] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };
    let (&[element_kind], Some(cons), Some(nil)) =
        (head.kind_arguments.as_slice(), context.known.row_list_cons, context.known.row_list_nil)
    else {
        return Ok(no_instance(state, context, wanted));
    };

    let parts = normalise::row_parts(state, context, row);
    if parts.tail.is_some() {
        return Ok(Outcome::Stuck);
    }

    let cons = context.intern_kind_application(context.intern_constructor(cons), element_kind);
    let nil = context.intern_kind_application(context.intern_constructor(nil), element_kind);

    let result = parts.fields.iter().rev().fold(nil, |rest, field| {
        let label = intern_symbol(context, &field.label);
        context.intern_application_list(cons, &[label, field.id, rest])
    });
    equate(state, context, wanted, head.class, &[(list, result)])
}
