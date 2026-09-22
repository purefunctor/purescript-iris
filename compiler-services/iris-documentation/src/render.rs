use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use building::QueryEngine;
use checking::PrettyQueries;
use checking::core::pretty::PrettyNames;
use files::FileId;

use crate::{Error, schema};

#[derive(Debug)]
pub struct PackageInput<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub license: Option<&'a str>,
    pub description: Option<&'a str>,
    pub dependencies: &'a BTreeMap<String, String>,
    pub location: Option<&'a schema::Location>,
    pub modules: &'a [FileId],
}

fn module_name(engine: &QueryEngine, file_id: FileId) -> Result<Option<String>, Error> {
    let content = engine.content(file_id)?;
    let (parsed, _) = engine.parsed(file_id)?;
    Ok(parsed.module_name(&content).map(|name| name.to_string()))
}

struct TypeEncoder<'a> {
    engine: &'a QueryEngine,
    checked: Arc<checking::CheckedModule>,
    package_by_file: &'a [(FileId, &'a str)],
    names: PrettyNames,
}

impl<'a> TypeEncoder<'a> {
    fn new(
        engine: &'a QueryEngine,
        checked: Arc<checking::CheckedModule>,
        package_by_file: &'a [(FileId, &'a str)],
    ) -> TypeEncoder<'a> {
        let names = PrettyNames::new();
        TypeEncoder { engine, checked, package_by_file, names }
    }

    fn encode_signature(&mut self, id: checking::TypeId) -> Result<schema::Type, Error> {
        self.names.reset();
        self.encode_type(id)
    }

    fn encode_declaration(
        &mut self,
        binders: impl IntoIterator<Item = checking::core::ForallBinderId>,
    ) -> Result<schema::TypeDeclaration, Error> {
        self.names.reset();
        let binders = self.encode_forall_binders(binders)?;
        Ok(schema::TypeDeclaration { binders })
    }

    fn encode_class_declaration(
        &mut self,
        class: checking::core::CheckedClass,
    ) -> Result<(schema::TypeDeclaration, Vec<schema::Type>), Error> {
        self.names.reset();
        let binders = self.encode_forall_binders(class.type_parameters.iter().copied())?;

        let superclasses = class
            .superclasses
            .into_iter()
            .map(|superclass| self.encode_type(superclass.constraint));
        let superclasses = superclasses.collect::<Result<Vec<_>, Error>>()?;

        Ok((schema::TypeDeclaration { binders }, superclasses))
    }

    fn encode_synonym_equation(
        &mut self,
        synonym: checking::core::CheckedSynonym,
    ) -> Result<schema::TypeSynonymEquation, Error> {
        self.names.reset();
        let binders = self.encode_forall_binder_values(synonym.parameters)?;
        let expansion = self.encode_type(synonym.expansion)?;
        Ok(schema::TypeSynonymEquation { binders, expansion })
    }

    fn encode_type(&mut self, id: checking::TypeId) -> Result<schema::Type, Error> {
        let expression = match *self.engine.lookup_type(id) {
            checking::Type::Application(function, argument) => schema::Type::Application {
                function: self.encode_boxed_type(function)?,
                argument: self.encode_boxed_type(argument)?,
            },
            checking::Type::KindApplication(function, argument) => schema::Type::KindApplication {
                function: self.encode_boxed_type(function)?,
                argument: self.encode_boxed_type(argument)?,
            },
            checking::Type::Forall(binder, body) => schema::Type::Forall {
                binder: self.encode_forall_binder(binder)?,
                body: self.encode_boxed_type(body)?,
            },
            checking::Type::Constrained(constraint, body) => schema::Type::Constrained {
                constraint: self.encode_boxed_type(constraint)?,
                body: self.encode_boxed_type(body)?,
            },
            checking::Type::Function(argument, result) => schema::Type::Function {
                argument: self.encode_boxed_type(argument)?,
                result: self.encode_boxed_type(result)?,
            },
            checking::Type::Kinded(expression, kind) => schema::Type::Kinded {
                expression: self.encode_boxed_type(expression)?,
                kind: self.encode_boxed_type(kind)?,
            },
            checking::Type::Constructor(file_id, type_id) => schema::Type::Constructor {
                reference: self.resolve_type_reference(file_id, type_id)?,
            },
            checking::Type::Integer(value) => schema::Type::Integer { value },
            checking::Type::String(kind, ref value) => {
                let value = match value.to_utf8() {
                    Ok(value) => schema::StringLiteralValue::Utf8(value),
                    Err(_) => schema::StringLiteralValue::Utf16(value.as_utf16().to_vec()),
                };
                schema::Type::String { kind: kind.into(), value }
            }
            checking::Type::Row(row_id) => {
                let row = self.engine.lookup_row_type(row_id);
                let fields = row.fields.iter().map(|field| {
                    let t = self.encode_type(field.id)?;
                    Ok(schema::TypeRowField { label: field.label.to_string(), t })
                });

                let fields = fields.collect::<Result<Vec<_>, Error>>()?;
                let tail = row.tail.map(|id| self.encode_boxed_type(id)).transpose()?;

                schema::Type::Row { fields, tail }
            }
            checking::Type::Rigid(name, _, kind) => schema::Type::Rigid {
                name: self.display_name(name),
                kind: self.encode_boxed_type(kind)?,
            },
            checking::Type::Unification(id) => schema::Type::Unification { id },
            checking::Type::Free(name_id) => {
                schema::Type::Free { name: self.engine.lookup_smol_str(name_id).to_string() }
            }
            checking::Type::Unknown(name_id) => {
                schema::Type::Unknown { name: self.engine.lookup_smol_str(name_id).to_string() }
            }
        };

        Ok(expression)
    }

    fn encode_boxed_type(&mut self, id: checking::TypeId) -> Result<Box<schema::Type>, Error> {
        Ok(Box::new(self.encode_type(id)?))
    }

    fn encode_forall_binder(
        &mut self,
        id: checking::core::ForallBinderId,
    ) -> Result<schema::TypeBinder, Error> {
        let binder = self.engine.lookup_forall_binder(id);
        self.encode_forall_binder_value(binder)
    }

    fn encode_forall_binders(
        &mut self,
        binders: impl IntoIterator<Item = checking::core::ForallBinderId>,
    ) -> Result<Vec<schema::TypeBinder>, Error> {
        let binders = binders.into_iter().map(|binder| self.encode_forall_binder(binder));
        binders.collect()
    }

    fn encode_forall_binder_values(
        &mut self,
        binders: impl IntoIterator<Item = checking::core::ForallBinder>,
    ) -> Result<Vec<schema::TypeBinder>, Error> {
        let binders = binders.into_iter().map(|binder| self.encode_forall_binder_value(binder));
        binders.collect()
    }

    fn encode_forall_binder_value(
        &mut self,
        binder: checking::core::ForallBinder,
    ) -> Result<schema::TypeBinder, Error> {
        let name = self.display_name(binder.name);
        let kind = self.encode_boxed_type(binder.kind)?;

        Ok(schema::TypeBinder { name, visible: binder.visible, kind })
    }

    fn resolve_type_reference(
        &self,
        file_id: FileId,
        type_id: indexing::TypeItemId,
    ) -> Result<schema::TypeReference, Error> {
        let package = self.package_by_file.iter().find_map(|&(id, package)| {
            if id == file_id { Some(package.to_string()) } else { None }
        });

        let module = module_name(self.engine, file_id)?;

        let indexed = self.engine.indexed(file_id)?;
        let name = indexed.items[type_id].name.as_ref().map(|name| name.to_string());

        Ok(schema::TypeReference { package, module, name })
    }

    fn display_name(&mut self, name: checking::core::Name) -> String {
        self.names.display_name(self.engine, &self.checked.names, name).to_string()
    }
}

