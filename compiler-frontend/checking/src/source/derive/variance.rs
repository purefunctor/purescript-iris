use building_types::QueryResult;
use files::FileId;
use indexing::{TermItemId, TypeItemId};
use smol_str::SmolStr;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::substitute::SubstituteName;
use crate::core::{ApplicationArgument, Name, Type, TypeId, constraint, normalise, toolkit};
use crate::error::ErrorKind;
use crate::state::CheckState;

use super::tools;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::source) enum Variance {
    Covariant,
    Contravariant,
}

impl Variance {
    fn flip(self) -> Variance {
        match self {
            Variance::Covariant => Variance::Contravariant,
            Variance::Contravariant => Variance::Covariant,
        }
    }
}

type ClassReference = (FileId, TypeItemId);

#[derive(Clone, Copy)]
pub(in crate::source) enum FunctionPolicy {
    Allow,
    Reject,
}

#[derive(Clone, Copy)]
pub(in crate::source) struct ParameterConfig {
    pub variance: Variance,
    pub unary_class: Option<ClassReference>,
    pub function_policy: FunctionPolicy,
}

#[derive(Clone, Copy)]
pub(in crate::source) enum VarianceConfig {
    Single { parameter: ParameterConfig, binary_class: Option<ClassReference> },
    Pair { first: ParameterConfig, second: ParameterConfig, binary_class: Option<ClassReference> },
}

pub struct VarianceRecipe {
    pub constructors: Vec<ConstructorRecipe>,
    pub valid: bool,
}

pub struct ConstructorRecipe {
    pub constructor_id: TermItemId,
    pub fields: Vec<Option<TraversalOperation>>,
}

#[derive(Clone, Copy)]
pub enum TraversalParameter {
    First,
    Second,
}

pub enum TraversalOperation {
    Parameter { parameter: TraversalParameter },
    Function { argument: Option<Box<TraversalOperation>>, result: Option<Box<TraversalOperation>> },
    UnaryApplication { argument_variance: Variance, argument: Box<TraversalOperation> },
    // Bifunctor and Profunctor are covariant in their second argument.
    BinaryApplication { first_variance: Variance, arguments: BinaryApplicationArguments },
    Record { fields: Vec<RecordFieldRecipe> },
}

pub enum BinaryApplicationArguments {
    First(Box<TraversalOperation>),
    Second(Box<TraversalOperation>),
    Both { first: Box<TraversalOperation>, second: Box<TraversalOperation> },
}

impl BinaryApplicationArguments {
    pub fn operations(&self) -> (Option<&TraversalOperation>, Option<&TraversalOperation>) {
        match self {
            BinaryApplicationArguments::First(first) => (Some(first), None),
            BinaryApplicationArguments::Second(second) => (None, Some(second)),
            BinaryApplicationArguments::Both { first, second } => (Some(first), Some(second)),
        }
    }
}

pub struct RecordFieldRecipe {
    pub label: SmolStr,
    pub operation: TraversalOperation,
}

struct DerivedParameter {
    name: Name,
    traversal_parameter: TraversalParameter,
    expected: Variance,
    unary_class: Option<ClassReference>,
    function_policy: FunctionPolicy,
}

enum DerivedRigids {
    Invalid,
    Single { parameter: DerivedParameter, binary_class: Option<ClassReference> },
    Pair { first: DerivedParameter, second: DerivedParameter, binary_class: Option<ClassReference> },
}

impl DerivedRigids {
    fn get(&self, name: Name) -> Option<&DerivedParameter> {
        self.iter().find(|parameter| parameter.name == name)
    }

    fn iter(&self) -> impl Iterator<Item = &DerivedParameter> {
        let (first, second) = match self {
            DerivedRigids::Invalid => (None, None),
            DerivedRigids::Single { parameter, .. } => (Some(parameter), None),
            DerivedRigids::Pair { first, second, .. } => (Some(first), Some(second)),
        };
        first.into_iter().chain(second)
    }

    fn binary_class(&self) -> Option<ClassReference> {
        match self {
            DerivedRigids::Single { binary_class, .. }
            | DerivedRigids::Pair { binary_class, .. } => *binary_class,
            DerivedRigids::Invalid => None,
        }
    }

    fn supports_contravariant_traversal(&self) -> bool {
        !matches!(self, DerivedRigids::Invalid)
            && self
                .iter()
                .all(|parameter| matches!(parameter.function_policy, FunctionPolicy::Allow))
    }
}

