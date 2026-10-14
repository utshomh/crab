use std::{cell::RefCell, rc::Rc};

use super::environment::Environment;
use crab::{
    binding::bound_tree::{
        BoundBinaryOperationKind as Binary, BoundExpression, BoundStatement,
        BoundUnaryOperationKind as Unary,
    },
    common::{
        diagnostic::Diagnostic,
        types::{Number, Object},
    },
};

#[derive(Debug)]
pub(super) enum Signal {
    Error(Diagnostic),
    Return(Object),
}

impl From<Diagnostic> for Signal {
    fn from(error: Diagnostic) -> Self {
        Self::Error(error)
    }
}

pub(super) type EvaluationResult = Result<Object, Signal>;

pub(crate) struct Evaluator {
    bound_statements: Rc<Vec<BoundStatement>>,
    pub(super) source: Rc<str>,
    pub(super) depth: usize,
    pub(super) roots: Vec<String>,
    pub(super) bindings: Rc<RefCell<Environment>>,
}

impl Evaluator {
    pub(crate) fn new(
        bound_statements: Vec<BoundStatement>,
        bindings: Rc<RefCell<Environment>>,
        source: Rc<str>,
        roots: Vec<String>,
    ) -> Self {
        Self {
            bound_statements: Rc::new(bound_statements),
            bindings,
            source,
            depth: 0,
            roots,
        }
    }

    pub(crate) fn evaluate(&mut self) -> Result<Object, Diagnostic> {
        let statements = Rc::clone(&self.bound_statements);
        let result = self
            .evaluate_statements(&statements)
            .map_err(|signal| match signal {
                Signal::Error(mut diagnostic) => {
                    if diagnostic.source.is_none() {
                        diagnostic.source = Some(Rc::clone(&self.source));
                    }

                    diagnostic
                }

                Signal::Return(_) => {
                    Diagnostic::new(Default::default(), "return outside a function".into())
                }
            })?;
        self.bindings.borrow_mut().collect(&result, &self.roots);

        Ok(result)
    }

    pub(super) fn evaluate_statements(
        &mut self,
        statements: &[BoundStatement],
    ) -> EvaluationResult {
        for statement in statements {
            if let BoundStatement::Function(symbol, code) = statement {
                let value = self.bindings.borrow_mut().function(Rc::clone(code));
                self.bindings.borrow_mut().set(symbol.clone(), value);
            }
        }

        let mut object = Object::Unit;

        for statement in statements {
            object = self.evaluate_statement(statement)?;
        }

        Ok(object)
    }

    pub(super) fn with_scope(
        &mut self,
        evaluate: impl FnOnce(&mut Self) -> EvaluationResult,
    ) -> EvaluationResult {
        let outer = Rc::clone(&self.bindings);
        self.bindings = Rc::new(RefCell::new(Environment::extend(Rc::clone(&outer))));
        let result = evaluate(self);
        self.bindings.borrow_mut().release_scope();
        self.bindings = outer;
        result
    }

    pub(super) fn evaluate_statement(&mut self, statement: &BoundStatement) -> EvaluationResult {
        match statement {
            BoundStatement::Each(statement) => self.evaluate_each(statement),
            BoundStatement::Expression(statement) => {
                self.evaluate_expression(&statement.expression)
            }

            BoundStatement::Function(_, _) => Ok(Object::Unit),
            BoundStatement::Return(value, _) => {
                let value = if let Some(value) = value {
                    self.evaluate_expression(value)?
                } else {
                    Object::Unit
                };

                Err(Signal::Return(value))
            }

            BoundStatement::Var(statement) => {
                let object = self.evaluate_expression(&statement.expression)?;
                self.bindings
                    .borrow_mut()
                    .set(statement.name.clone(), object.clone());

                Ok(object)
            }

            BoundStatement::Block(statement) => self.with_scope(|evaluator| {
                evaluator.evaluate_statements(&statement.statements)?;

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
                    let mut value = lower;

                    while value <= upper {
                        evaluator.with_scope(|iteration| {
                            iteration
                                .bindings
                                .borrow_mut()
                                .set(statement.identifier.clone(), Object::Number(value.clone()));
                            iteration.evaluate_statement(&statement.body)
                        })?;
                        value += 1u8;
                    }

                    Ok(Object::Unit)
                })
            }
        }
    }

    fn evaluate_number(&mut self, expression: &BoundExpression) -> Result<Number, Signal> {
        match self.evaluate_expression(expression)? {
            Object::Number(value) => Ok(value),
            _ => Err(Diagnostic::new(expression.get_position(), "Expected a number".into()).into()),
        }
    }

    fn evaluate_boolean(&mut self, expression: &BoundExpression) -> Result<bool, Signal> {
        match self.evaluate_expression(expression)? {
            Object::Boolean(value) => Ok(value),
            _ => {
                Err(Diagnostic::new(expression.get_position(), "Expected a boolean".into()).into())
            }
        }
    }

    pub(super) fn evaluate_expression(&mut self, expression: &BoundExpression) -> EvaluationResult {
        let error = |message: &str| {
            Signal::Error(Diagnostic::new(expression.get_position(), message.into()))
        };

        match expression {
            BoundExpression::Function(code) => {
                Ok(self.bindings.borrow_mut().function(Rc::clone(code)))
            }

            BoundExpression::Specialize(expression, signature, _) => {
                let value = self.evaluate_expression(expression)?;

                if let Object::Builtin(name, _) = value {
                    Ok(Object::Builtin(name, Some(signature.clone())))
                } else {
                    Ok(value)
                }
            }

            BoundExpression::Collection(expression) => self.evaluate_collection(expression),
            BoundExpression::Literal(literal) => Ok(literal.value.clone()),
            BoundExpression::Variable(variable) => {
                self.bindings.borrow().get(&variable.name).ok_or_else(|| {
                    error(&format!(
                        "Name '{}' is not initialized",
                        variable.name.split('#').next().unwrap_or(&variable.name)
                    ))
                })
            }

            BoundExpression::Unary(unary) => {
                let right = self.evaluate_expression(&unary.right)?;

                match (&unary.operator.operation_kind, right) {
                    (Unary::Identity, Object::Number(value)) => Ok(Object::Number(value)),
                    (Unary::Negation, Object::Number(value)) => Ok(Object::Number(-value)),
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

                match (&binary.operator.operation_kind, left, right) {
                    (Binary::Addition, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Number(a + b))
                    }

                    (Binary::Subtraction, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Number(a - b))
                    }

                    (Binary::Multiplication, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Number(a * b))
                    }

                    (Binary::Division, Object::Number(_), Object::Number(b))
                        if b == Number::default() =>
                    {
                        Err(error("Division by zero"))
                    }

                    (Binary::Division, Object::Number(a), Object::Number(b)) => {
                        Ok(Object::Number(a / b))
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
        }
    }
}
