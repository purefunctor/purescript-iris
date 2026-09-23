//! Oxc JavaScript program construction and code generation.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::rc::Rc;

use itertools::Itertools;
use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::ast::{
    Argument, ArrowFunctionBody, AssignmentTarget, BindingIdentifier, BindingPattern,
    BindingProperty, Declaration, ExportSpecifier, Expression, FormalParameter,
    FormalParameterKind, FormalParameters, Function, FunctionBody, FunctionType,
    ImportDeclarationSpecifier, ImportOrExportKind, LabelIdentifier, ModuleExportName, Program,
    PropertyKey, Statement, StringLiteral, SwitchCase, VariableDeclaration,
    VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::builder::AstBuilder;
use oxc_ast::{Comment, CommentKind, CommentPosition};
use oxc_codegen::{Codegen, CodegenOptions, IndentChar};
use oxc_span::{SPAN, SourceType, Span};
use oxc_syntax::operator::AssignmentOperator;
use smol_str::SmolStr;

use crate::convert::identifier_is_binding;
use crate::tree::{ExpressionId, Tree};

pub(crate) struct Writer<'a> {
    allocator: &'a Allocator,
    builder: AstBuilder<'a>,
    statements: Vec<Statement<'a>>,
    comments: Rc<RefCell<Comments>>,
    has_eager_throw: bool,
}

#[derive(Default)]
struct Comments {
    source_text: String,
    comments: Vec<Comment>,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(allocator: &'a Allocator) -> Writer<'a> {
        Writer {
            allocator,
            builder: AstBuilder::new(allocator),
            statements: Vec::new(),
            comments: Rc::new(RefCell::new(Comments::default())),
            has_eager_throw: false,
        }
    }

    fn child(&self) -> Writer<'a> {
        Writer {
            allocator: self.allocator,
            builder: AstBuilder::new(self.allocator),
            statements: Vec::new(),
            comments: Rc::clone(&self.comments),
            has_eager_throw: false,
        }
    }

    fn line_comment(&self, text: &str) -> Span {
        let mut comments = self.comments.borrow_mut();
        let start = comments.source_text.len() as u32;
        comments.source_text.push_str("// ");
        comments.source_text.push_str(text);
        let end = comments.source_text.len() as u32;
        comments.source_text.push('\n');
        let attached_to = comments.source_text.len() as u32;
        comments.source_text.push(' ');

        let mut comment = Comment::new(start, end, CommentKind::Line);
        comment.attached_to = attached_to;
        comment.position = CommentPosition::Leading;
        comment.newlines =
            oxc_ast::ast::CommentNewlines::Leading | oxc_ast::ast::CommentNewlines::Trailing;
        comments.comments.push(comment);
        Span::empty(attached_to)
    }

