use std::{cell::RefCell, rc::Rc};

use super::environment::Environment;
use crab::{
    binding::bound_tree::{
        BoundBinaryOperationKind as Binary, BoundExpression, BoundStatement,
        BoundUnaryOperationKind as Unary,
    },
    common::{diagnostic::Diagnostic, types::Object},
};

type EvaluationResult = Result<Object, Diagnostic>;

pub(crate) struct Evaluator {
    bound_statements: Rc<Vec<BoundStatement>>,
    pub(super) bindings: Rc<RefCell<Environment>>,
}

impl Evaluator {
    pub(crate) fn new(
        bound_statements: Vec<BoundStatement>,
        bindings: Rc<RefCell<Environment>>,
    ) -> Self {
        Self {
            bound_statements: Rc::new(bound_statements),
            bindings,
        }
    }

    pub(crate) fn evaluate(&mut self) -> EvaluationResult {
        let statements = Rc::clone(&self.bound_statements);
        let mut object = Object::Unit;
        for statement in statements.iter() {
            object = self.evaluate_statement(statement)?;
        }
        Ok(object)
    }

    fn with_scope(
        &mut self,
        evaluate: impl FnOnce(&mut Self) -> EvaluationResult,
    ) -> EvaluationResult {
        let outer = Rc::clone(&self.bindings);
        self.bindings = Rc::new(RefCell::new(Environment::extend(Rc::clone(&outer))));
        let result = evaluate(self);
        self.bindings = outer;
        result
    }

    fn evaluate_statement(&mut self, statement: &BoundStatement) -> EvaluationResult {
        match statement {
            BoundStatement::Expression(statement) => {
                self.evaluate_expression(&statement.expression)
            }
            BoundStatement::Print(statement) => {
                println!("{}", self.evaluate_expression(&statement.expression)?);
                Ok(Object::Unit)
            }
            BoundStatement::Var(statement) => {
                let object = self.evaluate_expression(&statement.expression)?;
                self.bindings
                    .borrow_mut()
                    .set(statement.name.clone(), object.clone());
                Ok(object)
            }
            BoundStatement::Block(statement) => self.with_scope(|evaluator| {
                for statement in &statement.statements {
                    evaluator.evaluate_statement(statement)?;
                }
                Ok(Object::Unit)
            }),
            BoundStatement::If(statement) => {
                if self.evaluate_boolean(&statement.condition)? {
                    self.evaluate_statement(&statement.consequence)
                } else if let Some(alternative) = statement.else_clause.as_ref() {
                    self.evaluate_statement(alternative)
                } else {
                    Ok(Object::Unit)
                }
            }
            BoundStatement::While(statement) => {
                while self.evaluate_boolean(&statement.condition)? {
                    self.evaluate_statement(&statement.body)?;
                }
                Ok(Object::Unit)
            }
            BoundStatement::For(statement) => {
                // Inclusive, fixed range; both bounds are evaluated once, before the loop scope.
                let lower = self.evaluate_number(&statement.lower_bound)?;
                let upper = self.evaluate_number(&statement.upper_bound)?;
                self.with_scope(|evaluator| {
                    for value in lower..=upper {
                        evaluator
                            .bindings
                            .borrow_mut()
                            .set(statement.identifier.clone(), Object::Number(value));
                        evaluator.evaluate_statement(&statement.body)?;
                    }
                    Ok(Object::Unit)
                })
            }
        }
    }

    fn evaluate_number(&mut self, expression: &BoundExpression) -> Result<i32, Diagnostic> {
        match self.evaluate_expression(expression)? {
            Object::Number(value) => Ok(value),
            _ => Err(Diagnostic::new(
                expression.get_position(),
                "Expected a number".into(),
            )),
        }
    }

    fn evaluate_boolean(&mut self, expression: &BoundExpression) -> Result<bool, Diagnostic> {
        match self.evaluate_expression(expression)? {
            Object::Boolean(value) => Ok(value),
            _ => Err(Diagnostic::new(
                expression.get_position(),
                "Expected a boolean".into(),
            )),
        }
    }