struct ModuleEncoder<'a> {
    file_id: FileId,
    indexed: Arc<indexing::IndexedModule>,
    lowered: Arc<lowering::LoweredModule>,
    documented: Arc<documenting::DocumentedModule>,
    checked: Arc<checking::CheckedModule>,
    type_encoder: TypeEncoder<'a>,
}

impl<'a> ModuleEncoder<'a> {
    fn new(
        engine: &'a QueryEngine,
        file_id: FileId,
        package_by_file: &'a [(FileId, &'a str)],
    ) -> Result<(Option<String>, ModuleEncoder<'a>), Error> {
        let name = module_name(engine, file_id)?;
        let indexed = engine.indexed(file_id)?;
        let lowered = engine.lowered(file_id)?;
        let checked = engine.checked(file_id)?;
        let documented = engine.documented(file_id)?;

        let type_encoder = TypeEncoder::new(engine, Arc::clone(&checked), package_by_file);

        Ok((name, ModuleEncoder { file_id, indexed, lowered, documented, checked, type_encoder }))
    }

    fn encode_signature(&mut self, id: checking::TypeId) -> Result<schema::Type, Error> {
        self.type_encoder.encode_signature(id)
    }

    fn encode_term_items(
        &mut self,
        terms: impl IntoIterator<Item = indexing::TermItemId>,
    ) -> Result<Vec<schema::TermItem>, Error> {
        terms.into_iter().map(|term_id| self.encode_term_item(term_id)).collect()
    }

    fn encode_term_item(
        &mut self,
        term_id: indexing::TermItemId,
    ) -> Result<schema::TermItem, Error> {
        let term_item = &self.indexed.items[term_id];
        let term_documentation = self.documented.terms.get(&term_id);

        let name = term_item.name.as_ref().map(|name| name.to_string());
        let documentation =
            term_documentation.and_then(|documentation| optional_string(documentation));
        let signature = term_signature(term_id, &self.checked)
            .map(|signature| self.type_encoder.encode_signature(signature))
            .transpose()?;

        Ok(schema::TermItem { name, documentation, signature, kind: term_kind(&term_item.kind) })
    }

    fn encode_instance_item(
        &mut self,
        item_id: InstanceDocumentItemId,
    ) -> Result<schema::TermItem, Error> {
        let (name, documentation, signature, kind) = match item_id {
            InstanceDocumentItemId::Instance(id) => {
                let item = &self.indexed.items[id];
                let documentation = self.documented.instances.get(&id);
                let signature = self.checked.lookup_instance(item.id).map(|item| item.signature);
                (&item.name, documentation, signature, schema::TermKind::Instance)
            }
            InstanceDocumentItemId::Derive(id) => {
                let item = &self.indexed.items[id];
                let documentation = self.documented.derives.get(&id);
                let signature =
                    self.checked.lookup_derived_instance(item.id).map(|item| item.signature);
                (&item.name, documentation, signature, schema::TermKind::Derive)
            }
        };
        let name = name.as_ref().map(ToString::to_string);
        let documentation = documentation.and_then(|documentation| optional_string(documentation));
        let signature =
            signature.map(|signature| self.type_encoder.encode_signature(signature)).transpose()?;
        Ok(schema::TermItem { name, documentation, signature, kind })
    }

    fn encode_instance_items(
        &mut self,
        items: impl IntoIterator<Item = InstanceDocumentItemId>,
    ) -> Result<Vec<schema::TermItem>, Error> {
        items.into_iter().map(|item| self.encode_instance_item(item)).collect()
    }

    fn encode_functional_dependencies(
        &self,
        type_id: indexing::TypeItemId,
        declaration: &schema::TypeDeclaration,
    ) -> Vec<schema::FunctionalDependency> {
        let Some(lowering::TypeItemKind::Class { declaration: Some(class), .. }) =
            self.lowered.tree.get_type_item_kind(type_id)
        else {
            return vec![];
        };

        let dependency_names = |positions: &[u8]| {
            let names = positions.iter().map(|&position| {
                let position = position as usize;
                let binder = &declaration.binders[position];
                String::clone(&binder.name)
            });
            names.collect()
        };

        let functional_dependencies = class.functional_dependencies.iter().map(|dependency| {
            let determiners = dependency_names(dependency.determiners.as_ref());
            let determined = dependency_names(dependency.determined.as_ref());
            schema::FunctionalDependency { determiners, determined }
        });

        functional_dependencies.collect()
    }

    fn encode_type_item(
        &mut self,
        type_id: indexing::TypeItemId,
        instances: impl IntoIterator<Item = InstanceDocumentItemId>,
    ) -> Result<schema::TypeItem, Error> {
        let indexed = Arc::clone(&self.indexed);
        let type_item = &indexed.items[type_id];
        let type_documentation = self.documented.types.get(&type_id);

        let name = type_item.name.as_ref().map(|name| name.to_string());
        let documentation =
            type_documentation.and_then(|documentation| optional_string(documentation));
        let signature = self.checked.lookup_type_item_kind(type_id);
        let signature = signature.map(|signature| self.encode_signature(signature)).transpose()?;
        let instance_ids = instances.into_iter().collect::<Vec<_>>();

        let form = match &type_item.kind {
            indexing::IndexedTypeItemKind::Data { constructors, .. } => {
                let declaration = self.checked.lookup_data_declaration(type_id);
                let declaration = if let Some(declaration) = declaration {
                    let type_parameters = declaration.type_parameters.iter().copied();
                    Some(self.type_encoder.encode_declaration(type_parameters)?)
                } else {
                    None
                };

                let constructors = self.encode_term_items(constructors.iter().copied())?;
                let instances = self.encode_instance_items(instance_ids.iter().copied())?;

                schema::TypeItemForm::Data { signature, declaration, constructors, instances }
            }
            indexing::IndexedTypeItemKind::Newtype { constructors, .. } => {
                let declaration = self.checked.lookup_data_declaration(type_id);
                let declaration = if let Some(declaration) = declaration {
                    let type_parameters = declaration.type_parameters.iter().copied();
                    Some(self.type_encoder.encode_declaration(type_parameters)?)
                } else {
                    None
                };

                let constructors = self.encode_term_items(constructors.iter().copied())?;
                let instances = self.encode_instance_items(instance_ids.iter().copied())?;

                schema::TypeItemForm::Newtype { signature, declaration, constructors, instances }
            }
            indexing::IndexedTypeItemKind::Synonym { .. } => {
                let equation = if let Some(synonym) = self.checked.lookup_synonym(type_id) {
                    Some(self.type_encoder.encode_synonym_equation(synonym)?)
                } else {
                    None
                };

                let instances = self.encode_instance_items(instance_ids.iter().copied())?;

                schema::TypeItemForm::Synonym { signature, equation, instances }
            }
            indexing::IndexedTypeItemKind::Class { members, .. } => {
                let (declaration, superclasses, functional_dependencies) =
                    if let Some(class) = self.checked.lookup_class(type_id) {
                        let (declaration, superclasses) =
                            self.type_encoder.encode_class_declaration(class)?;
                        let functional_dependencies =
                            self.encode_functional_dependencies(type_id, &declaration);
                        (Some(declaration), superclasses, functional_dependencies)
                    } else {
                        (None, vec![], vec![])
                    };

                let members = self.encode_term_items(members.iter().copied())?;
                let instances = self.encode_instance_items(instance_ids.iter().copied())?;

                schema::TypeItemForm::Class {
                    signature,
                    declaration,
                    superclasses,
                    functional_dependencies,
                    members,
                    instances,
                }
            }
            indexing::IndexedTypeItemKind::Foreign { .. } => {
                let instances = self.encode_instance_items(instance_ids.iter().copied())?;

                schema::TypeItemForm::Foreign { signature, instances }
            }
            indexing::IndexedTypeItemKind::Operator { .. } => {
                schema::TypeItemForm::Operator { signature }
            }
        };

        Ok(schema::TypeItem { name, documentation, form })
    }
}