pub fn generate_variance_constraints<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    data_file: FileId,
    data_id: TypeItemId,
    derived_type: TypeId,
    config: VarianceConfig,
    available_constraints: &[TypeId],
) -> QueryResult<VarianceRecipe>
where
    Q: ExternalQueries,
{
    let constructor_ids = tools::lookup_data_constructors(context, data_file, data_id)?;
    let mut constructors = Vec::with_capacity(constructor_ids.len());
    let mut valid = true;
    for constructor_id in constructor_ids {
        let constructor_t = toolkit::lookup_file_term(state, context, data_file, constructor_id)?;
        let (fields, rigids) =
            extract_fields_with_rigids(state, context, constructor_t, derived_type, config)?;
        valid &= !matches!(rigids, DerivedRigids::Invalid);

        let mut checker = VarianceFieldChecker {
            state,
            context,
            rigids: &rigids,
            available_constraints,
            valid: &mut valid,
        };
        let mut field_recipes = Vec::with_capacity(fields.len());
        for field in fields {
            let operation = checker.check(field, Variance::Covariant)?;
            field_recipes.push(operation);
        }
        constructors.push(ConstructorRecipe { constructor_id, fields: field_recipes });
    }

    Ok(VarianceRecipe { constructors, valid })
}

fn extract_fields_with_rigids<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    constructor_t: TypeId,
    derived_type: TypeId,
    config: VarianceConfig,
) -> QueryResult<(Vec<TypeId>, DerivedRigids)>
where
    Q: ExternalQueries,
{
    let (_, arguments) = toolkit::extract_all_applications(state, context, derived_type)?;
    let mut arguments = arguments.iter().copied();
    let mut current = constructor_t;
    let mut names = Vec::new();

    loop {
        current = normalise::expand(state, context, current)?;
        let Type::Forall(binder_id, inner) = *context.lookup_type(current) else {
            break;
        };

        let binder = context.lookup_forall_binder(binder_id);
        let replacement = arguments
            .next()
            .map(|argument| match argument {
                ApplicationArgument::Kind(argument) | ApplicationArgument::Type(argument) => {
                    argument
                }
            })
            .unwrap_or_else(|| {
                let rigid = state.fresh_rigid(context.queries, binder.kind);
                let Type::Rigid(name, _, _) = *context.lookup_type(rigid) else {
                    unreachable!("fresh_rigid must create Type::Rigid")
                };
                names.push(name);
                rigid
            });

        current = SubstituteName::one(state, context, binder.name, replacement, inner)?;
    }

    let rigids = match (config, &names[..]) {
        (VarianceConfig::Single { parameter, binary_class }, [.., name]) => DerivedRigids::Single {
            parameter: DerivedParameter {
                name: *name,
                traversal_parameter: TraversalParameter::First,
                expected: parameter.variance,
                unary_class: parameter.unary_class,
                function_policy: parameter.function_policy,
            },
            binary_class,
        },
        (VarianceConfig::Pair { first, second, binary_class }, [.., a, b]) => DerivedRigids::Pair {
            first: DerivedParameter {
                name: *a,
                traversal_parameter: TraversalParameter::First,
                expected: first.variance,
                unary_class: first.unary_class,
                function_policy: first.function_policy,
            },
            second: DerivedParameter {
                name: *b,
                traversal_parameter: TraversalParameter::Second,
                expected: second.variance,
                unary_class: second.unary_class,
                function_policy: second.function_policy,
            },
            binary_class,
        },
        _ => {
            state.insert_error(ErrorKind::CannotDeriveForType { type_id: derived_type });
            DerivedRigids::Invalid
        }
    };

    let toolkit::InspectFunction { arguments: fields, .. } =
        toolkit::inspect_function(state, context, current)?;

    Ok((fields, rigids))
}

struct VarianceFieldChecker<'state, 'context, 'queries, 'rigids, Q: ExternalQueries> {
    state: &'state mut CheckState,
    context: &'context CheckContext<'queries, Q>,
    rigids: &'rigids DerivedRigids,
    available_constraints: &'rigids [TypeId],
    valid: &'state mut bool,
}

#[derive(Clone, Copy)]
struct TypeApplication {
    type_id: TypeId,
    function: TypeId,
    argument: TypeId,
}

