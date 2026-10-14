use super::evaluator::{EvaluationResult, Evaluator, Signal};
use crab::{
    binding::bound_tree::{
        BoundCollectionExpression, BoundCollectionKind as Kind, BoundEachStatement,
    },
    common::{
        diagnostic::{Diagnostic, Position},
        types::{Key, MapData, Number, Object, Type},
    },
};
use std::rc::Rc;

type RuntimeResult<T> = Result<T, Signal>;

fn error(position: &Position, message: impl Into<String>) -> Signal {
    Signal::Error(Diagnostic::new(position.clone(), message.into()))
}

fn size(number: &Number, position: &Position) -> RuntimeResult<usize> {
    usize::try_from(number).map_err(|_| {
        error(
            position,
            "Size or index must be nonnegative and fit in addressable memory",
        )
    })
}

fn reserve<T>(values: &mut Vec<T>, additional: usize, position: &Position) -> RuntimeResult<()> {
    values.try_reserve_exact(additional).map_err(|_| {
        error(
            position,
            "Collection allocation is too large or memory is unavailable",
        )
    })
}

fn range_length(start: &Number, end: &Number) -> Number {
    if end < start {
        Number::default()
    } else {
        end - start + 1u8
    }
}

fn lookup<'a>(
    collection: &'a Object,
    index: &Object,
    position: &Position,
) -> RuntimeResult<Option<&'a Object>> {
    match (collection, index) {
        (Object::List(_, values), Object::Number(index)) => Ok(usize::try_from(index)
            .ok()
            .and_then(|index| values.get(index))),
        (Object::Map(_, _, data), key) => {
            let key =
                Key::from_object(key).ok_or_else(|| error(position, "Invalid dictionary key"))?;

            Ok(data.get(&key))
        }

        (Object::Record(fields), Object::String(name)) => Ok(fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)),
        _ => Err(error(position, "Invalid collection or key")),
    }
}

fn read(collection: &Object, index: &Object, position: &Position) -> EvaluationResult {
    lookup(collection, index, position)?
        .cloned()
        .ok_or_else(|| {
            error(
                position,
                format!("Missing key or index out of bounds: {index}"),
            )
        })
}

fn write_path(
    collection: &mut Object,
    indices: &[Object],
    value: Object,
    position: &Position,
) -> RuntimeResult<()> {
    let Some((index, remaining)) = indices.split_first() else {
        *collection = value;

        return Ok(());
    };

    match collection {
        Object::List(_, values) => {
            let Object::Number(index) = index else {
                return Err(error(position, "List index must be Number"));
            };
            let index = size(index, position)?;

            if index >= values.len() {
                return Err(error(position, "List index out of bounds"));
            }

            write_path(&mut Rc::make_mut(values)[index], remaining, value, position)
        }

        Object::Map(_, _, data) => {
            let key =
                Key::from_object(index).ok_or_else(|| error(position, "Invalid dictionary key"))?;

            if remaining.is_empty() {
                let data = Rc::make_mut(data);

                if !data.indices.contains_key(&key) {
                    reserve(&mut data.entries, 1, position)?;
                    data.indices
                        .try_reserve(1)
                        .map_err(|_| error(position, "Dictionary allocation failed"))?;
                }

                data.insert(key, value);

                Ok(())
            } else {
                let index = *data
                    .indices
                    .get(&key)
                    .ok_or_else(|| error(position, "Missing intermediate dictionary key"))?;
                write_path(
                    &mut Rc::make_mut(data).entries[index].1,
                    remaining,
                    value,
                    position,
                )
            }
        }

        Object::Record(fields) => {
            let Object::String(name) = index else {
                return Err(error(position, "Record key must be String"));
            };
            let index = fields
                .iter()
                .position(|(key, _)| key == name)
                .ok_or_else(|| error(position, "Missing record field"))?;
            write_path(
                &mut Rc::make_mut(fields)[index].1,
                remaining,
                value,
                position,
            )
        }

        _ => Err(error(position, "Cannot mutate a scalar through an index")),
    }
}