pub fn render_package_manifest(
    engine: &QueryEngine,
    package: &PackageInput<'_>,
) -> Result<schema::Package, Error> {
    let mut modules = vec![];
    for &id in package.modules {
        if let Some(name) = module_name(engine, id)? {
            modules.push(name);
        }
    }

    Ok(schema::Package {
        name: package.name.to_string(),
        version: package.version.to_string(),
        license: package.license.map(str::to_string),
        description: package.description.map(str::to_string),
        dependencies: BTreeMap::clone(package.dependencies),
        location: package.location.cloned(),
        modules,
    })
}

pub fn render_module(
    engine: &QueryEngine,
    file_id: FileId,
    package_by_file: &[(FileId, &str)],
) -> Result<Option<schema::Module>, Error> {
    let (name, mut encoder) = ModuleEncoder::new(engine, file_id, package_by_file)?;

    let Some(name) = name else { return Ok(None) };
    let documentation = optional_string(&encoder.documented.documentation);

    let mut terms = vec![];
    let mut types = vec![];

    let mut nested_terms = NestedTerms::new();
    let mut nested_instances = NestedInstances::new();
    let mut instances_of = collect_instances_of(&encoder, &mut nested_instances);
    collect_constructors_members(&encoder, &mut nested_terms);

    let indexed = Arc::clone(&encoder.indexed);
    for &item_id in indexed.items.ordered_terms() {
        match item_id {
            indexing::OrderedTermItemId::Term(id) if !nested_terms.contains(&id) => {
                terms.push(encoder.encode_term_item(id)?);
            }
            indexing::OrderedTermItemId::Instance(id) => {
                let id = InstanceDocumentItemId::Instance(id);
                if !nested_instances.contains(&id) {
                    terms.push(encoder.encode_instance_item(id)?);
                }
            }
            indexing::OrderedTermItemId::Derive(id) => {
                let id = InstanceDocumentItemId::Derive(id);
                if !nested_instances.contains(&id) {
                    terms.push(encoder.encode_instance_item(id)?);
                }
            }
            indexing::OrderedTermItemId::Term(_) => {}
        }
    }
    for (type_id, _) in indexed.items.iter_types() {
        let instances = instances_of.remove(&type_id).unwrap_or_default();
        types.push(encoder.encode_type_item(type_id, instances)?);
    }

    Ok(Some(schema::Module { name, documentation, terms, types }))
}