    fn text(&self, value: &str) -> &'a str {
        self.allocator.alloc_str(value)
    }

    fn binding(&self, name: &str) -> BindingIdentifier<'a> {
        BindingIdentifier::new(SPAN, self.text(name), &self.builder)
    }

    fn binding_pattern(&self, name: &str) -> BindingPattern<'a> {
        BindingPattern::new_binding_identifier(SPAN, self.text(name), &self.builder)
    }

    fn parameters(&self, parameters: &[SmolStr]) -> oxc_allocator::Box<'a, FormalParameters<'a>> {
        let parameters = parameters.iter().map(|parameter| {
            FormalParameter::new(
                SPAN,
                [],
                self.binding_pattern(parameter),
                None,
                None,
                false,
                None,
                false,
                false,
                &self.builder,
            )
        });
        let parameters = parameters.collect_vec();
        let parameters = ArenaVec::from_iter_in(parameters, &self.allocator);
        FormalParameters::boxed(
            SPAN,
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
        FunctionBody::boxed(SPAN, [], statements, &self.builder)
    }

    fn block_statement(&self, statements: Vec<Statement<'a>>) -> Statement<'a> {
        let statements = ArenaVec::from_iter_in(statements, &self.allocator);
        Statement::new_block_statement(SPAN, statements, &self.builder)
    }

    fn arrow_expression(
        &self,
        parameters: &[SmolStr],
        statements: Vec<Statement<'a>>,
    ) -> Expression<'a> {
        let parameters = self.parameters(parameters);
        let body = ArrowFunctionBody::new_function_body(
            SPAN,
            [],
            ArenaVec::from_iter_in(statements, &self.allocator),
            &self.builder,
        );
        Expression::new_arrow_function_expression(
            SPAN,
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
        name: &str,
        value: Option<Expression<'a>>,
        exported: bool,
    ) -> Statement<'a> {
        let declarator = VariableDeclarator::new(
            SPAN,
            self.binding_pattern(name),
            None,
            value,
            false,
            &self.builder,
        );
        let declarations = ArenaVec::from_array_in([declarator], &self.allocator);
        let declaration =
            VariableDeclaration::boxed(SPAN, kind, declarations, false, &self.builder);
        if exported {
            Statement::new_export_declaration(
                SPAN,
                Declaration::VariableDeclaration(declaration),
                &self.builder,
            )
        } else {
            Statement::VariableDeclaration(declaration)
        }
    }

    pub(crate) fn import_namespace(&mut self, namespace: &str, path: &str) {
        let specifier = ImportDeclarationSpecifier::new_import_namespace_specifier(
            SPAN,
            self.binding(namespace),
            &self.builder,
        );
        let specifiers = ArenaVec::from_array_in([specifier], &self.allocator);
        let source = StringLiteral::new(SPAN, self.text(path), None, &self.builder);
        let statement = Statement::new_import_declaration(
            SPAN,
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
                SPAN,
                imported,
                self.binding(local),
                ImportOrExportKind::Value,
                &self.builder,
            )
        });
        let specifiers = ArenaVec::from_iter_in(specifiers, &self.allocator);
        let source = StringLiteral::new(SPAN, self.text(path), None, &self.builder);
        let statement = Statement::new_import_declaration(
            SPAN,
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
        tree: &Tree<'_>,
        name: &str,
        value: impl Borrow<ExpressionId>,
        exported: bool,
    ) {
        let statement = self.variable_statement(
            VariableDeclarationKind::Const,
            name,
            Some(tree.expression_in(value.borrow(), self.allocator)),
            exported,
        );
        self.statements.push(statement);
    }

    pub(crate) fn constant_object_pattern(
        &mut self,
        tree: &Tree<'_>,
        names: &[Option<SmolStr>],
        value: impl Borrow<ExpressionId>,
    ) {
        let properties = names.iter().enumerate().filter_map(|(index, name)| {
            let name = name.as_deref()?;
            let field = format!("_{}", index + 1);
            let key = PropertyKey::new_static_identifier(SPAN, self.text(&field), &self.builder);
            let value = self.binding_pattern(name);
            Some(BindingProperty::new(SPAN, key, value, false, false, &self.builder))
        });
        let properties = ArenaVec::from_iter_in(properties, &self.allocator);
        let pattern = BindingPattern::new_object_pattern(SPAN, properties, None, &self.builder);
        let declarator = VariableDeclarator::new(
            SPAN,
            pattern,
            None,
            Some(tree.expression_in(value.borrow(), self.allocator)),
            false,
            &self.builder,
        );
        let declarations = ArenaVec::from_array_in([declarator], &self.allocator);
        let declaration = VariableDeclaration::boxed(
            SPAN,
            VariableDeclarationKind::Const,
            declarations,
            false,
            &self.builder,
        );
        self.statements.push(Statement::VariableDeclaration(declaration));
    }

    pub(crate) fn mutable(&mut self, name: &str) {
        let statement = self.variable_statement(VariableDeclarationKind::Let, name, None, false);
        self.statements.push(statement);
    }

    pub(crate) fn mutable_value(&mut self, tree: &Tree<'_>, name: &str, value: ExpressionId) {
        let value = tree.expression_in(&value, self.allocator);
        let statement =
            self.variable_statement(VariableDeclarationKind::Let, name, Some(value), false);
        self.statements.push(statement);
    }

    pub(crate) fn expression(
        &self,
        tree: &Tree<'_>,
        expression: impl Borrow<ExpressionId>,
    ) -> Expression<'a> {
        tree.expression_in(expression.borrow(), self.allocator)
    }

    pub(crate) fn assign(&mut self, tree: &Tree<'_>, name: &str, value: ExpressionId) {
        let target = AssignmentTarget::new_assignment_target_identifier(
            SPAN,
            self.text(name),
            &self.builder,
        );
        let expression = Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            target,
            tree.expression_in(&value, self.allocator),
            &self.builder,
        );
        self.statements.push(Statement::new_expression_statement(SPAN, expression, &self.builder));
    }

    pub(crate) fn return_expression(&mut self, tree: &Tree<'_>, value: ExpressionId) {
        self.statements.push(Statement::new_return_statement(
            SPAN,
            Some(tree.expression_in(&value, self.allocator)),
            &self.builder,
        ));
    }

    pub(crate) fn break_label(&mut self, label: &str) {
        let label = LabelIdentifier::new(SPAN, self.text(label), &self.builder);
        self.statements.push(Statement::new_break_statement(SPAN, Some(label), &self.builder));
    }

    pub(crate) fn continue_loop(&mut self) {
        self.statements.push(Statement::new_continue_statement(SPAN, None, &self.builder));
    }

    pub(crate) fn function<R>(
        &mut self,
        name: &str,
        parameters: Vec<SmolStr>,
        exported: bool,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        let function = Function::boxed(
            SPAN,
            FunctionType::FunctionDeclaration,
            Some(self.binding(name)),
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
                SPAN,
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
        parameters: Vec<SmolStr>,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        let expression = self.arrow_expression(&parameters, body.statements);
        let statement =
            self.variable_statement(VariableDeclarationKind::Const, name, Some(expression), false);
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
            SPAN,
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
        let mut body = self.child();
        let result = render(&mut body);
        let expression = self.arrow_expression(&parameters, body.statements);
        let target = AssignmentTarget::new_assignment_target_identifier(
            SPAN,
            self.text(name),
            &self.builder,
        );
        let expression = Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            target,
            expression,
            &self.builder,
        );
        self.statements.push(Statement::new_expression_statement(SPAN, expression, &self.builder));
        result
    }

    pub(crate) fn constant_iife<R>(
        &mut self,
        name: &str,
        exported: bool,
        render: impl FnOnce(&mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(&mut body);
        let has_eager_throw = body.has_eager_throw;
        let arrow = self.arrow_expression(&[], body.statements);
        let call = Expression::new_call_expression_with_pure(
            SPAN,
            arrow,
            None,
            [],
            false,
            !has_eager_throw,
            &self.builder,
        );
        let statement =
            self.variable_statement(VariableDeclarationKind::Const, name, Some(call), exported);
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
        let mut body = self.child();
        let result = render(&mut body);
        let body = self.arrow_expression(&[], body.statements);
        let arguments = [Argument::from(name), Argument::from(body)];
        let arguments = ArenaVec::from_array_in(arguments, &self.allocator);
        let call =
            Expression::new_call_expression(SPAN, callee, None, arguments, false, &self.builder);
        let statement = match target {
            BindingCallTarget::Constant(name) => {
                self.variable_statement(VariableDeclarationKind::Const, name, Some(call), false)
            }
            BindingCallTarget::Assignment(name) => {
                let target = AssignmentTarget::new_assignment_target_identifier(
                    SPAN,
                    self.text(name),
                    &self.builder,
                );
                let assignment = Expression::new_assignment_expression(
                    SPAN,
                    AssignmentOperator::Assign,
                    target,
                    call,
                    &self.builder,
                );
                Statement::new_expression_statement(SPAN, assignment, &self.builder)
            }
        };
        self.statements.push(statement);
        result
    }

    pub(crate) fn if_else<E>(
        &mut self,
        tree: &mut Tree<'_>,
        condition: ExpressionId,
        render_then: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>) -> Result<(), E>,
        render_else: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut then_writer = self.child();
        render_then(tree, &mut then_writer)?;
        let mut else_writer = self.child();
        render_else(tree, &mut else_writer)?;
        self.has_eager_throw |= then_writer.has_eager_throw || else_writer.has_eager_throw;
        let consequent = self.block_statement(then_writer.statements);
        let alternate = self.block_statement(else_writer.statements);
        self.statements.push(Statement::new_if_statement(
            SPAN,
            tree.expression_in(&condition, self.allocator),
            consequent,
            Some(alternate),
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn if_else_with_state<E, S>(
        &mut self,
        tree: &mut Tree<'_>,
        condition: ExpressionId,
        state: &mut S,
        render_then: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>, &mut S) -> Result<(), E>,
        render_else: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>, &mut S) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut then_writer = self.child();
        render_then(tree, &mut then_writer, state)?;
        let mut else_writer = self.child();
        render_else(tree, &mut else_writer, state)?;
        self.has_eager_throw |= then_writer.has_eager_throw || else_writer.has_eager_throw;
        let consequent = self.block_statement(then_writer.statements);
        let alternate = self.block_statement(else_writer.statements);
        self.statements.push(Statement::new_if_statement(
            SPAN,
            tree.expression_in(&condition, self.allocator),
            consequent,
            Some(alternate),
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn if_block<R>(
        &mut self,
        tree: &mut Tree<'_>,
        condition: ExpressionId,
        render: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(tree, &mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let consequent = self.block_statement(body.statements);
        self.statements.push(Statement::new_if_statement(
            SPAN,
            tree.expression_in(&condition, self.allocator),
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
        let label = LabelIdentifier::new(SPAN, self.text(label), &self.builder);
        self.statements.push(Statement::new_labeled_statement(SPAN, label, body, &self.builder));
        result
    }

    pub(crate) fn while_loop<R>(
        &mut self,
        tree: &mut Tree<'_>,
        condition: ExpressionId,
        render: impl FnOnce(&mut Tree<'_>, &mut Writer<'a>) -> R,
    ) -> R {
        let mut body = self.child();
        let result = render(tree, &mut body);
        self.has_eager_throw |= body.has_eager_throw;
        let body = self.block_statement(body.statements);
        let condition = tree.expression_in(&condition, self.allocator);
        self.statements.push(Statement::new_while_statement(SPAN, condition, body, &self.builder));
        result
    }

    pub(crate) fn switch<E>(
        &mut self,
        tree: &mut Tree<'_>,
        discriminant: ExpressionId,
        cases: &[(ExpressionId, SmolStr)],
        mut render: impl FnMut(usize, &mut Tree<'_>, &mut Writer<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut switch_cases = Vec::with_capacity(cases.len());
        let mut has_eager_throw = false;
        for (position, (test, comment)) in cases.iter().enumerate() {
            let span = self.line_comment(comment);
            let mut body = self.child();
            render(position, tree, &mut body)?;
            has_eager_throw |= body.has_eager_throw;
            let test = tree.expression_in(test, self.allocator);
            let body = self.block_statement(body.statements);
            let consequent = ArenaVec::from_array_in([body], &self.allocator);
            let switch_case = SwitchCase::new(span, Some(test), consequent, &self.builder);
            switch_cases.push(switch_case);
        }
        self.has_eager_throw |= has_eager_throw;
        let discriminant = tree.expression_in(&discriminant, self.allocator);
        let switch_cases = ArenaVec::from_iter_in(switch_cases, &self.allocator);
        self.statements.push(Statement::new_switch_statement(
            SPAN,
            discriminant,
            switch_cases,
            &self.builder,
        ));
        Ok(())
    }

    pub(crate) fn throw_error(&mut self, message: &str) {
        self.has_eager_throw = true;
        let callee = Expression::new_identifier(SPAN, self.text("Error"), &self.builder);
        let message = Expression::new_string_literal(SPAN, self.text(message), None, &self.builder);
        let arguments = ArenaVec::from_array_in([Argument::from(message)], &self.allocator);
        let error = Expression::new_new_expression(SPAN, callee, None, arguments, &self.builder);
        self.statements.push(Statement::new_throw_statement(SPAN, error, &self.builder));
    }

    fn module_export_name(&self, name: &str, reference: bool) -> ModuleExportName<'a> {
        if identifier_is_binding(name) {
            if reference {
                ModuleExportName::new_identifier_reference(SPAN, self.text(name), &self.builder)
            } else {
                ModuleExportName::new_identifier_name(SPAN, self.text(name), &self.builder)
            }
        } else {
            ModuleExportName::new_string_literal(SPAN, self.text(name), None, &self.builder)
        }
    }

    pub(crate) fn export(&mut self, local: &str, exported: &str) {
        let specifier = ExportSpecifier::new(
            SPAN,
            self.module_export_name(local, true),
            self.module_export_name(exported, false),
            ImportOrExportKind::Value,
            &self.builder,
        );
        self.statements.push(Statement::new_export_named_declaration(
            SPAN,
            [specifier],
            ImportOrExportKind::Value,
            &self.builder,
        ));
    }

    pub(crate) fn re_export(&mut self, specifiers: Vec<String>, path: &str) {
        let specifiers = specifiers.into_iter().map(|name| {
            ExportSpecifier::new(
                SPAN,
                self.module_export_name(&name, false),
                self.module_export_name(&name, false),
                ImportOrExportKind::Value,
                &self.builder,
            )
        });
        let specifiers = specifiers.collect_vec();
        let specifiers = ArenaVec::from_iter_in(specifiers, &self.allocator);
        let source = StringLiteral::new(SPAN, self.text(path), None, &self.builder);
        self.statements.push(Statement::new_export_from_declaration(
            SPAN,
            specifiers,
            source,
            ImportOrExportKind::Value,
            None,
            &self.builder,
        ));
    }

    pub(crate) fn blank(&mut self) {}

    pub(crate) fn finish(self) -> String {
        let body = ArenaVec::from_iter_in(self.statements, &self.allocator);
        let comments = self.comments.as_ref().borrow();
        let source_text = self.allocator.alloc_str(&comments.source_text);
        let span = Span::new(0, source_text.len() as u32);
        let program_comments =
            ArenaVec::from_iter_in(comments.comments.iter().copied(), &self.allocator);
        let program = Program::new(
            span,
            SourceType::mjs(),
            source_text,
            program_comments,
            None,
            [],
            body,
            &self.builder,
        );
        let options = CodegenOptions {
            indent_char: IndentChar::Space,
            indent_width: 2,
            ..CodegenOptions::default()
        };
        Codegen::new().with_options(options).build(&program).code
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BindingCallTarget<'a> {
    Constant(&'a str),
    Assignment(&'a str),
}
