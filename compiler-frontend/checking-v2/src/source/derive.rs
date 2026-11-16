use building_types::QueryResult;
use indexing::DeriveItemId;
use itertools::Itertools;

use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::{CheckContext, Item};
use crate::core::data::{self, ConstructorFields};
use crate::core::entail::{self, ConstraintHead};
use crate::core::{normalise, subsume, unify};
use crate::error::ErrorKind;
use crate::source::instances::type_item_name;
use crate::state::CheckState;
use crate::tree::Obligation;
use crate::types::{ForallBinder, Name, Type, TypeId};

pub fn derive_obligations<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    derive_id: DeriveItemId,
    head: &ConstraintHead,
) -> QueryResult<Vec<Obligation>>
where
    Q: ExternalQueries,
{
    let Some(lowered) = context.lowered.tree.get_derive_item(derive_id) else {
        return Ok(Vec::new());
    };
    let class = type_item_name(context, head.class)?;
    let Some((&last, preceding)) = head.arguments.split_last() else {
        state.insert_error(ErrorKind::CannotDerive { class });
        return Ok(Vec::new());
    };

    let known = &context.known;
    let derived_class = Some(head.class);

    if !lowered.newtype
        && let Some(base) = lifted_base(context, head.class)
    {
        return Ok(vec![lifted_obligation(state, context, base, last)?]);
    }

    let obligations = if lowered.newtype {
        let Some(field) = data::derived_newtype_field(state, context, last)? else {
            state.insert_error(ErrorKind::CannotDerive { class });
            return Ok(Vec::new());
        };
        let arguments = preceding.iter().copied().chain([field]).collect_vec();
        vec![constraint_type(context, head.class, &head.kind_arguments, &arguments)]
    } else if context.known.newtype == Some(head.class) {
        Vec::new()
    } else if context.known.eq == Some(head.class) || context.known.ord == Some(head.class) {
        let Some((_, _, fields)) = data::data_fields(state, context, last)? else {
            state.insert_error(ErrorKind::CannotDerive { class });
            return Ok(Vec::new());
        };
        let lifted = if context.known.eq == Some(head.class) {
            context.known.eq1
        } else {
            context.known.ord1
        };
        let mut obligations = Vec::with_capacity(fields.len());
        for field in fields {
            field_obligations(state, context, (head.class, lifted), field, &mut obligations);
        }
        obligations
    } else if derived_class == known.functor
        || derived_class == known.foldable
        || derived_class == known.traversable
    {
        let functions =
            if derived_class == known.functor { Functions::Covariant } else { Functions::Rejected };
        let Some(obligations) = traversal_obligations(state, context, head.class, functions, last)?
        else {
            state.insert_error(ErrorKind::CannotDerive { class });
            return Ok(Vec::new());
        };
        obligations
    } else if derived_class == known.generic {
        Vec::new()
    } else {
        state.insert_error(ErrorKind::UnsupportedFeature { feature: "deriving this class" });
        Vec::new()
    };

    let obligations = obligations.into_iter().map(|constraint| Obligation {
        constraint,
        abstraction: None,
        evidence: state.emit_wanted(constraint),
    });
    Ok(obligations.collect_vec())
}

pub fn improve_derived_generic<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constraint: TypeId,
) -> QueryResult<()>
where
    Q: ExternalQueries,
{
    let Some(head) = entail::constraint_head(state, context, constraint) else {
        return Ok(());
    };
    let [derived, representation] = head.arguments.as_slice() else {
        return Ok(());
    };
    match generic_representation(state, context, *derived)? {
        Some(generic) => {
            unify::unify(state, context, *representation, generic)?;
        }
        None => {
            let class = type_item_name(context, head.class)?;
            state.insert_error(ErrorKind::CannotDerive { class });
        }
    }
    Ok(())
}

fn generic_representation<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    derived: TypeId,
) -> QueryResult<Option<TypeId>>
where
    Q: ExternalQueries,
{
    let known = &context.known;
    let (
        Some(sum),
        Some(product),
        Some(constructor),
        Some(argument),
        Some(no_arguments),
        Some(no_constructors),
    ) = (
        known.generic_sum,
        known.generic_product,
        known.generic_constructor,
        known.generic_argument,
        known.generic_no_arguments,
        known.generic_no_constructors,
    )
    else {
        return Ok(None);
    };
    let Some((head, _, constructors)) = data::constructor_fields(state, context, derived)? else {
        return Ok(None);
    };

    let (data_file, _) = head.class;
    let indexed = context.queries.indexed(data_file)?;

    let sum = context.intern_constructor(sum);
    let product = context.intern_constructor(product);
    let constructor = context.intern_constructor(constructor);
    let argument = context.intern_constructor(argument);

    let representations = constructors.iter().map(|ConstructorFields { item, fields }| {
        let name = indexed.items[*item].name.as_ref().map_or("", SmolStr::as_str);
        let name = context.intern_type(Type::String(context.intern_text(SmolStr::new(name))));

        let arguments = fields.iter().map(|&field| context.intern_application(argument, field));
        let arguments = arguments.collect_vec();
        let fields = match arguments.split_last() {
            None => context.intern_constructor(no_arguments),
            Some((&last, preceding)) => preceding.iter().rev().fold(last, |rest, &field| {
                context.intern_application_list(product, &[field, rest])
            }),
        };
        context.intern_application_list(constructor, &[name, fields])
    });
    let representations = representations.collect_vec();

    let representation = match representations.split_last() {
        None => context.intern_constructor(no_constructors),
        Some((&last, preceding)) => preceding.iter().rev().fold(last, |rest, &alternative| {
            context.intern_application_list(sum, &[alternative, rest])
        }),
    };
    Ok(Some(representation))
}