fn optional_string(value: &str) -> Option<String> {
    if value.is_empty() { None } else { Some(value.to_string()) }
}

fn collect_constructors_members(encoder: &ModuleEncoder<'_>, nested_terms: &mut NestedTerms) {
    for (_, type_item) in encoder.indexed.items.iter_types() {
        match &type_item.kind {
            indexing::IndexedTypeItemKind::Data { constructors, .. }
            | indexing::IndexedTypeItemKind::Newtype { constructors, .. } => {
                nested_terms.extend(constructors.iter().copied());
            }
            indexing::IndexedTypeItemKind::Class { members, .. } => {
                nested_terms.extend(members.iter().copied());
            }
            _ => {}
        }
    }
}

fn term_kind(kind: &indexing::IndexedTermItemKind) -> schema::TermKind {
    match kind {
        indexing::IndexedTermItemKind::ClassMember { .. } => schema::TermKind::ClassMember,
        indexing::IndexedTermItemKind::Constructor { .. } => schema::TermKind::Constructor,
        indexing::IndexedTermItemKind::Foreign { .. } => schema::TermKind::Foreign,
        indexing::IndexedTermItemKind::Operator { .. } => schema::TermKind::Operator,
        indexing::IndexedTermItemKind::Value { .. } => schema::TermKind::Value,
    }
}

