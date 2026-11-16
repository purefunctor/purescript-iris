pub mod binders;
pub mod derive;
pub mod equations;
pub mod escape;
pub mod instances;
pub mod roles;
pub mod term_items;
pub mod terms;
pub mod type_items;
pub mod types;
pub mod zonk;

use std::mem;

use building_types::QueryResult;
use itertools::Itertools;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::fold;
use crate::error::{CheckingError, ErrorKind};
use crate::state::CheckState;

pub fn finish<Q>(state: &mut CheckState, context: &CheckContext<Q>) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    for hole in mem::take(&mut state.holes) {
        let inferred = hole.type_id;
        state.insert_error_at(
            ErrorKind::HoleInferredType { name: hole.name, inferred },
            hole.crumbs,
        );
    }

    let errors = mem::take(&mut state.checked.errors).into_iter().map(|error| {
        let kind = error.kind.map_types(|id| fold::zonk(state, context, id));
        CheckingError { kind, crumbs: error.crumbs }
    });
    let errors = errors.collect_vec();
    state.checked.errors = errors;

    let mut checked = mem::take(&mut state.checked);
    zonk::zonk_declarations(state, context, &mut checked);
    zonk::zonk_tree(state, context, &mut checked.tree);
    state.checked = checked;

    Ok(())
}
