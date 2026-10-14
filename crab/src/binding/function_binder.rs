use super::{
    binder::Binder,
    bindings::Bindings,
    bound_tree::{BoundExpression, BoundFunction, BoundStatement, BoundVariableExpression},
};
use crate::{
    common::{diagnostic::Position, types::Type},
    syntax::syntax_tree::{FunctionExpression, Statement},
};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

impl Binder {
    pub(super) fn predeclare_functions(&mut self, statements: &[Statement]) {
        let mut names = HashSet::new();

        for statement in statements {
            if let Statement::Function(name, function) = statement {
                if !names.insert(name) {
                    self.diagnostic_bag.borrow_mut().report(
                        function.position.clone(),
                        format!("Duplicate function '{name}'"),
                    );
                }

                let typ = Type::Function(
                    function
                        .parameters
                        .iter()
                        .map(|(_, typ)| typ.clone())
                        .collect(),
                    Box::new(function.result.clone()),
                );
                let symbol = self.bindings.borrow_mut().set(name.clone(), typ);
                self.function_symbols
                    .insert(function.position.start, symbol);
            }
        }
    }

    pub(super) fn bind_function_declaration(
        &mut self,
        name: String,
        function: FunctionExpression,
    ) -> BoundStatement {
        let symbol = self
            .function_symbols
            .get(&function.position.start)
            .cloned()
            .expect("predeclared function");
        BoundStatement::Function(symbol, self.bind_function(name, function))
    }

    pub(super) fn bind_function(
        &mut self,
        name: String,
        function: FunctionExpression,
    ) -> Rc<BoundFunction> {
        let signature = Type::Function(
            function
                .parameters
                .iter()
                .map(|(_, typ)| typ.clone())
                .collect(),
            Box::new(function.result.clone()),
        );
        self.validate_type(&signature, &function.position);
        let outer = Rc::clone(&self.bindings);
        self.bindings = Rc::new(RefCell::new(Bindings::extend(Rc::clone(&outer))));
        let previous_result = self.result_type.replace(function.result.clone());
        let mut names = HashSet::new();
        let mut parameters = Vec::new();

        for (token, typ) in function.parameters {
            if !names.insert(token.lexeme.clone()) {
                self.diagnostic_bag
                    .borrow_mut()
                    .report(token.position, "Duplicate parameter name");
            }

            parameters.push(self.bindings.borrow_mut().set(token.lexeme, typ));
        }

        let body = self.bind_scoped_statement(*function.body);

        if function.result != Type::Unit && !Self::always_returns(&body) {
            self.diagnostic_bag.borrow_mut().report(
                function.position.clone(),
                format!(
                    "Function '{name}' must return {} on every path",
                    function.result
                ),
            );
        }

        self.bindings = outer;
        self.result_type = previous_result;
        let mut captures = HashSet::new();
        Self::capture_statement(&body, &mut captures);
        Rc::new(BoundFunction {
            captures: captures.into_iter().collect(),
            name,
            parameters,
            signature,
            body,
            position: function.position,
            source: Rc::clone(&self.source),
        })
    }

    fn capture_statement(statement: &BoundStatement, captures: &mut HashSet<String>) {
        match statement {
            BoundStatement::Expression(statement) => {
                Self::capture_expression(&statement.expression, captures)
            }

            BoundStatement::Var(statement) => {
                Self::capture_expression(&statement.expression, captures)
            }

            BoundStatement::Block(block) => {
                for statement in &block.statements {
                    Self::capture_statement(statement, captures);
                }
            }

            BoundStatement::If(statement) => {
                Self::capture_expression(&statement.condition, captures);
                Self::capture_statement(&statement.consequence, captures);

                if let Some(statement) = statement.else_clause.as_ref() {
                    Self::capture_statement(statement, captures);
                }
            }

            BoundStatement::While(statement) => {
                Self::capture_expression(&statement.condition, captures);
                Self::capture_statement(&statement.body, captures);
            }

            BoundStatement::For(statement) => {
                Self::capture_expression(&statement.lower_bound, captures);
                Self::capture_expression(&statement.upper_bound, captures);
                Self::capture_statement(&statement.body, captures);
            }

            BoundStatement::Each(statement) => {
                Self::capture_expression(&statement.iterable, captures);
                Self::capture_statement(&statement.body, captures);
            }

            BoundStatement::Function(_, function) => {
                captures.extend(function.captures.iter().cloned())
            }

            BoundStatement::Return(value, _) => {
                if let Some(value) = value {
                    Self::capture_expression(value, captures);
                }
            }
        }
    }