    fn evaluate_expression(&mut self, expression: &BoundExpression) -> EvaluationResult {
        let error = |message: &str| Diagnostic::new(expression.get_position(), message.into());
        match expression {
            BoundExpression::Literal(literal) => Ok(literal.value.clone()),
            BoundExpression::Variable(variable) => self
                .bindings
                .borrow()
                .get(&variable.name)
                .ok_or_else(|| error(&format!("Name '{}' is not initialized", variable.name))),
            BoundExpression::Unary(unary) => {
                let right = self.evaluate_expression(&unary.right)?;
                match (&unary.operator.operation_kind, right) {
                    (Unary::Identity, Object::Number(value)) => Ok(Object::Number(value)),
                    (Unary::Negation, Object::Number(value)) => value
                        .checked_neg()
                        .map(Object::Number)
                        .ok_or_else(|| error("Integer overflow")),
                    (Unary::LogicalNegation, Object::Boolean(value)) => Ok(Object::Boolean(!value)),
                    _ => Err(error("Invalid unary operand")),
                }
            }
            BoundExpression::Binary(binary) => {
                let left = self.evaluate_expression(&binary.left)?;
                match (&binary.operator.operation_kind, &left) {
                    (Binary::LogicalAnd, Object::Boolean(false)) => {
                        return Ok(Object::Boolean(false))
                    }
                    (Binary::LogicalOr, Object::Boolean(true)) => return Ok(Object::Boolean(true)),
                    _ => {}
                }
                let right = self.evaluate_expression(&binary.right)?;
                let number = |value: Option<i32>| {
                    value
                        .map(Object::Number)
                        .ok_or_else(|| error("Integer overflow"))
                };
                match (&binary.operator.operation_kind, left, right) {
                    (Binary::Addition, Object::Number(a), Object::Number(b)) => {
                        number(a.checked_add(b))
                    }
                    (Binary::Subtraction, Object::Number(a), Object::Number(b)) => {
                        number(a.checked_sub(b))
                    }
                    (Binary::Multiplication, Object::Number(a), Object::Number(b)) => {
                        number(a.checked_mul(b))
                    }
                    (Binary::Division, Object::Number(_), Object::Number(0)) => {
                        Err(error("Division by zero"))
                    }
                    (Binary::Division, Object::Number(a), Object::Number(b)) => {
                        number(a.checked_div(b))
                    }
                    (Binary::Addition, Object::String(a), Object::String(b)) => {
                        Ok(Object::String(a + &b))
                    }
                    (Binary::Greater, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Boolean(a > b))
                    }
                    (Binary::Lesser, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Boolean(a < b))
                    }
                    (Binary::GreaterEqual, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Boolean(a >= b))
                    }
                    (Binary::LesserEqual, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Boolean(a <= b))
                    }
                    (Binary::LogicalAnd, Object::Boolean(a), Object::Boolean(b)) => {
                        Ok(Object::Boolean(a && b))
                    }
                    (Binary::LogicalOr, Object::Boolean(a), Object::Boolean(b)) => {
                        Ok(Object::Boolean(a || b))
                    }
                    (Binary::Equal, a, b) => Ok(Object::Boolean(a == b)),
                    (Binary::NotEqual, a, b) => Ok(Object::Boolean(a != b)),
                    _ => Err(error("Invalid binary operands")),
                }
            }
            BoundExpression::Assignment(assignment) => {
                let object = self.evaluate_expression(&assignment.expression)?;
                if self
                    .bindings
                    .borrow_mut()
                    .reset(assignment.name.clone(), object.clone())
                {
                    Ok(object)
                } else {
                    Err(error(&format!(
                        "Name '{}' is not initialized",
                        assignment.name
                    )))
                }
            }
        }
    }
}