impl Evaluator {
    pub(super) fn evaluate_each(&mut self, statement: &BoundEachStatement) -> EvaluationResult {
        let iterable = self.evaluate_expression(&statement.iterable)?;

        self.with_scope(|evaluator| {
            let mut visit = |value| {
                evaluator.with_scope(|iteration| {
                    iteration
                        .bindings
                        .borrow_mut()
                        .set(statement.name.clone(), value);
                    iteration.evaluate_statement(&statement.body)
                })
            };

            match iterable {
                Object::Range(mut start, end) => {
                    while start <= end {
                        visit(Object::Number(start.clone()))?;
                        start += 1u8;
                    }
                }

                Object::List(_, values) => {
                    for value in values.iter() {
                        visit(value.clone())?;
                    }
                }

                Object::Map(_, _, data) => {
                    for (_, value) in &data.entries {
                        visit(value.clone())?;
                    }
                }

                _ => {
                    return Err(error(
                        &statement.iterable.get_position(),
                        "Value is not iterable",
                    ))
                }
            }

            Ok(Object::Unit)
        })
    }

    pub(super) fn evaluate_collection(
        &mut self,
        expression: &BoundCollectionExpression,
    ) -> EvaluationResult {
        let position = &expression.position;

        match &expression.kind {
            Kind::List(elements) => {
                let Type::List(element_type) = &expression.typ else {
                    return Err(error(position, "Invalid list type"));
                };
                let mut values = Vec::new();
                reserve(&mut values, elements.len(), position)?;

                for element in elements {
                    values.push(self.evaluate_expression(element)?);
                }

                Ok(Object::List((**element_type).clone(), Rc::new(values)))
            }

            Kind::Repeat(value, count) => {
                let value = self.evaluate_expression(value)?;
                let Object::Number(count) = self.evaluate_expression(count)? else {
                    return Err(error(position, "Repetition count must be Number"));
                };
                let count = size(&count, position)?;
                let mut values = Vec::new();
                reserve(&mut values, count, position)?;
                values.resize(count, value.clone());

                Ok(Object::List(value.get_type(), Rc::new(values)))
            }

            Kind::Record(fields) => {
                let mut values = Vec::new();
                reserve(&mut values, fields.len(), position)?;

                for (name, value) in fields {
                    values.push((name.clone(), self.evaluate_expression(value)?));
                }

                Ok(Object::Record(Rc::new(values)))
            }

            Kind::Map(entries) => {
                let Type::Map(key_type, value_type) = &expression.typ else {
                    return Err(error(position, "Invalid dictionary type"));
                };
                let mut data = MapData::default();
                reserve(&mut data.entries, entries.len(), position)?;
                data.indices
                    .try_reserve(entries.len())
                    .map_err(|_| error(position, "Dictionary allocation failed"))?;

                for (key, value) in entries {
                    let key = self.evaluate_expression(key)?;
                    let key = Key::from_object(&key)
                        .ok_or_else(|| error(position, "Invalid dictionary key"))?;
                    let value = self.evaluate_expression(value)?;
                    data.insert(key, value);
                }

                Ok(Object::Map(
                    (**key_type).clone(),
                    (**value_type).clone(),
                    Rc::new(data),
                ))
            }

            Kind::Range(start, end) => {
                let Object::Number(start) = self.evaluate_expression(start)? else {
                    return Err(error(position, "Range start must be Number"));
                };
                let Object::Number(end) = self.evaluate_expression(end)? else {
                    return Err(error(position, "Range end must be Number"));
                };

                Ok(Object::Range(start, end))
            }

            Kind::Index(target, index) => {
                let target = self.evaluate_expression(target)?;
                let index = self.evaluate_expression(index)?;
                read(&target, &index, position)
            }

            Kind::Assign {
                name,
                indices,
                value,
            } => {
                // Indices are evaluated once, left-to-right, followed by the RHS.
                // The write uses the root variable's value after these evaluations.
                let mut path = Vec::new();

                for index in indices {
                    path.push(self.evaluate_expression(index)?);
                }

                let value = self.evaluate_expression(value)?;
                self.bindings
                    .borrow_mut()
                    .update(name, |root| {
                        write_path(root, &path, value.clone(), position)
                    })
                    .ok_or_else(|| error(position, "Assignment root is not initialized"))??;

                Ok(value)
            }

            Kind::Call(callee, expressions) => {
                let callee = self.evaluate_expression(callee)?;
                let mut args = Vec::new();

                for expression in expressions {
                    args.push(self.evaluate_expression(expression)?);
                }

                self.invoke(callee, args, position)
            }
        }
    }
}

