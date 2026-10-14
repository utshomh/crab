use std::{cell::RefCell, rc::Rc};

use crate::{
    common::{
        diagnostic::DiagnosticBag,
        types::{Object, Type},
    },
    syntax::syntax_tree::{
        BinaryExpression, BlockStatement, Expression, ForStatement, IfStatement, LiteralExpression,
        NameExpression, ParenthesizedExpression, Statement, UnaryExpression, VarStatement,
        WhileStatement,
    },
};

use super::{
    bindings::Bindings,
    bound_tree::{
        BoundBinaryExpression, BoundBinaryOperator, BoundBlockStatement, BoundExpression,
        BoundExpressionStatement, BoundForStatement, BoundIfStatement, BoundLiteralExpression,
        BoundStatement, BoundUnaryExpression, BoundUnaryOperator, BoundVarStatement,
        BoundVariableExpression, BoundWhileStatement,
    },
};

pub(crate) struct Binder {
    pub(super) bindings: Rc<RefCell<Bindings>>,
    pub(super) result_type: Option<Type>,
    pub(super) function_symbols: std::collections::HashMap<usize, String>,
    pub(super) source: Rc<str>,
    pub(crate) diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
}

impl Binder {
    pub(crate) fn new(
        bindings: Rc<RefCell<Bindings>>,
        diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
        source: Rc<str>,
    ) -> Self {
        Self {
            bindings,
            diagnostic_bag,
            result_type: None,
            function_symbols: std::collections::HashMap::new(),
            source,
        }
    }

    pub(crate) fn bind(&mut self, program: Vec<Statement>) -> Vec<BoundStatement> {
        self.predeclare_functions(&program);
        let mut bound_statements = vec![];

        for statement in program {
            bound_statements.push(self.bind_statement(statement));
        }

        bound_statements
    }

    fn bind_statement(&mut self, statement: Statement) -> BoundStatement {
        match statement {
            Statement::Expression(statement) => BoundStatement::Expression(
                BoundExpressionStatement::new(self.bind_expression(statement.expression)),
            ),
            Statement::Function(name, function) => self.bind_function_declaration(name, function),
            Statement::Return(value, position) => {
                let expected = self.result_type.clone();

                if expected.is_none() {
                    self.diagnostic_bag
                        .borrow_mut()
                        .report(position.clone(), "return is only valid inside a function");
                }

                let value = value.map(|value| self.bind_with_type(value, expected.clone()));

                if value.is_none() && expected.is_some_and(|typ| typ != Type::Unit) {
                    self.diagnostic_bag
                        .borrow_mut()
                        .report(position.clone(), "A value is required for this return");
                }

                BoundStatement::Return(value, position)
            }

            Statement::Var(statement) => self.bind_var_statement(statement),
            Statement::Block(statement) => self.bind_block_statement(statement),
            Statement::If(statement) => self.bind_if_statement(statement),
            Statement::While(statement) => self.bind_while_statement(statement),
            Statement::For(statement) => self.bind_for_statement(statement),
            Statement::Each(statement) => self.bind_each_statement(statement),
        }
    }

    fn bind_var_statement(&mut self, statement: VarStatement) -> BoundStatement {
        if let Some(annotation) = &statement.annotation {
            self.validate_type(annotation, &statement.identifier.position);
        }

        let bound_expression = self.bind_with_type(statement.expression, statement.annotation);
        let symbol = self
            .bindings
            .borrow_mut()
            .set(statement.identifier.lexeme, bound_expression.get_type());
        BoundStatement::Var(BoundVarStatement::new(symbol, bound_expression))
    }

    fn bind_block_statement(&mut self, statement: BlockStatement) -> BoundStatement {
        self.bindings = Rc::new(RefCell::new(Bindings::extend(Rc::clone(&self.bindings))));
        let statements = self.bind(statement.statements);
        let old_bindings = self.bindings.borrow().outer.clone().unwrap();
        self.bindings = old_bindings;
        BoundStatement::Block(BoundBlockStatement::new(statements))
    }

    // Every control-flow body has its own lexical scope, including unbraced bodies.

    pub(super) fn bind_scoped_statement(&mut self, statement: Statement) -> BoundStatement {
        match statement {
            Statement::Block(block) => self.bind_block_statement(block),
            statement => self.bind_block_statement(BlockStatement {
                statements: vec![statement],
            }),
        }
    }

    fn bind_if_statement(&mut self, statement: IfStatement) -> BoundStatement {
        let condition = self.bind_expression(statement.condition.clone());

        if condition.get_type() == Type::Boolean {
            let consequence = self.bind_scoped_statement(*statement.consequence);
            let else_clause =
                (*statement.else_clause).map(|statement| self.bind_scoped_statement(statement));
            BoundStatement::If(BoundIfStatement::new(condition, consequence, else_clause))
        } else {
            self.diagnostic_bag.borrow_mut().invalid_expression_type(
                condition.get_position(),
                Type::Boolean,
                condition.get_type(),
            );
            BoundStatement::Expression(BoundExpressionStatement::new(BoundExpression::Literal(
                BoundLiteralExpression::new(Object::Unit, statement.condition.get_position()),
            )))
        }
    }