    fn capture_expression(expression: &BoundExpression, captures: &mut HashSet<String>) {
        use super::bound_tree::BoundCollectionKind as Kind;

        match expression {
            BoundExpression::Variable(variable) => {
                captures.insert(variable.name.clone());
            }

            BoundExpression::Function(function) => {
                captures.extend(function.captures.iter().cloned())
            }

            BoundExpression::Specialize(expression, _, _) => {
                Self::capture_expression(expression, captures)
            }

            BoundExpression::Unary(expression) => {
                Self::capture_expression(&expression.right, captures)
            }

            BoundExpression::Binary(expression) => {
                Self::capture_expression(&expression.left, captures);
                Self::capture_expression(&expression.right, captures);
            }

            BoundExpression::Literal(_) => {}
            BoundExpression::Collection(expression) => match &expression.kind {
                Kind::List(values) => {
                    for value in values {
                        Self::capture_expression(value, captures);
                    }
                }

                Kind::Repeat(a, b) | Kind::Range(a, b) | Kind::Index(a, b) => {
                    Self::capture_expression(a, captures);
                    Self::capture_expression(b, captures);
                }

                Kind::Record(fields) => {
                    for (_, value) in fields {
                        Self::capture_expression(value, captures);
                    }
                }

                Kind::Map(fields) => {
                    for (key, value) in fields {
                        Self::capture_expression(key, captures);
                        Self::capture_expression(value, captures);
                    }
                }

                Kind::Call(callee, args) => {
                    Self::capture_expression(callee, captures);

                    for value in args {
                        Self::capture_expression(value, captures);
                    }
                }

                Kind::Assign {
                    name,
                    indices,
                    value,
                } => {
                    captures.insert(name.clone());

                    for index in indices {
                        Self::capture_expression(index, captures);
                    }

                    Self::capture_expression(value, captures);
                }
            },
        }
    }

    fn always_returns(statement: &BoundStatement) -> bool {
        match statement {
            BoundStatement::Return(_, _) => true,
            BoundStatement::Block(block) => block.statements.iter().any(Self::always_returns),
            BoundStatement::If(statement) => {
                Self::always_returns(&statement.consequence)
                    && statement
                        .else_clause
                        .as_ref()
                        .as_ref()
                        .is_some_and(Self::always_returns)
            }

            _ => false,
        }
    }

    pub(super) fn specialize(
        &mut self,
        expression: BoundExpression,
        expected: &Type,
    ) -> BoundExpression {
        if let (Type::Builtin(name), Type::Function(parameters, result)) =
            (expression.get_type(), expected)
        {
            let position = expression.get_position();
            let arguments: Vec<_> = parameters
                .iter()
                .map(|typ| {
                    BoundExpression::Variable(BoundVariableExpression::new(
                        String::new(),
                        typ.clone(),
                        position.clone(),
                    ))
                })
                .collect();
            let actual = self.builtin_type(&name, &arguments, &position);

            if actual != **result {
                self.diagnostic_bag.borrow_mut().report(
                    position.clone(),
                    format!("Built-in '{name}' returns {actual}, expected {result}"),
                );
            }

            BoundExpression::Specialize(Box::new(expression), expected.clone(), position)
        } else {
            expression
        }
    }

    pub(super) fn bind_call(
        &mut self,
        callee: crate::syntax::syntax_tree::Expression,
        expressions: Vec<crate::syntax::syntax_tree::Expression>,
        position: &Position,
    ) -> (BoundExpression, Vec<BoundExpression>, Type) {
        let callee = self.bind_expression(callee);
        let typ = callee.get_type();
        let mut args: Vec<BoundExpression> = Vec::new();

        for (index, expression) in expressions.into_iter().enumerate() {
            let expected = match &typ {
                Type::Function(parameters, _) => parameters.get(index).cloned(),
                Type::Builtin(name) => {
                    let first = args.first().map(BoundExpression::get_type);

                    match (name.as_str(), index, first) {
                        ("push", 1, Some(Type::List(element)))
                        | ("get", 2, Some(Type::List(element))) => Some(*element),
                        ("get", 2, Some(Type::Map(_, value))) => Some(*value),
                        ("get", 2, Some(Type::Record(fields))) => args
                            .get(1)
                            .and_then(Self::literal_key)
                            .and_then(|key| fields.get(key).cloned()),
                        ("filter", 1, Some(Type::List(element))) => {
                            Some(Type::Function(vec![*element], Box::new(Type::Boolean)))
                        }

                        ("each", 1, Some(Type::List(element))) => {
                            Some(Type::Function(vec![*element], Box::new(Type::Unit)))
                        }

                        ("sort", 1, Some(Type::List(element))) => Some(Type::Function(
                            vec![(*element).clone(), *element],
                            Box::new(Type::Number),
                        )),
                        ("fold", 2, Some(Type::List(element))) => args.get(1).map(|acc| {
                            Type::Function(vec![acc.get_type(), *element], Box::new(acc.get_type()))
                        }),
                        _ => None,
                    }
                }

                _ => None,
            };
            let mut argument = self.bind_with_type(expression, expected);

            if let (Type::Builtin(name), 1, Type::Builtin(callback)) =
                (&typ, index, argument.get_type())
            {
                if name == "map" {
                    if let Some(Type::List(element)) = args.first().map(BoundExpression::get_type) {
                        let placeholder = BoundExpression::Variable(BoundVariableExpression::new(
                            String::new(),
                            (*element).clone(),
                            position.clone(),
                        ));
                        let result = self.builtin_type(&callback, &[placeholder], position);
                        argument = self.specialize(
                            argument,
                            &Type::Function(vec![*element], Box::new(result)),
                        );
                    }
                }
            }

            args.push(argument);
        }

        let result = match typ {
            Type::Builtin(name) => self.builtin_type(&name, &args, position),
            Type::Function(parameters, result) => {
                if parameters.len() != args.len() {
                    self.diagnostic_bag.borrow_mut().report(
                        position.clone(),
                        format!(
                            "Function expects {} arguments, got {}",
                            parameters.len(),
                            args.len()
                        ),
                    );
                }

                *result
            }

            other => {
                self.diagnostic_bag
                    .borrow_mut()
                    .report(position.clone(), format!("Cannot call {other}"));
                Type::Unit
            }
        };
        (callee, args, result)
    }
}