fn term_signature(
    term_id: indexing::TermItemId,
    checked: &checking::CheckedModule,
) -> Option<checking::TypeId> {
    checked.lookup_term_item_type(term_id)
}

type NestedTerms = BTreeSet<indexing::TermItemId>;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InstanceDocumentItemId {
    Instance(indexing::InstanceItemId),
    Derive(indexing::DeriveItemId),
}

type NestedInstances = BTreeSet<InstanceDocumentItemId>;
type InstanceParentMap = BTreeMap<indexing::TypeItemId, Vec<InstanceDocumentItemId>>;
type InstanceParents = BTreeSet<indexing::TypeItemId>;

fn collect_instances_of(
    encoder: &ModuleEncoder<'_>,
    nested_instances: &mut NestedInstances,
) -> InstanceParentMap {
    let mut instances_by_parent = InstanceParentMap::new();

    for &item_id in encoder.indexed.items.instance_sources() {
        let item_id = match item_id {
            indexing::InstanceSourceItemId::Instance(id) => InstanceDocumentItemId::Instance(id),
            indexing::InstanceSourceItemId::Derive(id) => InstanceDocumentItemId::Derive(id),
        };
        let parents = instance_parents(encoder, item_id);
        if parents.is_empty() {
            continue;
        }

        nested_instances.insert(item_id);
        for parent in parents {
            instances_by_parent.entry(parent).or_default().push(item_id);
        }
    }

    instances_by_parent
}