    fn bind_while_statement(&mut self, statement: WhileStatement) -> BoundStatement {
        let condition = self.bind_expression(statement.condition.clone());

        if condition.get_type() == Type::Boolean {
            let body = self.bind_scoped_statement(*statement.body);
            BoundStatement::While(BoundWhileStatement::new(condition, body))
        } else {
            self.diagnostic_bag.borrow_mut().invalid_expression_type(
                condition.get_position(),
                Type::Boolean,
                condition.get_type(),
            );
            BoundStatement::Expression(BoundExpressionStatement::new(BoundExpression::Literal(
                BoundLiteralExpression::new(Object::Unit, statement.condition.get_position()),
            )))
        }
    }

    fn bind_for_statement(&mut self, statement: ForStatement) -> BoundStatement {
        let lower_bound = self.bind_expression(statement.lower_bound);
        let upper_bound = self.bind_expression(statement.upper_bound);

        match (lower_bound.get_type(), upper_bound.get_type()) {
            (Type::Number, Type::Number) => {
                let outer = Rc::clone(&self.bindings);
                self.bindings = Rc::new(RefCell::new(Bindings::extend(Rc::clone(&outer))));
                let symbol = self
                    .bindings
                    .borrow_mut()
                    .set(statement.identifier.lexeme.clone(), Type::Number);
                let body = self.bind_scoped_statement(*statement.body);
                self.bindings = outer;
                BoundStatement::For(BoundForStatement::new(
                    symbol,
                    lower_bound,
                    upper_bound,
                    body,
                ))
            }

            (_, _) => {
                for bound in [&lower_bound, &upper_bound] {
                    if bound.get_type() != Type::Number {
                        self.diagnostic_bag.borrow_mut().invalid_expression_type(
                            bound.get_position(),
                            Type::Number,
                            bound.get_type(),
                        );
                    }
                }

                BoundStatement::Expression(BoundExpressionStatement::new(BoundExpression::Literal(
                    BoundLiteralExpression::new(Object::Unit, statement.identifier.position),
                )))
            }
        }
    }

    pub(super) fn bind_expression(&mut self, expression: Expression) -> BoundExpression {
        self.bind_with_type(expression, None)
    }

    pub(super) fn bind_with_type(
        &mut self,
        expression: Expression,
        expected: Option<Type>,
    ) -> BoundExpression {
        let mut bound = match expression {
            Expression::Literal(expression) => self.bind_literal_expression(expression),
            Expression::Name(expression) => self.bind_name_expression(expression),
            Expression::Parenthesized(expression) => self.bind_parenthesized_expression(expression),
            Expression::Unary(expression) => self.bind_unary_expression(expression),
            Expression::Binary(expression) => self.bind_binary_expression(expression),
            Expression::Function(function) => {
                BoundExpression::Function(self.bind_function("<anonymous>".into(), function))
            }

            Expression::Collection(expression) => {
                self.bind_collection(expression, expected.clone())
            }
        };

        if let Some(expected) = expected {
            bound = self.specialize(bound, &expected);

            if bound.get_type() != expected {
                self.diagnostic_bag.borrow_mut().invalid_expression_type(
                    bound.get_position(),
                    expected,
                    bound.get_type(),
                );
            }
        }

        bound
    }

    fn bind_literal_expression(&self, expression: LiteralExpression) -> BoundExpression {
        BoundExpression::Literal(BoundLiteralExpression::new(
            expression.value.clone(),
            expression.get_position(),
        ))
    }

    fn bind_name_expression(&mut self, expression: NameExpression) -> BoundExpression {
        if let Some((typ, symbol)) = self
            .bindings
            .borrow()
            .resolve(&expression.identifier.lexeme)
        {
            BoundExpression::Variable(BoundVariableExpression::new(
                symbol,
                typ,
                expression.get_position(),
            ))
        } else {
            self.diagnostic_bag.borrow_mut().undefined_name(
                expression.get_position(),
                expression.identifier.lexeme.clone(),
            );
            BoundExpression::Literal(BoundLiteralExpression::new(
                Object::Unit,
                expression.get_position(),
            ))
        }
    }

    fn bind_parenthesized_expression(
        &mut self,
        expression: ParenthesizedExpression,
    ) -> BoundExpression {
        self.bind_expression(*expression.expression)
    }

    fn bind_unary_expression(&mut self, expression: UnaryExpression) -> BoundExpression {
        let right = self.bind_expression(*expression.right.clone());

        if let Some(operator) =
            BoundUnaryOperator::bind(expression.operator.kind.clone(), right.get_type())
        {
            BoundExpression::Unary(BoundUnaryExpression::new(
                operator,
                right,
                expression.get_position(),
            ))
        } else {
            self.diagnostic_bag.borrow_mut().invalid_unary_operator(
                expression.get_position(),
                expression.operator.kind.clone(),
                right.get_type(),
            );
            BoundExpression::Literal(BoundLiteralExpression::new(
                Object::Unit,
                expression.get_position(),
            ))
        }
    }

    fn bind_binary_expression(&mut self, expression: BinaryExpression) -> BoundExpression {
        let left = self.bind_expression(*expression.left.clone());
        let right = self.bind_expression(*expression.right.clone());

        if let Some(operator) = BoundBinaryOperator::bind(
            expression.operator.kind.clone(),
            left.get_type(),
            right.get_type(),
        ) {
            BoundExpression::Binary(BoundBinaryExpression::new(
                left,
                operator,
                right,
                expression.get_position(),
            ))
        } else {
            self.diagnostic_bag.borrow_mut().invalid_binary_operator(
                expression.get_position(),
                expression.operator.kind.clone(),
                left.get_type(),
                right.get_type(),
            );
            BoundExpression::Literal(BoundLiteralExpression::new(
                Object::Unit,
                expression.get_position(),
            ))
        }
    }
}
