use super::{
    binder::Binder,
    bindings::Bindings,
    bound_tree::{
        BoundCollectionExpression, BoundCollectionKind as Kind, BoundEachStatement,
        BoundExpression, BoundStatement,
    },
};
use crate::{
    common::{
        diagnostic::Position,
        types::{Object, Type},
    },
    syntax::syntax_tree::{
        CollectionExpression, CollectionKind as Syntax, EachStatement, Expression,
    },
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

impl Binder {
    pub(super) fn validate_type(&mut self, typ: &Type, position: &Position) {
        match typ {
            Type::Map(key, value) => {
                if !matches!(**key, Type::Number | Type::String) {
                    self.report(position, "Dictionary keys must be Number or String");
                }

                self.validate_type(value, position);
            }

            Type::List(element) => self.validate_type(element, position),
            Type::Record(fields) => {
                for typ in fields.values() {
                    self.validate_type(typ, position);
                }
            }

            Type::Function(parameters, result) => {
                for typ in parameters {
                    self.validate_type(typ, position);
                }

                self.validate_type(result, position);
            }

            _ => {}
        }
    }

    fn report(&mut self, position: &Position, message: impl Into<String>) {
        self.diagnostic_bag
            .borrow_mut()
            .report(position.clone(), message);
    }

    fn number(&mut self, expression: Expression) -> BoundExpression {
        self.bind_with_type(expression, Some(Type::Number))
    }

    pub(super) fn literal_key(expression: &BoundExpression) -> Option<&str> {
        if let BoundExpression::Literal(literal) = expression {
            if let Object::String(key) = &literal.value {
                return Some(key);
            }
        }

        None
    }

    fn access_type(
        &mut self,
        typ: &Type,
        index: &BoundExpression,
        position: &Position,
        allow_missing: bool,
    ) -> Type {
        let (key, value) = match typ {
            Type::List(element) => (Type::Number, (**element).clone()),
            Type::Map(key, value) => ((**key).clone(), (**value).clone()),
            Type::Record(fields) => {
                if let Some(name) = Self::literal_key(index) {
                    if let Some(value) = fields.get(name) {
                        return value.clone();
                    }

                    if !allow_missing {
                        self.report(position, format!("Record has no field '{name}'"));
                    }
                } else {
                    self.report(
                        position,
                        "Record access requires a literal field name; use a Map for dynamic keys",
                    );
                }

                return Type::Unit;
            }

            _ => {
                self.report(position, format!("Cannot index {typ}"));

                return Type::Unit;
            }
        };

        if index.get_type() != key {
            self.report(
                position,
                format!("Expected {key} index, got {}", index.get_type()),
            );
        }

        value
    }

    pub(super) fn bind_each_statement(&mut self, statement: EachStatement) -> BoundStatement {
        let iterable = self.bind_expression(statement.iterable);

        let typ = match iterable.get_type() {
            Type::List(element) | Type::Map(_, element) => *element,
            Type::Range => Type::Number,
            typ => {
                self.report(
                    &iterable.get_position(),
                    format!("Cannot iterate {typ}; expected List, Map, or Range"),
                );
                Type::Unit
            }
        };

        let outer = Rc::clone(&self.bindings);
        self.bindings = Rc::new(RefCell::new(Bindings::extend(Rc::clone(&outer))));
        let symbol = self
            .bindings
            .borrow_mut()
            .set(statement.identifier.lexeme.clone(), typ);

        let body = self.bind_scoped_statement(*statement.body);
        self.bindings = outer;

        BoundStatement::Each(BoundEachStatement {
            name: symbol,
            iterable,
            body: Box::new(body),
        })
    }

    pub(super) fn bind_collection(
        &mut self,
        expression: CollectionExpression,
        expected: Option<Type>,
    ) -> BoundExpression {
        let position = expression.position;
        let (kind, typ) = match expression.kind {
            Syntax::List(elements) => {
                let mut element_type = if let Some(Type::List(element)) = expected {
                    Some(*element)
                } else {
                    None
                };

                let mut values = Vec::new();

                for expression in elements {
                    let value = self.bind_with_type(expression, element_type.clone());
                    element_type.get_or_insert_with(|| value.get_type());
                    values.push(value);
                }

                let element = element_type.unwrap_or_else(|| {
                    self.report(
                        &position,
                        "Cannot infer an empty list; use a List<T> annotation or [value; 0]",
                    );
                    Type::Unit
                });

                (Kind::List(values), Type::List(Box::new(element)))
            }

            Syntax::Repeat(value, count) => {
                let element_type = if let Some(Type::List(element)) = expected {
                    Some(*element)
                } else {
                    None
                };
                let value = self.bind_with_type(*value, element_type);
                let typ = Type::List(Box::new(value.get_type()));
                let count = self.number(*count);
                (Kind::Repeat(Box::new(value), Box::new(count)), typ)
            }

            Syntax::Map(entries) => {
                let (mut key_type, mut value_type) = if let Some(Type::Map(key, value)) = expected {
                    (Some(*key), Some(*value))
                } else {
                    (None, None)
                };

                let mut values = Vec::new();

                for (key, value) in entries {
                    let key = self.bind_with_type(key, key_type.clone());
                    let value = self.bind_with_type(value, value_type.clone());
                    key_type.get_or_insert_with(|| key.get_type());
                    value_type.get_or_insert_with(|| value.get_type());
                    values.push((key, value));
                }

                let key = key_type.unwrap_or_else(|| {
                    self.report(
                        &position,
                        "Cannot infer an empty dictionary; use a Map<K, V> annotation",
                    );
                    Type::Unit
                });

                let value = value_type.unwrap_or(Type::Unit);

                if !matches!(key, Type::Number | Type::String | Type::Unit) {
                    self.report(&position, "Dictionary keys must be Number or String");
                }

                (Kind::Map(values), Type::Map(Box::new(key), Box::new(value)))
            }

            Syntax::Record(entries) => {
                let mut fields = BTreeMap::new();
                let mut values = Vec::new();

                for (name, expression) in entries {
                    let field_type = if let Some(Type::Record(expected)) = &expected {
                        expected.get(&name).cloned()
                    } else {
                        None
                    };

                    let value = self.bind_with_type(expression, field_type);

                    if fields.insert(name.clone(), value.get_type()).is_some() {
                        self.report(&position, format!("Duplicate record field '{name}'"));
                    }

                    values.push((name, value));
                }

                (Kind::Record(values), Type::Record(fields))
            }

            Syntax::Range(start, end) => (
                Kind::Range(Box::new(self.number(*start)), Box::new(self.number(*end))),
                Type::Range,
            ),

            Syntax::Index(target, index) => {
                let target = self.bind_expression(*target);
                let index = self.bind_expression(*index);
                let typ = self.access_type(&target.get_type(), &index, &position, false);
                (Kind::Index(Box::new(target), Box::new(index)), typ)
            }

            Syntax::Call(callee, expressions) => {
                let (callee, args, result) = self.bind_call(*callee, expressions, &position);
                (Kind::Call(Box::new(callee), args), result)
            }

            Syntax::Assign(target, value) => {
                let mut path = Vec::new();

                let root = Self::assignment_path(*target, &mut path);

                let name = match root {
                    Some(name) => name,
                    None => {
                        self.report(
                            &position,
                            "Assignment target must be a variable or an indexed variable",
                        );
                        String::new()
                    }
                };

                let binding = self.bindings.borrow().resolve(&name);
                let (mut typ, name) = binding.unwrap_or_else(|| {
                    self.report(&position, format!("Name '{name}' is not defined"));
                    (Type::Unit, name)
                });

                let mut indices = Vec::new();

                for expression in path {
                    let index = self.bind_expression(expression);
                    typ = self.access_type(&typ, &index, &position, false);
                    indices.push(index);
                }

                let value = self.bind_with_type(*value, Some(typ.clone()));
                (
                    Kind::Assign {
                        name,
                        indices,
                        value: Box::new(value),
                    },
                    typ,
                )
            }
        };

        BoundExpression::Collection(BoundCollectionExpression {
            kind,
            typ,
            position,
        })
    }

    fn assignment_path(target: Expression, indices: &mut Vec<Expression>) -> Option<String> {
        match target {
            Expression::Name(name) => Some(name.identifier.lexeme),
            Expression::Parenthesized(expression) => {
                Self::assignment_path(*expression.expression, indices)
            }

            Expression::Collection(CollectionExpression {
                kind: Syntax::Index(target, index),
                ..
            }) => {
                let root = Self::assignment_path(*target, indices)?;
                indices.push(*index);
                Some(root)
            }

            _ => None,
        }
    }

    pub(super) fn builtin_type(
        &mut self,
        name: &str,
        args: &[BoundExpression],
        position: &Position,
    ) -> Type {
        if let Some((arity, result)) = match name {
            "input" => Some((1, Type::String)),
            "readLine" => Some((
                0,
                Type::Record(BTreeMap::from([
                    ("text".into(), Type::String),
                    ("eof".into(), Type::Boolean),
                ])),
            )),
            "readAll" => Some((0, Type::String)),
            "readFile" | "trim" => Some((1, Type::String)),
            "readLines" => Some((1, Type::List(Box::new(Type::String)))),
            "split" => Some((2, Type::List(Box::new(Type::String)))),
            "parseNumber" => Some((1, Type::Number)),
            "parseBoolean" => Some((1, Type::Boolean)),
            "tryParseNumber" => Some((
                1,
                Type::Record(BTreeMap::from([
                    ("ok".into(), Type::Boolean),
                    ("value".into(), Type::Number),
                ])),
            )),
            "tryParseBoolean" => Some((
                1,
                Type::Record(BTreeMap::from([
                    ("ok".into(), Type::Boolean),
                    ("value".into(), Type::Boolean),
                ])),
            )),
            _ => None,
        } {
            if args.len() != arity && !(name == "input" && args.is_empty()) {
                self.report(
                    position,
                    format!(
                        "'{name}' expects {} arguments, got {}",
                        if name == "input" {
                            "zero or one".to_owned()
                        } else {
                            arity.to_string()
                        },
                        args.len()
                    ),
                );
            }

            if args.iter().any(|arg| arg.get_type() != Type::String) {
                self.report(position, format!("'{name}' requires String arguments"));
            }

            return result;
        }

        if name == "print" {
            return Type::Unit;
        }

        if name == "str" {
            if args.len() != 1 {
                self.report(position, "str expects one argument");
            }

            return Type::String;
        }

        let arity = match name {
            "len" | "pop" | "keys" | "values" | "collect" | "abs" => 1,
            "push" | "has" | "remove" | "map" | "filter" | "each" | "sort" | "min" | "max" => 2,
            "get" | "fold" => 3,
            _ => {
                self.report(position, format!("Unknown built-in '{name}'"));

                return Type::Unit;
            }
        };

        if args.len() != arity {
            self.report(
                position,
                format!("'{name}' expects {arity} arguments, got {}", args.len()),
            );

            return Type::Unit;
        }

        let typ = args[0].get_type();
        let invalid = |this: &mut Self| {
            this.report(position, format!("'{name}' does not support {typ}"));
            Type::Unit
        };

        match name {
            "abs" | "min" | "max" => {
                if args.iter().any(|arg| arg.get_type() != Type::Number) {
                    self.report(position, format!("{name} requires Number arguments"));
                }

                Type::Number
            }

            "map" | "filter" | "fold" | "each" | "sort" => {
                let Type::List(element) = &typ else {
                    return invalid(self);
                };
                let callback = &args[if name == "fold" { 2 } else { 1 }];
                let Type::Function(parameters, result) = callback.get_type() else {
                    self.report(position, "Expected a typed function callback");

                    return Type::Unit;
                };
                let expected_parameters = if name == "fold" {
                    vec![args[1].get_type(), (**element).clone()]
                } else if name == "sort" {
                    vec![(**element).clone(), (**element).clone()]
                } else {
                    vec![(**element).clone()]
                };

                if parameters != expected_parameters {
                    self.report(
                        position,
                        "Callback parameter types do not match the collection operation",
                    );
                }

                match name {
                    "map" => Type::List(result),
                    "fold" => {
                        if *result != args[1].get_type() {
                            self.report(position, "fold callback must return the accumulator type");
                        }

                        args[1].get_type()
                    }

                    "filter" => {
                        if *result != Type::Boolean {
                            self.report(position, "filter callback must return Boolean");
                        }

                        typ
                    }

                    "each" => {
                        if *result != Type::Unit {
                            self.report(position, "each callback must return Unit");
                        }

                        Type::Unit
                    }

                    "sort" => {
                        if *result != Type::Number {
                            self.report(position, "sort comparator must return Number");
                        }

                        typ
                    }

                    _ => unreachable!(),
                }
            }

            "len" => {
                if matches!(
                    typ,
                    Type::List(_) | Type::Map(_, _) | Type::Record(_) | Type::Range | Type::String
                ) {
                    Type::Number
                } else {
                    invalid(self)
                }
            }

            "collect" => {
                if typ == Type::Range {
                    Type::List(Box::new(Type::Number))
                } else {
                    invalid(self)
                }
            }

            "push" => {
                if let Type::List(element) = &typ {
                    if args[1].get_type() != **element {
                        self.report(
                            position,
                            format!("push expects {element}, got {}", args[1].get_type()),
                        );
                    }

                    typ
                } else {
                    invalid(self)
                }
            }

            "pop" => {
                if let Type::List(element) = &typ {
                    Type::Record(BTreeMap::from([
                        ("items".into(), typ.clone()),
                        ("value".into(), (**element).clone()),
                    ]))
                } else {
                    invalid(self)
                }
            }

            "keys" => match &typ {
                Type::List(_) => Type::List(Box::new(Type::Number)),
                Type::Map(key, _) => Type::List(key.clone()),
                Type::Record(_) => Type::List(Box::new(Type::String)),
                _ => invalid(self),
            },
            "values" => match &typ {
                Type::List(_) => typ.clone(),
                Type::Map(_, value) => Type::List(value.clone()),
                Type::Record(fields) => {
                    let element = fields.values().next().cloned();

                    if let Some(element) = element {
                        if fields.values().all(|typ| *typ == element) {
                            return Type::List(Box::new(element));
                        }
                    }

                    self.report(
                        position,
                        "values(record) requires nonempty fields of one type",
                    );

                    Type::Unit
                }

                _ => invalid(self),
            },
            "has" => {
                let key = match &typ {
                    Type::List(_) => Type::Number,
                    Type::Map(key, _) => (**key).clone(),
                    Type::Record(_) => Type::String,
                    _ => return invalid(self),
                };

                if args[1].get_type() != key {
                    self.report(position, format!("has expects a {key} key"));
                }

                Type::Boolean
            }

            "get" => {
                let element = self.access_type(&typ, &args[1], position, true);

                let element = if element == Type::Unit {
                    args[2].get_type()
                } else {
                    element
                };

                if args[2].get_type() != element {
                    self.report(position, format!("get default must be {element}"));
                }

                element
            }

            "remove" => {
                if let Type::Record(fields) = &typ {
                    if let Some(name) = Self::literal_key(&args[1]) {
                        let mut fields = fields.clone();
                        fields.remove(name);

                        return Type::Record(fields);
                    }

                    self.report(position, "remove(record) requires a literal field name");

                    return Type::Unit;
                }

                self.access_type(&typ, &args[1], position, false);

                typ
            }

            _ => unreachable!(),
        }
    }
}
