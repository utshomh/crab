use super::{
    collections,
    environment::Environment,
    evaluator::{EvaluationResult, Evaluator, Signal},
    io_builtins,
};
use crab::common::{
    diagnostic::{Diagnostic, Position},
    types::{Number, Object, Type},
};
use std::{cell::RefCell, rc::Rc};

impl Evaluator {
    pub(super) fn invoke(
        &mut self,
        callee: Object,
        args: Vec<Object>,
        position: &Position,
    ) -> EvaluationResult {
        let result = match callee {
            Object::Builtin(name, _) => self.builtin(&name, args, position),
            Object::Function(id, _) => {
                if self.depth >= 128 {
                    return Err(Diagnostic::new(
                        position.clone(),
                        "Maximum function call depth (128) exceeded".into(),
                    )
                    .into());
                }

                let closure = self.bindings.borrow().closure(id).ok_or_else(|| {
                    Diagnostic::new(position.clone(), "Function is no longer available".into())
                })?;

                if args.len() != closure.code.parameters.len() {
                    return Err(Diagnostic::new(
                        position.clone(),
                        "Wrong function argument count".into(),
                    )
                    .into());
                }

                let caller = Rc::clone(&self.bindings);
                let caller_source = Rc::clone(&self.source);
                let captured = Rc::new(RefCell::new(caller.borrow().captured(closure.scope)));
                self.bindings = Rc::new(RefCell::new(Environment::extend(captured)));
                self.source = Rc::clone(&closure.code.source);

                for (name, value) in closure.code.parameters.iter().zip(args) {
                    self.bindings.borrow_mut().set(name.clone(), value);
                }

                self.depth += 1;
                let result = match self.evaluate_statement(&closure.code.body) {
                    Ok(_) => Ok(Object::Unit),
                    Err(Signal::Return(value)) => Ok(value),
                    Err(Signal::Error(mut diagnostic)) => {
                        if diagnostic.source.is_none() {
                            diagnostic.source = Some(Rc::clone(&self.source));
                        }

                        diagnostic.message.push_str(&format!(
                            "\n  at {} (call line {})",
                            closure.code.name,
                            position.get_line(&caller_source)
                        ));

                        Err(Signal::Error(diagnostic))
                    }
                };
                self.depth -= 1;
                self.bindings.borrow_mut().release_scope();
                self.bindings = caller;
                self.source = caller_source;
                result
            }

            _ => Err(Diagnostic::new(position.clone(), "Value is not callable".into()).into()),
        };
        result.map_err(|signal| match signal {
            Signal::Error(mut diagnostic) => {
                if diagnostic.source.is_none() {
                    diagnostic.source = Some(Rc::clone(&self.source));
                }

                Signal::Error(diagnostic)
            }

            signal => signal,
        })
    }

    fn builtin(&mut self, name: &str, args: Vec<Object>, position: &Position) -> EvaluationResult {
        let error =
            |message: &str| Signal::Error(Diagnostic::new(position.clone(), message.into()));

        match name {
            "print" => {
                println!(
                    "{}",
                    args.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                );

                Ok(Object::Unit)
            }

            "input" | "readLine" | "readAll" | "readFile" | "readLines" | "parseNumber"
            | "parseBoolean" | "tryParseNumber" | "tryParseBoolean" | "trim" | "split" => {
                io_builtins::invoke(name, &args, position)
            }

            "str" => Ok(Object::String(args[0].to_string())),
            "abs" => {
                let Object::Number(value) = &args[0] else {
                    return Err(error("abs requires Number"));
                };

                Ok(Object::Number(if value < &Number::default() {
                    -value
                } else {
                    value.clone()
                }))
            }

            "min" | "max" => {
                let (Object::Number(a), Object::Number(b)) = (&args[0], &args[1]) else {
                    return Err(error("min/max require Number"));
                };

                Ok(Object::Number(if name == "min" {
                    a.min(b).clone()
                } else {
                    a.max(b).clone()
                }))
            }

            "map" | "filter" | "fold" | "each" => {
                let Object::List(element, values) = &args[0] else {
                    return Err(error("Expected List"));
                };
                let callback = args[if name == "fold" { 2 } else { 1 }].clone();
                let mut output = Vec::new();

                if matches!(name, "map" | "filter") {
                    output
                        .try_reserve_exact(values.len())
                        .map_err(|_| error("Collection allocation failed"))?;
                }

                let mut accumulator = if name == "fold" {
                    args[1].clone()
                } else {
                    Object::Unit
                };

                for value in values.iter() {
                    let parameters = if name == "fold" {
                        vec![accumulator.clone(), value.clone()]
                    } else {
                        vec![value.clone()]
                    };
                    let result = self.invoke(callback.clone(), parameters, position)?;

                    match name {
                        "map" => output.push(result),
                        "filter" => {
                            if let Object::Boolean(keep) = result {
                                if keep {
                                    output.push(value.clone());
                                }
                            } else {
                                return Err(error("filter callback must return Boolean"));
                            }
                        }

                        "fold" => accumulator = result,
                        "each" => {}
                        _ => unreachable!(),
                    }
                }

                match name {
                    "fold" => Ok(accumulator),
                    "each" => Ok(Object::Unit),
                    "filter" => Ok(Object::List(element.clone(), Rc::new(output))),
                    _ => {
                        let Type::Function(_, result) = callback.get_type() else {
                            return Err(error("Expected typed callback"));
                        };

                        Ok(Object::List(*result, Rc::new(output)))
                    }
                }
            }

            "sort" => {
                let Object::List(element, values) = &args[0] else {
                    return Err(error("sort requires List"));
                };
                // Stable bottom-up merge sort allows comparator failures to propagate.
                let mut sorted = values.as_ref().clone();
                let mut width = 1;

                while width < sorted.len() {
                    let mut merged = Vec::new();
                    merged
                        .try_reserve_exact(sorted.len())
                        .map_err(|_| error("Sort allocation failed"))?;
                    let mut start = 0;

                    while start < sorted.len() {
                        let middle = (start + width).min(sorted.len());
                        let end = middle.saturating_add(width).min(sorted.len());
                        let (mut left, mut right) = (start, middle);

                        while left < middle && right < end {
                            let Object::Number(order) = self.invoke(
                                args[1].clone(),
                                vec![sorted[left].clone(), sorted[right].clone()],
                                position,
                            )?
                            else {
                                return Err(error("sort comparator must return Number"));
                            };

                            if order <= Number::default() {
                                merged.push(sorted[left].clone());
                                left += 1;
                            } else {
                                merged.push(sorted[right].clone());
                                right += 1;
                            }
                        }

                        merged.extend_from_slice(&sorted[left..middle]);
                        merged.extend_from_slice(&sorted[right..end]);
                        start = end;
                    }

                    sorted = merged;
                    width = width.saturating_mul(2);
                }

                Ok(Object::List(element.clone(), Rc::new(sorted)))
            }

            _ => collections::builtin(name, args, position),
        }
    }
}