fn lifted_base<Q>(context: &CheckContext<Q>, class: Item) -> Option<Item>
where
    Q: ExternalQueries,
{
    let known = &context.known;
    if Some(class) == known.eq1 {
        known.eq
    } else if Some(class) == known.ord1 {
        known.ord
    } else {
        None
    }
}

fn lifted_obligation<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    base: Item,
    constructor: TypeId,
) -> QueryResult<Obligation>
where
    Q: ExternalQueries,
{
    let name = state.fresh_name();
    let text = context.intern_text(SmolStr::new_static("a"));
    let kind = context.types.type_;
    let binder_id = context.intern_forall_binder(ForallBinder { visible: false, name, text, kind });
    let variable = context.intern_type(Type::Variable(binder_id));

    let given = constraint_type(context, base, &[], &[variable]);
    let applied = context.intern_application(constructor, variable);
    let wanted = constraint_type(context, base, &[], &[applied]);
    let constraint =
        context.intern_forall_list(&[binder_id], context.intern_constrained(given, wanted));

    let (abstraction, evidence) =
        subsume::abstract_over(state, context, constraint, |state, inner| {
            Ok(state.emit_wanted(inner))
        })?;
    Ok(Obligation { constraint, abstraction: Some(abstraction), evidence })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Functions {
    Covariant,
    Rejected,
}

fn traversal_obligations<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    class: Item,
    functions: Functions,
    derived: TypeId,
) -> QueryResult<Option<Vec<TypeId>>>
where
    Q: ExternalQueries,
{
    let Some((head, data, fields)) = data::data_fields(state, context, derived)? else {
        return Ok(None);
    };
    let (Some(&parameter), true) =
        (data.parameters.last(), data.parameters.len() == head.arguments.len() + 1)
    else {
        return Ok(None);
    };
    let parameter = context.lookup_forall_binder(parameter).name;

    let mut traversal = Traversal { state, context, class, functions, parameter };
    let mut obligations = Vec::new();
    for field in fields {
        if !traversal.field(field, &mut obligations) {
            return Ok(None);
        }
    }
    Ok(Some(obligations))
}

struct Traversal<'a, 'q, Q>
where
    Q: ExternalQueries,
{
    state: &'a CheckState,
    context: &'a CheckContext<'q, Q>,
    class: Item,
    functions: Functions,
    parameter: Name,
}

impl<Q> Traversal<'_, '_, Q>
where
    Q: ExternalQueries,
{
    fn mentions(&self, id: TypeId) -> bool {
        data::mentions_variable(self.context, id, self.parameter)
    }

    fn field(&mut self, field: TypeId, obligations: &mut Vec<TypeId>) -> bool {
        if !self.mentions(field) {
            return true;
        }

        let field = normalise::normalise(self.state, self.context, field);
        if let Type::Variable(_) = self.context.lookup_type(field) {
            return true;
        }

        if let Some((argument, result)) = normalise::function_parts(self.state, self.context, field)
        {
            let covariant = self.functions == Functions::Covariant && !self.mentions(argument);
            return covariant && self.field(result, obligations);
        }

        let Type::Application(function, argument) = self.context.lookup_type(field) else {
            return false;
        };
        let function = normalise::normalise(self.state, self.context, function);

        if function == self.context.types.record {
            let row = normalise::row_parts(self.state, self.context, argument);
            if row.tail.is_some_and(|tail| self.mentions(tail)) {
                return false;
            }
            return row.fields.iter().all(|row_field| self.field(row_field.id, obligations));
        }

        if self.mentions(function) {
            return false;
        }
        obligations.push(constraint_type(self.context, self.class, &[], &[function]));
        self.field(argument, obligations)
    }
}

fn constraint_type<Q>(
    context: &CheckContext<Q>,
    class: Item,
    kind_arguments: &[TypeId],
    arguments: &[TypeId],
) -> TypeId
where
    Q: ExternalQueries,
{
    let constructor = context.intern_constructor(class);
    let constructor = kind_arguments.iter().fold(constructor, |function, &argument| {
        context.intern_kind_application(function, argument)
    });
    context.intern_application_list(constructor, arguments)
}

fn field_obligations<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    (class, lifted): (Item, Option<Item>),
    field: TypeId,
    obligations: &mut Vec<TypeId>,
) where
    Q: ExternalQueries,
{
    let normalised = normalise::normalise(state, context, field);
    let Type::Application(function, argument) = context.lookup_type(normalised) else {
        obligations.push(constraint_type(context, class, &[], &[field]));
        return;
    };
    let function = normalise::normalise(state, context, function);

    if function == context.types.record {
        let row = normalise::row_parts(state, context, argument);
        if row.tail.is_none() {
            for row_field in row.fields.iter() {
                field_obligations(state, context, (class, lifted), row_field.id, obligations);
            }
            return;
        }
    }

    if let Some(lifted) = lifted
        && is_type_constructor_variable(state, context, function)
    {
        obligations.push(constraint_type(context, lifted, &[], &[function]));
        obligations.push(constraint_type(context, class, &[], &[argument]));
        return;
    }

    obligations.push(constraint_type(context, class, &[], &[field]));
}

fn is_type_constructor_variable<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    function: TypeId,
) -> bool
where
    Q: ExternalQueries,
{
    let kind = match context.lookup_type(function) {
        Type::Rigid(rigid_id) => context.lookup_rigid(rigid_id).kind,
        Type::Unification(unification_id) => state.unification(unification_id).kind,
        _ => return false,
    };
    let parts = normalise::function_parts(state, context, kind);
    let parts = parts.map(|(argument, result)| {
        (
            normalise::normalise(state, context, argument),
            normalise::normalise(state, context, result),
        )
    });
    parts == Some((context.types.type_, context.types.type_))
}
