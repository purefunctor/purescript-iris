//! Oxc JavaScript program construction and code generation.

use std::path::PathBuf;

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::ast::{
    Argument, ArrowFunctionBody, AssignmentTarget, BindingIdentifier, BindingPattern,
    BindingProperty, BindingRestElement, Declaration, ExportSpecifier, Expression, FormalParameter,
    FormalParameterKind, FormalParameters, Function, FunctionBody, FunctionType,
    ImportDeclarationSpecifier, ImportOrExportKind, LabelIdentifier, ModuleExportName, Program,
    PropertyKey, Statement, StringLiteral, SwitchCase, VariableDeclaration,
    VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::builder::AstBuilder;
use oxc_codegen::{Codegen, CodegenOptions, CodegenReturn, IndentChar};
use oxc_span::{SourceType, Span};
use oxc_syntax::operator::AssignmentOperator;
use smol_str::SmolStr;

use crate::convert::identifier_is_binding;
use crate::program_text::{ProgramNode, ProgramText};
use crate::source_map::{OriginMapping, Position};
use crate::tree::{ExpressionId, Tree, property_is_identifier};

pub(crate) struct Writer<'a> {
    allocator: &'a Allocator,
    builder: AstBuilder<'a>,
    statements: Vec<Statement<'a>>,
    program_text: ProgramText,
    has_eager_throw: bool,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(allocator: &'a Allocator, program_text: ProgramText) -> Writer<'a> {
        Writer {
            allocator,
            builder: AstBuilder::new(allocator),
            statements: Vec::new(),
            program_text,
            has_eager_throw: false,
        }
    }

    fn child(&self) -> Writer<'a> {
        Writer {
            allocator: self.allocator,
            builder: AstBuilder::new(self.allocator),
            statements: Vec::new(),
            program_text: ProgramText::clone(&self.program_text),
            has_eager_throw: false,
        }
    }

    fn span(&self) -> Span {
        self.program_text.span()
    }

    fn named_span(&self) -> Span {
        self.program_text.named_span(self.function_name())
    }

    pub(crate) fn function_name(&self) -> Option<SmolStr> {
        self.program_text.function_name()
    }

    fn text(&self, value: &str) -> &'a str {
        self.allocator.alloc_str(value)
    }

    fn binding(&self, name: &str) -> BindingIdentifier<'a> {
        BindingIdentifier::new(self.span(), self.text(name), &self.builder)
    }

    fn named_binding(&self, name: &str) -> BindingIdentifier<'a> {
        BindingIdentifier::new(self.named_span(), self.text(name), &self.builder)
    }

    fn binding_pattern(&self, name: &str) -> BindingPattern<'a> {
        BindingPattern::new_binding_identifier(self.span(), self.text(name), &self.builder)
    }

    fn named_binding_pattern(&self, name: &str) -> BindingPattern<'a> {
        BindingPattern::new_binding_identifier(self.named_span(), self.text(name), &self.builder)
    }

    fn parameters(&self, parameters: &[SmolStr]) -> oxc_allocator::Box<'a, FormalParameters<'a>> {
        let parameters = parameters.iter().map(|parameter| {
            FormalParameter::new(
                self.named_span(),
                [],
                self.named_binding_pattern(parameter),
                None,
                None,
                false,
                None,
                false,
                false,
                &self.builder,
            )
        });
        let parameters = ArenaVec::from_iter_in(parameters, &self.allocator);
        FormalParameters::boxed(
            self.named_span(),
            FormalParameterKind::FormalParameter,
            parameters,
            None,
            &self.builder,
        )
    }

    fn function_body(
        &self,
        statements: Vec<Statement<'a>>,
    ) -> oxc_allocator::Box<'a, FunctionBody<'a>> {
        let statements = ArenaVec::from_iter_in(statements, &self.allocator);
        FunctionBody::boxed(self.span(), [], statements, &self.builder)
    }

    fn block_statement(&self, statements: Vec<Statement<'a>>) -> Statement<'a> {
        let statements = ArenaVec::from_iter_in(statements, &self.allocator);
        Statement::new_block_statement(self.span(), statements, &self.builder)
    }

    fn arrow_expression(
        &self,
        parameters: &[SmolStr],
        statements: Vec<Statement<'a>>,
    ) -> Expression<'a> {
        let parameters = self.parameters(parameters);
        let body = ArrowFunctionBody::new_function_body(
            self.span(),
            [],
            ArenaVec::from_iter_in(statements, &self.allocator),
            &self.builder,
        );
        Expression::new_arrow_function_expression(
            self.named_span(),
            false,
            None,
            parameters,
            None,
            body,
            &self.builder,
        )
    }

    fn variable_statement(
        &self,
        kind: VariableDeclarationKind,
        binding: BindingPattern<'a>,
        value: Option<Expression<'a>>,
        exported: bool,
    ) -> Statement<'a> {
        let declarator =
            VariableDeclarator::new(self.span(), binding, None, value, false, &self.builder);
        let declarations = ArenaVec::from_array_in([declarator], &self.allocator);
        let declaration =
            VariableDeclaration::boxed(self.span(), kind, declarations, false, &self.builder);
        if exported {
            Statement::new_export_declaration(
                self.span(),
                Declaration::VariableDeclaration(declaration),
                &self.builder,
            )
        } else {
            Statement::VariableDeclaration(declaration)
        }
    }

    pub(crate) fn import_namespace(&mut self, namespace: &str, path: &str) {
        let specifier = ImportDeclarationSpecifier::new_import_namespace_specifier(
            self.span(),
            self.binding(namespace),
            &self.builder,
        );
        let specifiers = ArenaVec::from_array_in([specifier], &self.allocator);
        let source = StringLiteral::new(self.span(), self.text(path), None, &self.builder);
        let statement = Statement::new_import_declaration(
            self.span(),
            Some(specifiers),
            source,
            None,
            None,
            ImportOrExportKind::Value,
            &self.builder,
        );
        self.statements.push(statement);
    }

    pub(crate) fn import_named(&mut self, bindings: &[(&str, &str)], path: &str) {
        let specifiers = bindings.iter().map(|(imported, local)| {
            let imported = self.module_export_name(imported, false);
            ImportDeclarationSpecifier::new_import_specifier(
                self.span(),
                imported,
                self.binding(local),
                ImportOrExportKind::Value,
                &self.builder,
            )
        });
        let specifiers = ArenaVec::from_iter_in(specifiers, &self.allocator);
        let source = StringLiteral::new(self.span(), self.text(path), None, &self.builder);
        let statement = Statement::new_import_declaration(
            self.span(),
            Some(specifiers),
            source,
            None,
            None,
            ImportOrExportKind::Value,
            &self.builder,
        );
        self.statements.push(statement);
    }

    pub(crate) fn constant(
        &mut self,
        tree: &mut Tree<'a>,
        name: &str,
        value: ExpressionId,
        exported: bool,
    ) {
        let statement = self.variable_statement(
            VariableDeclarationKind::Const,
            self.binding_pattern(name),
            Some(tree.expression(value)),
            exported,
        );
        self.statements.push(statement);
    }

    pub(crate) fn constant_object_pattern(
        &mut self,
        tree: &mut Tree<'a>,
        names: &[Option<SmolStr>],
        value: ExpressionId,
    ) {
        let properties = names.iter().enumerate().filter_map(|(index, name)| {
            let name = name.as_deref()?;
            let field = format!("_{}", index + 1);
            let key =
                PropertyKey::new_static_identifier(self.span(), self.text(&field), &self.builder);
            let value = self.binding_pattern(name);
            Some(BindingProperty::new(self.span(), key, value, false, false, &self.builder))
        });
        let properties = ArenaVec::from_iter_in(properties, &self.allocator);
        let pattern =
            BindingPattern::new_object_pattern(self.span(), properties, None, &self.builder);
        let declarator = VariableDeclarator::new(
            self.span(),
            pattern,
            None,
            Some(tree.expression(value)),
            false,
            &self.builder,
        );
        let declarations = ArenaVec::from_array_in([declarator], &self.allocator);
        let declaration = VariableDeclaration::boxed(
            self.span(),
            VariableDeclarationKind::Const,
            declarations,
            false,
            &self.builder,
        );
        self.statements.push(Statement::VariableDeclaration(declaration));
    }

    pub(crate) fn constant_object_rest(
        &mut self,
        tree: &mut Tree<'a>,
        field: &str,
        field_name: &str,
        rest_name: &str,
        value: ExpressionId,
    ) {
        let key = if property_is_identifier(field) {
            PropertyKey::new_static_identifier(self.span(), self.text(field), &self.builder)
        } else {
            PropertyKey::new_string_literal(self.span(), self.text(field), None, &self.builder)
        };
        let field_pattern = self.binding_pattern(field_name);
        let property =
            BindingProperty::new(self.span(), key, field_pattern, false, false, &self.builder);
        let properties = ArenaVec::from_array_in([property], &self.allocator);
        let rest =
            BindingRestElement::boxed(self.span(), self.binding_pattern(rest_name), &self.builder);
        let pattern =
            BindingPattern::new_object_pattern(self.span(), properties, Some(rest), &self.builder);
        let declarator = VariableDeclarator::new(
            self.span(),
            pattern,
            None,
            Some(tree.expression(value)),
            false,
            &self.builder,
        );
        let declarations = ArenaVec::from_array_in([declarator], &self.allocator);
        let declaration = VariableDeclaration::boxed(
            self.span(),
            VariableDeclarationKind::Const,
            declarations,
            false,
            &self.builder,
        );
        self.statements.push(Statement::VariableDeclaration(declaration));
    }

    pub(crate) fn mutable(&mut self, name: &str) {
        let binding = self.binding_pattern(name);
        let statement = self.variable_statement(VariableDeclarationKind::Let, binding, None, false);
        self.statements.push(statement);
    }

    pub(crate) fn mutable_value(&mut self, tree: &mut Tree<'a>, name: &str, value: ExpressionId) {
        let value = tree.expression(value);
        let binding = self.binding_pattern(name);
        let statement =
            self.variable_statement(VariableDeclarationKind::Let, binding, Some(value), false);
        self.statements.push(statement);
    }

    pub(crate) fn assign(&mut self, tree: &mut Tree<'a>, name: &str, value: ExpressionId) {
        let target = AssignmentTarget::new_assignment_target_identifier(
            self.span(),
            self.text(name),
            &self.builder,
        );
        let expression = Expression::new_assignment_expression(
            self.span(),
            AssignmentOperator::Assign,
            target,
            tree.expression(value),
            &self.builder,
        );
        self.statements.push(Statement::new_expression_statement(
            self.span(),
            expression,
            &self.builder,
        ));
    }

    pub(crate) fn return_expression(&mut self, tree: &mut Tree<'a>, value: ExpressionId) {
        self.statements.push(Statement::new_return_statement(
            self.span(),
            Some(tree.expression(value)),
            &self.builder,
        ));
    }

    pub(crate) fn return_function(&mut self, tree: &mut Tree<'a>, function: ExpressionId) {
        self.statements.push(Statement::new_return_statement(
            self.named_span(),
            Some(tree.expression(function)),
            &self.builder,
        ));
    }

    pub(crate) fn break_label(&mut self, label: &str) {
        let label = LabelIdentifier::new(self.span(), self.text(label), &self.builder);
        self.statements.push(Statement::new_break_statement(
            self.span(),
            Some(label),
            &self.builder,
        ));
    }

    pub(crate) fn continue_loop(&mut self) {
        self.statements.push(Statement::new_continue_statement(self.span(), None, &self.builder));
    }

    pub(crate) fn function<R>(
        &mut self,
        name: &str,
        source_name: Option<SmolStr>,
        parameters: Vec<SmolStr>,
        exported: bool,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let _function_name = self.program_text.enter_function_name(source_name);
        let mut body = self.child();
        let result = render(&mut body);
        let function = Function::boxed(
            self.named_span(),
            FunctionType::FunctionDeclaration,
            Some(self.named_binding(name)),
            false,
            false,
            false,
            None,
            None,
            self.parameters(&parameters),
            None,
            Some(self.function_body(body.statements)),
            &self.builder,
        );
        let statement = if exported {
            Statement::new_export_declaration(
                self.span(),
                Declaration::FunctionDeclaration(function),
                &self.builder,
            )
        } else {
            Statement::FunctionDeclaration(function)
        };
        self.statements.push(statement);
        result
    }

    pub(crate) fn constant_arrow<R>(
        &mut self,
        name: &str,
        source_name: Option<SmolStr>,
        parameters: Vec<SmolStr>,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let _function_name = self.program_text.enter_function_name(source_name);
        let mut body = self.child();
        let result = render(&mut body);
        let expression = self.arrow_expression(&parameters, body.statements);
        let binding = self.named_binding_pattern(name);
        let statement = self.variable_statement(
            VariableDeclarationKind::Const,
            binding,
            Some(expression),
            false,
        );
        self.statements.push(statement);
        result
    }

    pub(crate) fn return_arrow<R>(
        &mut self,
        parameters: Vec<SmolStr>,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        let expression = self.arrow_expression(&parameters, body.statements);
        self.statements.push(Statement::new_return_statement(
            self.named_span(),
            Some(expression),
            &self.builder,
        ));
        result
    }

    pub(crate) fn assign_arrow<R>(
        &mut self,
        name: &str,
        parameters: Vec<SmolStr>,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let _function_name = self.program_text.enter_function_name(None);
        let mut body = self.child();
        let result = render(&mut body);
        let expression = self.arrow_expression(&parameters, body.statements);
        let target = AssignmentTarget::new_assignment_target_identifier(
            self.span(),
            self.text(name),
            &self.builder,
        );
        let expression = Expression::new_assignment_expression(
            self.span(),
            AssignmentOperator::Assign,
            target,
            expression,
            &self.builder,
        );
        self.statements.push(Statement::new_expression_statement(
            self.span(),
            expression,
            &self.builder,
        ));
        result
    }

    pub(crate) fn constant_iife<R>(
        &mut self,
        name: &str,
        source_name: Option<SmolStr>,
        exported: bool,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let _function_name = self.program_text.enter_function_name(source_name);
        let mut body = self.child();
        let result = render(&mut body);
        let has_eager_throw = body.has_eager_throw;
        let arrow = self.arrow_expression(&[], body.statements);
        let call = Expression::new_call_expression_with_pure(
            self.named_span(),
            arrow,
            None,
            [],
            false,
            !has_eager_throw,
            &self.builder,
        );
        let binding = self.named_binding_pattern(name);
        let statement =
            self.variable_statement(VariableDeclarationKind::Const, binding, Some(call), exported);
        self.statements.push(statement);
        self.has_eager_throw |= has_eager_throw;
        result
    }

    pub(crate) fn binding_call<R>(
        &mut self,
        target: BindingCallTarget<'_>,
        callee: Expression<'a>,
        name: Expression<'a>,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let _function_name = self.program_text.enter_function_name(None);
        let mut body = self.child();
        let result = render(&mut body);
        let body = self.arrow_expression(&[], body.statements);
        let arguments = [Argument::from(name), Argument::from(body)];
        let arguments = ArenaVec::from_array_in(arguments, &self.allocator);
        let call = Expression::new_call_expression(
            self.span(),
            callee,
            None,
            arguments,
            false,
            &self.builder,
        );
        let statement = match target {
            BindingCallTarget::Constant(name) => {
                let binding = self.binding_pattern(name);
                self.variable_statement(VariableDeclarationKind::Const, binding, Some(call), false)
            }
            BindingCallTarget::Assignment(name) => {
                let target = AssignmentTarget::new_assignment_target_identifier(
                    self.span(),
                    self.text(name),
                    &self.builder,
                );
                let assignment = Expression::new_assignment_expression(
                    self.span(),
                    AssignmentOperator::Assign,
                    target,
                    call,
                    &self.builder,
                );
                Statement::new_expression_statement(self.span(), assignment, &self.builder)
            }
        };
        self.statements.push(statement);
        result
    }

    pub(crate) fn if_else<E>(
        &mut self,
        tree: &mut Tree<'a>,
        condition: ExpressionId,
        render_then: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>) -> Result<(), E>,
        render_else: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut then_writer = self.child();
        render_then(tree, &mut then_writer)?;
        let mut else_writer = self.child();
        render_else(tree, &mut else_writer)?;
        self.has_eager_throw |= then_writer.has_eager_throw || else_writer.has_eager_throw;
        let consequent = self.block_statement(then_writer.statements);
        let alternate = self.block_statement(else_writer.statements);
        self.statements.push(Statement::new_if_statement(
            self.span(),
            tree.expression(condition),
            consequent,
            Some(alternate),
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn if_else_with_state<E, S>(
        &mut self,
        tree: &mut Tree<'a>,
        condition: ExpressionId,
        state: &mut S,
        render_then: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>, &mut S) -> Result<(), E>,
        render_else: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>, &mut S) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut then_writer = self.child();
        render_then(tree, &mut then_writer, state)?;
        let mut else_writer = self.child();
        render_else(tree, &mut else_writer, state)?;
        self.has_eager_throw |= then_writer.has_eager_throw || else_writer.has_eager_throw;
        let consequent = self.block_statement(then_writer.statements);
        let alternate = self.block_statement(else_writer.statements);
        self.statements.push(Statement::new_if_statement(
            self.span(),
            tree.expression(condition),
            consequent,
            Some(alternate),
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn if_block<R>(
        &mut self,
        tree: &mut Tree<'a>,
        condition: ExpressionId,
        render: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(tree, &mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let consequent = self.block_statement(body.statements);
        self.statements.push(Statement::new_if_statement(
            self.span(),
            tree.expression(condition),
            consequent,
            None,
            &self.builder,
        ));
        result
    }

    pub(crate) fn block<R>(&mut self, render: impl FnOnce(&mut Writer<'a>) -> R) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let statement = self.block_statement(body.statements);
        self.statements.push(statement);
        result
    }

    pub(crate) fn labeled_block<R>(
        &mut self,
        label: &str,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let body = self.block_statement(body.statements);
        let label = LabelIdentifier::new(self.span(), self.text(label), &self.builder);
        self.statements.push(Statement::new_labeled_statement(
            self.span(),
            label,
            body,
            &self.builder,
        ));
        result
    }

    pub(crate) fn while_loop<R>(
        &mut self,
        tree: &mut Tree<'a>,
        condition: ExpressionId,
        render: impl FnOnce(&mut Tree<'a>, &mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(tree, &mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let body = self.block_statement(body.statements);
        let condition = tree.expression(condition);
        self.statements.push(Statement::new_while_statement(
            self.span(),
            condition,
            body,
            &self.builder,
        ));
        result
    }

    pub(crate) fn switch<E>(
        &mut self,
        tree: &mut Tree<'a>,
        discriminant: ExpressionId,
        cases: Vec<(ExpressionId, SmolStr)>,
        mut render: impl FnMut(usize, &mut Tree<'a>, &mut Writer<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut switch_cases = Vec::with_capacity(cases.len());
        let mut has_eager_throw = false;
        for (position, (test, comment)) in cases.into_iter().enumerate() {
            let span = self.program_text.line_comment(&comment);
            let mut body = self.child();
            render(position, tree, &mut body)?;
            has_eager_throw |= body.has_eager_throw;
            let test = tree.expression(test);
            let body = self.block_statement(body.statements);
            let consequent = ArenaVec::from_array_in([body], &self.allocator);
            let switch_case = SwitchCase::new(span, Some(test), consequent, &self.builder);
            switch_cases.push(switch_case);
        }
        self.has_eager_throw |= has_eager_throw;
        let discriminant = tree.expression(discriminant);
        let switch_cases = ArenaVec::from_iter_in(switch_cases, &self.allocator);
        self.statements.push(Statement::new_switch_statement(
            self.span(),
            discriminant,
            switch_cases,
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn throw_error(&mut self, message: &str) {
        self.has_eager_throw = true;
        let callee = Expression::new_identifier(self.span(), self.text("Error"), &self.builder);
        let message =
            Expression::new_string_literal(self.span(), self.text(message), None, &self.builder);
        let arguments = ArenaVec::from_array_in([Argument::from(message)], &self.allocator);
        let error =
            Expression::new_new_expression(self.span(), callee, None, arguments, &self.builder);
        self.statements.push(Statement::new_throw_statement(self.span(), error, &self.builder));
    }

    fn module_export_name(&self, name: &str, reference: bool) -> ModuleExportName<'a> {
        if identifier_is_binding(name) {
            if reference {
                ModuleExportName::new_identifier_reference(
                    self.span(),
                    self.text(name),
                    &self.builder,
                )
            } else {
                ModuleExportName::new_identifier_name(self.span(), self.text(name), &self.builder)
            }
        } else {
            ModuleExportName::new_string_literal(self.span(), self.text(name), None, &self.builder)
        }
    }

    pub(crate) fn export(&mut self, local: &str, exported: &str) {
        let specifier = ExportSpecifier::new(
            self.span(),
            self.module_export_name(local, true),
            self.module_export_name(exported, false),
            ImportOrExportKind::Value,
            &self.builder,
        );
        self.statements.push(Statement::new_export_named_declaration(
            self.span(),
            [specifier],
            ImportOrExportKind::Value,
            &self.builder,
        ));
    }

    pub(crate) fn export_from(&mut self, specifiers: Vec<String>, path: &str) {
        let specifiers = specifiers.into_iter().map(|name| {
            ExportSpecifier::new(
                self.span(),
                self.module_export_name(&name, false),
                self.module_export_name(&name, false),
                ImportOrExportKind::Value,
                &self.builder,
            )
        });
        let specifiers = ArenaVec::from_iter_in(specifiers, &self.allocator);
        let source = StringLiteral::new(self.span(), self.text(path), None, &self.builder);
        self.statements.push(Statement::new_export_from_declaration(
            self.span(),
            specifiers,
            source,
            ImportOrExportKind::Value,
            None,
            &self.builder,
        ));
    }

    pub(crate) fn blank(&mut self) {}

    pub(crate) fn finish(self) -> String {
        self.generate(None).code
    }

    pub(crate) fn finish_with_mappings(self) -> (String, Vec<OriginMapping>) {
        let program_text = ProgramText::clone(&self.program_text);
        let output = self.generate(Some(PathBuf::new()));
        let map = output.map.expect("invariant violated: generated code has no source map");
        let mut mappings = Vec::<OriginMapping>::new();
        for token in map.get_tokens() {
            let generated = Position { line: token.get_dst_line(), column: token.get_dst_col() };
            let ProgramNode { origin, mut name } = program_text.line_node(token.get_src_line());
            if let Some(previous) = mappings.pop_if(|previous| previous.generated == generated) {
                name = name.or(previous.name);
            }
            let continues_previous = mappings.last().is_some_and(|previous| {
                previous.generated.line == generated.line
                    && previous.origin == origin
                    && previous.name == name
            });
            if !continues_previous {
                mappings.push(OriginMapping { generated, origin, name });
            }
        }
        (output.code, mappings)
    }

    fn generate(self, source_map_path: Option<PathBuf>) -> CodegenReturn<'a> {
        let body = ArenaVec::from_iter_in(self.statements, &self.allocator);
        let source_text = self.allocator.alloc_str(&self.program_text.source_text());
        let span = Span::new(0, source_text.len() as u32);
        let comments = ArenaVec::from_iter_in(self.program_text.comments(), &self.allocator);
        let program = Program::new(
            span,
            SourceType::mjs(),
            source_text,
            comments,
            None,
            [],
            body,
            &self.builder,
        );
        let options = CodegenOptions {
            indent_char: IndentChar::Space,
            indent_width: 2,
            source_map_path,
            ..CodegenOptions::default()
        };
        Codegen::new().with_options(options).build(&program)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BindingCallTarget<'a> {
    Constant(&'a str),
    Assignment(&'a str),
}
