use building_types::QueryResult;
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::entail::{ConstraintHead, Outcome};
use crate::core::normalise;
use crate::state::{CheckState, Wanted};
use crate::tree::BuiltinEvidence;
use crate::types::Type;

use super::{equate_with, no_instance, solved_with, symbol};

pub fn solve_is_symbol<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> Outcome
where
    Q: ExternalQueries,
{
    let &[label] = head.arguments.as_slice() else {
        return no_instance(state, context, wanted);
    };
    let Some(label) = symbol(state, context, label) else {
        return Outcome::Stuck;
    };
    solved_with(state, wanted, head.class, BuiltinEvidence::Symbol(label))
}

pub fn solve_reflectable<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    wanted: &Wanted,
    head: &ConstraintHead,
) -> QueryResult<Outcome>
where
    Q: ExternalQueries,
{
    let &[value, reflected] = head.arguments.as_slice() else {
        return Ok(no_instance(state, context, wanted));
    };

    let known = &context.known;
    let value = normalise::normalise(state, context, value);
    let (reflected_type, evidence) = match context.lookup_type(value) {
        Type::String(text) => {
            (context.types.string, BuiltinEvidence::Symbol(context.lookup_text(text)))
        }
        Type::Integer(integer) => (context.types.int, BuiltinEvidence::Integer(integer)),
        Type::Constructor(file_id, type_id) => {
            let item = Some((file_id, type_id));
            if item == known.boolean_true || item == known.boolean_false {
                (context.types.boolean, BuiltinEvidence::Boolean(item == known.boolean_true))
            } else if let Some(ordering) = known.ordering
                && let Some(name) = ordering_name(context, (file_id, type_id))
            {
                (context.intern_constructor(ordering), BuiltinEvidence::Ordering(name))
            } else {
                return Ok(no_instance(state, context, wanted));
            }
        }
        Type::Unification(_) => return Ok(Outcome::Stuck),
        _ => return Ok(no_instance(state, context, wanted)),
    };

    equate_with(state, context, wanted, head.class, &[(reflected, reflected_type)], evidence)
}

fn ordering_name<Q>(context: &CheckContext<Q>, item: Item) -> Option<SmolStr>
where
    Q: ExternalQueries,
{
    let known = &context.known;
    let item = Some(item);
    if item == known.ordering_lt {
        Some(SmolStr::new_static("LT"))
    } else if item == known.ordering_eq {
        Some(SmolStr::new_static("EQ"))
    } else if item == known.ordering_gt {
        Some(SmolStr::new_static("GT"))
    } else {
        None
    }
}