impl<Q> VarianceFieldChecker<'_, '_, '_, '_, Q>
where
    Q: ExternalQueries,
{
    fn check(
        &mut self,
        type_id: TypeId,
        variance: Variance,
    ) -> QueryResult<Option<TraversalOperation>> {
        let type_id = normalise::expand(self.state, self.context, type_id)?;

        if let Some((argument, result)) =
            toolkit::decompose_function(self.state, self.context, type_id)?
        {
            // Function results can be transformed pointwise when deriving `Functor`, but an
            // arbitrary function cannot be folded or traversed structurally. For example:
            //
            //   data Reader a = Reader (Int -> a)
            //
            // A fold has no finite collection of `a` values to consume without an `Int`.
            // An applicative traversal encounters a separate obstruction:
            //
            //   traverse :: (a -> m b) -> Reader a -> m (Reader b)
            //   function :: a -> m b
            //   program  :: Int -> a
            //
            // Applying `function` pointwise produces `Int -> m b`, but reconstructing
            // `Reader b` inside the applicative requires `m (Int -> b)`. An arbitrary
            // `Applicative m` cannot exchange the `Int ->` and `m` constructors. Fold and
            // traversal derivation therefore reject function types containing a derived
            // parameter, while fixed function fields still require no operation.
            let mut allowed = true;
            for parameter in self.rigids.iter() {
                if matches!(parameter.function_policy, FunctionPolicy::Reject)
                    && toolkit::contains_rigid(self.state, self.context, type_id, parameter.name)?
                {
                    allowed = false;
                    break;
                }
            }
            *self.valid &= allowed;
            if !allowed {
                self.state.insert_error(ErrorKind::CannotDeriveForType { type_id });
                return Ok(None);
            }

            let argument = self.check(argument, variance.flip())?;
            let result = self.check(result, variance)?;
            if argument.is_some() || result.is_some() {
                return Ok(Some(TraversalOperation::Function {
                    argument: argument.map(Box::new),
                    result: result.map(Box::new),
                }));
            }
            return Ok(None);
        }

        match *self.context.lookup_type(type_id) {
            Type::Rigid(name, _, _) => {
                if let Some(parameter) = self.rigids.get(name) {
                    *self.valid &=
                        emit_variance_error(self.state, type_id, variance, parameter.expected);
                    return Ok(Some(TraversalOperation::Parameter {
                        parameter: parameter.traversal_parameter,
                    }));
                }
            }
            Type::Application(function, argument) => {
                let function = normalise::expand(self.state, self.context, function)?;
                if function == self.context.prim.record {
                    return self.check(argument, variance);
                }

                let application = TypeApplication { type_id, function, argument };
                if self.rigids.supports_contravariant_traversal() {
                    return self.check_mixed_application(application, variance);
                }
                if let Some(binary_class) = self.rigids.binary_class() {
                    return self.check_configured_binary_application(
                        application,
                        variance,
                        binary_class,
                    );
                }
                return self.check_configured_unary_application(application, variance);
            }
            Type::KindApplication(_, argument) => {
                return self.check(argument, variance);
            }
            Type::Row(row_id) => {
                let row = self.context.lookup_row_type(row_id);
                let mut fields = Vec::new();
                for field in row.fields.iter() {
                    let operation = self.check(field.id, variance)?;
                    if let Some(operation) = operation {
                        fields.push(RecordFieldRecipe { label: field.label.clone(), operation });
                    }
                }
                if let Some(tail) = row.tail {
                    self.check(tail, variance)?;
                }
                if !fields.is_empty() {
                    return Ok(Some(TraversalOperation::Record { fields }));
                }
            }
            _ => {}
        }

        Ok(None)
    }

    fn check_configured_unary_application(
        &mut self,
        application: TypeApplication,
        variance: Variance,
    ) -> QueryResult<Option<TraversalOperation>> {
        for parameter in self.rigids.iter() {
            if toolkit::contains_rigid(
                self.state,
                self.context,
                application.argument,
                parameter.name,
            )? {
                *self.valid &= emit_variance_error(
                    self.state,
                    application.type_id,
                    variance,
                    parameter.expected,
                );
                if variance == parameter.expected {
                    self.emit_unary_constraint(parameter, application.function);
                }
            }
        }

        let argument = self.check(application.argument, variance)?;
        Ok(argument.map(|argument| TraversalOperation::UnaryApplication {
            argument_variance: Variance::Covariant,
            argument: Box::new(argument),
        }))
    }

    fn check_configured_binary_application(
        &mut self,
        application: TypeApplication,
        variance: Variance,
        binary_class: ClassReference,
    ) -> QueryResult<Option<TraversalOperation>> {
        let Some((binary_head, first_type)) =
            toolkit::decompose_type_application(self.state, self.context, application.function)?
        else {
            return self.check_configured_unary_application(application, variance);
        };

        let mut head_is_fixed = true;
        for parameter in self.rigids.iter() {
            if toolkit::contains_rigid(self.state, self.context, binary_head, parameter.name)? {
                self.state
                    .insert_error(ErrorKind::CannotDeriveForType { type_id: application.type_id });
                *self.valid = false;
                head_is_fixed = false;
                break;
            }
        }

        let first_is_valid =
            self.validate_application_argument(application.type_id, first_type, variance)?;
        let second_is_valid = self.validate_application_argument(
            application.type_id,
            application.argument,
            variance,
        )?;
        *self.valid &= first_is_valid && second_is_valid;

        let first = self.check(first_type, variance)?;
        let second = self.check(application.argument, variance)?;

        if let Some(first) = first {
            if *self.valid && head_is_fixed && first_is_valid && second_is_valid {
                tools::emit_constraint(self.context, self.state, binary_class, binary_head);
            }
            let arguments = match second {
                Some(second) => BinaryApplicationArguments::Both {
                    first: Box::new(first),
                    second: Box::new(second),
                },
                None => BinaryApplicationArguments::First(Box::new(first)),
            };
            return Ok(Some(TraversalOperation::BinaryApplication {
                first_variance: Variance::Covariant,
                arguments,
            }));
        }

        if let Some(second) = second {
            // A right-only occurrence can use either the unary class for the partially
            // applied constructor or the binary class for its head. Match purs by using
            // the binary class only when the unary dictionary is unavailable.
            let binary_available = self.has_instance_head(binary_class, binary_head)?;
            let unary_available = self.unary_instances_available(
                application.argument,
                variance,
                application.function,
            )?;
            if binary_available && !unary_available {
                if *self.valid && head_is_fixed && second_is_valid {
                    tools::emit_constraint(self.context, self.state, binary_class, binary_head);
                }
                let arguments = BinaryApplicationArguments::Second(Box::new(second));
                return Ok(Some(TraversalOperation::BinaryApplication {
                    first_variance: Variance::Covariant,
                    arguments,
                }));
            }

            if *self.valid && head_is_fixed && second_is_valid {
                self.emit_unary_constraints(application.argument, variance, application.function)?;
            }
            return Ok(Some(TraversalOperation::UnaryApplication {
                argument_variance: Variance::Covariant,
                argument: Box::new(second),
            }));
        }

        Ok(None)
    }

    fn validate_application_argument(
        &mut self,
        type_id: TypeId,
        argument: TypeId,
        variance: Variance,
    ) -> QueryResult<bool> {
        let mut valid = true;
        for parameter in self.rigids.iter() {
            if toolkit::contains_rigid(self.state, self.context, argument, parameter.name)? {
                valid &= emit_variance_error(self.state, type_id, variance, parameter.expected);
            }
        }
        Ok(valid)
    }

    fn emit_unary_constraints(
        &mut self,
        argument: TypeId,
        variance: Variance,
        function: TypeId,
    ) -> QueryResult<()> {
        for parameter in self.rigids.iter() {
            if variance == parameter.expected
                && toolkit::contains_rigid(self.state, self.context, argument, parameter.name)?
            {
                self.emit_unary_constraint(parameter, function);
            }
        }
        Ok(())
    }

    fn unary_instances_available(
        &mut self,
        argument: TypeId,
        variance: Variance,
        function: TypeId,
    ) -> QueryResult<bool> {
        for parameter in self.rigids.iter() {
            if variance != parameter.expected
                || !toolkit::contains_rigid(self.state, self.context, argument, parameter.name)?
            {
                continue;
            }

            let Some(class) = parameter.unary_class else {
                return Ok(false);
            };
            if !self.has_instance_head(class, function)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn has_instance_head(&mut self, class: ClassReference, argument: TypeId) -> QueryResult<bool> {
        has_instance_head(self.state, self.context, self.available_constraints, class, argument)
    }

    fn emit_unary_constraint(&mut self, parameter: &DerivedParameter, function: TypeId) {
        if let Some(class) = parameter.unary_class {
            tools::emit_constraint(self.context, self.state, class, function);
        } else {
            self.state.insert_error(ErrorKind::DeriveMissingFunctor);
            *self.valid = false;
        }
    }
}

impl<Q> VarianceFieldChecker<'_, '_, '_, '_, Q>
where
    Q: ExternalQueries,
{
    fn check_mixed_application(
        &mut self,
        application: TypeApplication,
        variance: Variance,
    ) -> QueryResult<Option<TraversalOperation>> {
        let Some((binary_head, first_type)) =
            toolkit::decompose_type_application(self.state, self.context, application.function)?
        else {
            return self.check_mixed_unary_application(application, variance);
        };

        if self.contains_parameter(binary_head)? {
            self.reject(application.type_id);
            return Ok(None);
        }

        // Match upstream's instance-driven priority. Once an edge class is available, a
        // variance error beneath that edge is final rather than a reason to try the next class.
        if let Some(bifunctor) = self.context.known_types.bifunctor
            && has_instance_head(
                self.state,
                self.context,
                self.available_constraints,
                bifunctor,
                binary_head,
            )?
        {
            return self.check_mixed_binary_application(
                application,
                binary_head,
                first_type,
                variance,
                Variance::Covariant,
                bifunctor,
            );
        }

        if let Some(profunctor) = self.context.known_types.profunctor
            && has_instance_head(
                self.state,
                self.context,
                self.available_constraints,
                profunctor,
                binary_head,
            )?
        {
            return self.check_mixed_binary_application(
                application,
                binary_head,
                first_type,
                variance,
                Variance::Contravariant,
                profunctor,
            );
        }

        if self.contains_parameter(first_type)? {
            self.reject(application.type_id);
            return Ok(None);
        }

        self.check_mixed_unary_application(application, variance)
    }

    fn check_mixed_unary_application(
        &mut self,
        application: TypeApplication,
        variance: Variance,
    ) -> QueryResult<Option<TraversalOperation>> {
        let function_mentions_parameter = self.contains_parameter(application.function)?;
        let argument_mentions_parameter = self.contains_parameter(application.argument)?;
        if function_mentions_parameter {
            self.reject(application.type_id);
            return Ok(None);
        }
        if !argument_mentions_parameter {
            return Ok(None);
        }

        if let Some(functor) = self.context.known_types.functor
            && has_instance_head(
                self.state,
                self.context,
                self.available_constraints,
                functor,
                application.function,
            )?
        {
            let argument = self.check(application.argument, variance)?;
            if let Some(argument) = argument {
                if *self.valid {
                    tools::emit_constraint(self.context, self.state, functor, application.function);
                }
                return Ok(Some(TraversalOperation::UnaryApplication {
                    argument_variance: Variance::Covariant,
                    argument: Box::new(argument),
                }));
            }
            return Ok(None);
        }

        if let Some(contravariant) = self.context.known_types.contravariant
            && has_instance_head(
                self.state,
                self.context,
                self.available_constraints,
                contravariant,
                application.function,
            )?
        {
            let argument = self.check(application.argument, variance.flip())?;
            if let Some(argument) = argument {
                if *self.valid {
                    tools::emit_constraint(
                        self.context,
                        self.state,
                        contravariant,
                        application.function,
                    );
                }
                return Ok(Some(TraversalOperation::UnaryApplication {
                    argument_variance: Variance::Contravariant,
                    argument: Box::new(argument),
                }));
            }
            return Ok(None);
        }

        self.reject(application.type_id);
        Ok(None)
    }

    fn check_mixed_binary_application(
        &mut self,
        application: TypeApplication,
        binary_head: TypeId,
        first_type: TypeId,
        variance: Variance,
        first_variance: Variance,
        class: ClassReference,
    ) -> QueryResult<Option<TraversalOperation>> {
        let first_type_variance = match first_variance {
            Variance::Covariant => variance,
            Variance::Contravariant => variance.flip(),
        };
        let first = self.check(first_type, first_type_variance)?;
        let second = self.check(application.argument, variance)?;

        // A right-only binary occurrence uses the simpler partially applied Functor when
        // available. The recipe must record that choice because generation must call `map`
        // rather than the binary operation selected above.
        if first.is_none()
            && second.is_some()
            && let Some(functor) = self.context.known_types.functor
            && has_instance_head(
                self.state,
                self.context,
                self.available_constraints,
                functor,
                application.function,
            )?
        {
            if *self.valid {
                tools::emit_constraint(self.context, self.state, functor, application.function);
            }
            let Some(second) = second else {
                unreachable!("right-only traversal requires a second operation")
            };
            return Ok(Some(TraversalOperation::UnaryApplication {
                argument_variance: Variance::Covariant,
                argument: Box::new(second),
            }));
        }

        let arguments = match (first, second) {
            (Some(first), Some(second)) => BinaryApplicationArguments::Both {
                first: Box::new(first),
                second: Box::new(second),
            },
            (Some(first), None) => BinaryApplicationArguments::First(Box::new(first)),
            (None, Some(second)) => BinaryApplicationArguments::Second(Box::new(second)),
            (None, None) => return Ok(None),
        };

        if *self.valid {
            tools::emit_constraint(self.context, self.state, class, binary_head);
        }
        Ok(Some(TraversalOperation::BinaryApplication { first_variance, arguments }))
    }

    fn contains_parameter(&mut self, type_id: TypeId) -> QueryResult<bool> {
        for parameter in self.rigids.iter() {
            if toolkit::contains_rigid(self.state, self.context, type_id, parameter.name)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn reject(&mut self, type_id: TypeId) {
        self.state.insert_error(ErrorKind::CannotDeriveForType { type_id });
        *self.valid = false;
    }
}

fn has_instance_head<Q>(
    state: &mut CheckState,
    context: &CheckContext<Q>,
    available_constraints: &[TypeId],
    class: ClassReference,
    argument: TypeId,
) -> QueryResult<bool>
where
    Q: ExternalQueries,
{
    // Derivation tests constructor-head availability rather than solving the exact
    // candidate constraint. For example, `Foldable (p Int)` advertises unary traversal
    // for the head `p`; the emitted wanted still checks the complete application.
    let (argument_head, _) = toolkit::extract_type_application(state, context, argument)?;

    let type_heads_equal = |context: &CheckContext<Q>, left: TypeId, right: TypeId| {
        if left == right {
            return true;
        }

        match (context.lookup_type(left), context.lookup_type(right)) {
            (&Type::Constructor(left_file, left_id), &Type::Constructor(right_file, right_id)) => {
                left_file == right_file && left_id == right_id
            }
            // Generalisation can reconstruct a rigid's kind while preserving the binder
            // name. Constructor-head availability depends on that binder identity, not
            // its kind ID.
            (&Type::Rigid(left_name, ..), &Type::Rigid(right_name, ..)) => left_name == right_name,
            (&Type::Free(left_name), &Type::Free(right_name)) => left_name == right_name,
            _ => false,
        }
    };

    let mut canonical_constraints = Vec::with_capacity(available_constraints.len());
    for &available in available_constraints {
        let Some(available) = constraint::canonical::canonicalise(state, context, available)?
        else {
            continue;
        };
        canonical_constraints.push(available);
    }
    let canonical_constraints =
        constraint::elaborate::elaborate_superclasses(state, context, &canonical_constraints)?;

    for available in canonical_constraints {
        let available = state.canonicals[available].clone();
        if (available.file_id, available.type_id) != class {
            continue;
        }
        let [ApplicationArgument::Type(available_argument)] = available.arguments.as_ref() else {
            continue;
        };
        let (available_head, _) =
            toolkit::extract_type_application(state, context, *available_argument)?;
        if type_heads_equal(context, available_head, argument_head) {
            return Ok(true);
        }
    }

    let class_type = context.queries.intern_type(Type::Constructor(class.0, class.1));
    let constraint_type = context.intern_application(class_type, argument);
    let Some(constraint) = constraint::canonical::canonicalise(state, context, constraint_type)?
    else {
        return Ok(false);
    };
    let instances = constraint::instances::collect_instance_chains(state, context, constraint)?;
    for chain in instances.chains() {
        for candidate in chain {
            let Some(candidate) =
                toolkit::instance_info(state, context, candidate.instance.signature, class)?
            else {
                continue;
            };
            let [ApplicationArgument::Type(candidate_argument)] = candidate.arguments.as_slice()
            else {
                continue;
            };
            let (candidate_head, _) =
                toolkit::extract_type_application(state, context, *candidate_argument)?;
            if type_heads_equal(context, candidate_head, argument_head) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn emit_variance_error(
    state: &mut CheckState,
    type_id: TypeId,
    actual: Variance,
    expected: Variance,
) -> bool {
    if actual == expected {
        return true;
    }

    match actual {
        Variance::Covariant => state.insert_error(ErrorKind::CovariantOccurrence { type_id }),
        Variance::Contravariant => {
            state.insert_error(ErrorKind::ContravariantOccurrence { type_id })
        }
    }
    false
}