pub(super) fn builtin(name: &str, args: Vec<Object>, position: &Position) -> EvaluationResult {
    let first = &args[0]; // The binder verifies arity before execution.

    match name {
        "len" => {
            let count = match first {
                Object::List(_, values) => values.len().into(),
                Object::Map(_, _, data) => data.entries.len().into(),
                Object::Record(fields) => fields.len().into(),
                Object::String(text) => text.chars().count().into(),
                Object::Range(start, end) => range_length(start, end),
                _ => {
                    return Err(error(
                        position,
                        "len requires a collection, string, or range",
                    ))
                }
            };

            Ok(Object::Number(count))
        }

        "collect" => {
            let Object::Range(start, end) = first else {
                return Err(error(position, "collect requires a range"));
            };
            let count = size(&range_length(start, end), position)?;
            let mut values = Vec::new();
            reserve(&mut values, count, position)?;
            let mut value = start.clone();

            for _ in 0..count {
                values.push(Object::Number(value.clone()));
                value += 1u8;
            }

            Ok(Object::List(Type::Number, Rc::new(values)))
        }

        "push" => {
            let Object::List(typ, values) = first else {
                return Err(error(position, "push requires a list"));
            };
            let mut values = Rc::clone(values);
            let data = Rc::make_mut(&mut values);
            reserve(data, 1, position)?;
            data.push(args[1].clone());

            Ok(Object::List(typ.clone(), values))
        }

        "pop" => {
            let Object::List(typ, values) = first else {
                return Err(error(position, "pop requires a list"));
            };

            if values.is_empty() {
                return Err(error(position, "Cannot pop an empty list"));
            }

            let mut values = Rc::clone(values);
            let value = Rc::make_mut(&mut values).pop().expect("nonempty list");

            Ok(Object::Record(Rc::new(vec![
                ("items".into(), Object::List(typ.clone(), values)),
                ("value".into(), value),
            ])))
        }

        "has" => Ok(Object::Boolean(
            lookup(first, &args[1], position)?.is_some(),
        )),
        "get" => Ok(lookup(first, &args[1], position)?
            .cloned()
            .unwrap_or_else(|| args[2].clone())),
        "keys" | "values" => {
            let (typ, values) = match first {
                Object::List(typ, values) if name == "values" => {
                    return Ok(Object::List(typ.clone(), Rc::clone(values)))
                }

                Object::List(_, values) => (
                    Type::Number,
                    (0..values.len())
                        .map(|index| Object::Number(index.into()))
                        .collect(),
                ),
                Object::Map(key, value, data) => {
                    if name == "keys" {
                        (
                            key.clone(),
                            data.entries
                                .iter()
                                .map(|(key, _)| key.to_object())
                                .collect(),
                        )
                    } else {
                        (
                            value.clone(),
                            data.entries
                                .iter()
                                .map(|(_, value)| value.clone())
                                .collect(),
                        )
                    }
                }

                Object::Record(fields) => {
                    if name == "keys" {
                        (
                            Type::String,
                            fields
                                .iter()
                                .map(|(name, _)| Object::String(name.clone()))
                                .collect(),
                        )
                    } else {
                        (
                            fields
                                .first()
                                .map_or(Type::Unit, |(_, value)| value.get_type()),
                            fields.iter().map(|(_, value)| value.clone()).collect(),
                        )
                    }
                }

                _ => return Err(error(position, "Expected a collection")),
            };

            Ok(Object::List(typ, Rc::new(values)))
        }

        "remove" => {
            let mut value = first.clone();

            match &mut value {
                Object::List(_, values) => {
                    let Object::Number(index) = &args[1] else {
                        return Err(error(position, "List index must be Number"));
                    };
                    let index = size(index, position)?;

                    if index >= values.len() {
                        return Err(error(position, "List index out of bounds"));
                    }

                    Rc::make_mut(values).remove(index);
                }

                Object::Map(_, _, data) => {
                    let key = Key::from_object(&args[1])
                        .ok_or_else(|| error(position, "Invalid dictionary key"))?;
                    Rc::make_mut(data).remove(&key);
                }

                Object::Record(fields) => {
                    let Object::String(name) = &args[1] else {
                        return Err(error(position, "Record key must be String"));
                    };
                    Rc::make_mut(fields).retain(|(key, _)| key != name);
                }

                _ => return Err(error(position, "Expected a collection")),
            }

            Ok(value)
        }

        _ => Err(error(position, "Unknown built-in")),
    }
}