fn instance_parents(
    encoder: &ModuleEncoder<'_>,
    item_id: InstanceDocumentItemId,
) -> InstanceParents {
    let mut parents = InstanceParents::new();

    let (checked_instance, arguments) = match item_id {
        InstanceDocumentItemId::Instance(id) => {
            let item = &encoder.indexed.items[id];
            let checked = encoder.checked.lookup_instance(item.id);
            let arguments = encoder.lowered.tree.get_instance_item(id).map(|item| &item.arguments);
            (checked, arguments)
        }
        InstanceDocumentItemId::Derive(id) => {
            let item = &encoder.indexed.items[id];
            let checked = encoder.checked.lookup_derived_instance(item.id);
            let arguments = encoder.lowered.tree.get_derive_item(id).map(|item| &item.arguments);
            (checked, arguments)
        }
    };

    if let Some(instance) = checked_instance
        && let (parent_file, parent_type) = instance.resolution
        && parent_file == encoder.file_id
    {
        parents.insert(parent_type);
    }

    let Some(arguments) = arguments else { return parents };

    for &argument in arguments.iter() {
        collect_instance_type_parents(encoder, &mut parents, argument);
    }

    parents
}

fn collect_instance_type_parents(
    encoder: &ModuleEncoder<'_>,
    parents: &mut InstanceParents,
    type_id: lowering::TypeId,
) {
    let Some(kind) = encoder.lowered.tree.get_type_kind(type_id) else { return };

    match kind {
        lowering::TypeKind::Constructor { resolution } => {
            if let Some((parent_file, parent_type)) = resolution
                && *parent_file == encoder.file_id
                && instance_type_parent(encoder, *parent_type)
            {
                parents.insert(*parent_type);
            }
        }
        lowering::TypeKind::ApplicationChain { function, arguments } => {
            if let Some(function) = function {
                collect_instance_type_parents(encoder, parents, *function);
            }
            for &argument in arguments.iter() {
                collect_instance_type_parents(encoder, parents, argument);
            }
        }
        lowering::TypeKind::Arrow { argument, result } => {
            if let Some(argument) = argument {
                collect_instance_type_parents(encoder, parents, *argument);
            }
            if let Some(result) = result {
                collect_instance_type_parents(encoder, parents, *result);
            }
        }
        lowering::TypeKind::Constrained { constraint, constrained } => {
            if let Some(constraint) = constraint {
                collect_instance_type_parents(encoder, parents, *constraint);
            }
            if let Some(constrained) = constrained {
                collect_instance_type_parents(encoder, parents, *constrained);
            }
        }
        lowering::TypeKind::Forall { inner, .. } => {
            if let Some(inner) = inner {
                collect_instance_type_parents(encoder, parents, *inner);
            }
        }
        lowering::TypeKind::Kinded { type_, kind } => {
            if let Some(type_) = type_ {
                collect_instance_type_parents(encoder, parents, *type_);
            }
            if let Some(kind) = kind {
                collect_instance_type_parents(encoder, parents, *kind);
            }
        }
        lowering::TypeKind::OperatorChain { head, tail } => {
            if let Some(head) = head {
                collect_instance_type_parents(encoder, parents, *head);
            }
            for pair in tail.iter() {
                if let Some(element) = pair.element {
                    collect_instance_type_parents(encoder, parents, element);
                }
            }
        }
        lowering::TypeKind::Record { items, tail } | lowering::TypeKind::Row { items, tail } => {
            for item in items.iter() {
                if let Some(type_) = item.type_ {
                    collect_instance_type_parents(encoder, parents, type_);
                }
            }
            if let Some(tail) = tail {
                collect_instance_type_parents(encoder, parents, *tail);
            }
        }
        lowering::TypeKind::Parenthesized { parenthesized } => {
            if let Some(parenthesized) = parenthesized {
                collect_instance_type_parents(encoder, parents, *parenthesized);
            }
        }
        lowering::TypeKind::Operator { .. }
        | lowering::TypeKind::Hole
        | lowering::TypeKind::Integer { .. }
        | lowering::TypeKind::String { .. }
        | lowering::TypeKind::Variable { .. }
        | lowering::TypeKind::Wildcard => {}
    }
}

fn instance_type_parent(encoder: &ModuleEncoder<'_>, type_id: indexing::TypeItemId) -> bool {
    matches!(
        encoder.indexed.items[type_id].kind,
        indexing::IndexedTypeItemKind::Data { .. }
            | indexing::IndexedTypeItemKind::Newtype { .. }
            | indexing::IndexedTypeItemKind::Synonym { .. }
            | indexing::IndexedTypeItemKind::Foreign { .. }
    )
}
