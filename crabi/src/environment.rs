use crab::{
    binding::bound_tree::BoundFunction,
    common::types::{Object, Type, BUILTINS},
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Debug, Clone)]
struct Frame {
    outer: Option<u64>,
    values: HashMap<String, Object>,
}

#[derive(Debug, Clone)]
pub(super) struct Closure {
    pub code: Rc<BoundFunction>,
    pub scope: u64,
}

#[derive(Debug, Clone)]
struct Store {
    frames: HashMap<u64, Frame>,
    functions: HashMap<u64, Closure>,
    next: u64,
}

/// Frames and closures reference arena IDs rather than each other through Rc.
/// This permits transactional snapshots and collection of recursive capture cycles.
#[derive(Debug, Clone)]
pub(crate) struct Environment {
    store: Rc<RefCell<Store>>,
    scope: u64,
}

impl Default for Environment {
    fn default() -> Self {
        let values = BUILTINS
            .iter()
            .map(|name| (name.to_string(), Object::Builtin(name.to_string(), None)))
            .collect();
        let store = Store {
            frames: HashMap::from([(
                0,
                Frame {
                    outer: None,
                    values,
                },
            )]),
            functions: HashMap::new(),
            next: 1,
        };
        Self {
            store: Rc::new(RefCell::new(store)),
            scope: 0,
        }
    }
}

impl Environment {
    pub(crate) fn extend(with: Rc<RefCell<Environment>>) -> Self {
        let environment = with.borrow();
        let mut store = environment.store.borrow_mut();
        let scope = store.next;
        store.next += 1;
        store.frames.insert(
            scope,
            Frame {
                outer: Some(environment.scope),
                values: HashMap::new(),
            },
        );
        Self {
            store: Rc::clone(&environment.store),
            scope,
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<Object> {
        let store = self.store.borrow();
        let mut scope = Some(self.scope);

        while let Some(id) = scope {
            let frame = store.frames.get(&id)?;

            if let Some(value) = frame.values.get(name) {
                return Some(value.clone());
            }

            scope = frame.outer;
        }

        None
    }

    pub(crate) fn update<T>(
        &mut self,
        name: &str,
        update: impl FnOnce(&mut Object) -> T,
    ) -> Option<T> {
        let mut store = self.store.borrow_mut();
        let mut scope = Some(self.scope);

        while let Some(id) = scope {
            let frame = store.frames.get_mut(&id)?;

            if let Some(value) = frame.values.get_mut(name) {
                return Some(update(value));
            }

            scope = frame.outer;
        }

        None
    }

    pub(crate) fn set(&mut self, name: String, value: Object) {
        self.store
            .borrow_mut()
            .frames
            .get_mut(&self.scope)
            .expect("live scope")
            .values
            .insert(name, value);
    }

    pub(crate) fn snapshot(&self) -> Self {
        Self {
            store: Rc::new(RefCell::new(self.store.borrow().clone())),
            scope: self.scope,
        }
    }

    pub(crate) fn function(&mut self, code: Rc<BoundFunction>) -> Object {
        let mut store = self.store.borrow_mut();
        let id = store.next;
        store.next += 1;
        let signature = code.signature.clone();
        store.functions.insert(
            id,
            Closure {
                code,
                scope: self.scope,
            },
        );
        Object::Function(id, signature)
    }

    pub(crate) fn closure(&self, id: u64) -> Option<Closure> {
        self.store.borrow().functions.get(&id).cloned()
    }

    pub(crate) fn captured(&self, scope: u64) -> Self {
        Self {
            store: Rc::clone(&self.store),
            scope,
        }
    }

    fn contains_function(typ: &Type) -> bool {
        match typ {
            Type::Function(_, _) => true,
            Type::List(element) | Type::Map(_, element) => Self::contains_function(element),
            Type::Record(fields) => fields.values().any(Self::contains_function),
            _ => false,
        }
    }

    pub(crate) fn release_scope(&mut self) {
        let mut store = self.store.borrow_mut();

        if !store
            .functions
            .values()
            .any(|closure| closure.scope == self.scope)
            && !store
                .frames
                .values()
                .any(|frame| frame.outer == Some(self.scope))
        {
            store.frames.remove(&self.scope);
        }
    }

    pub(crate) fn collect(&mut self, result: &Object, roots: &[String]) {
        let mut scopes = HashSet::new();
        let mut functions = HashSet::new();
        let mut cells = HashSet::new();
        let mut pending_cells: Vec<_> = roots
            .iter()
            .map(|symbol| (self.scope, symbol.clone()))
            .collect();
        let mut pending_values = vec![result.clone()];
        let mut store = self.store.borrow_mut();
        scopes.insert(self.scope);
        loop {
            while let Some((scope, symbol)) = pending_cells.pop() {
                let mut current = Some(scope);

                while let Some(id) = current {
                    scopes.insert(id);
                    let Some(frame) = store.frames.get(&id) else {
                        break;
                    };

                    if let Some(value) = frame.values.get(&symbol) {
                        if cells.insert((id, symbol.clone())) {
                            pending_values.push(value.clone());
                        }

                        break;
                    }

                    current = frame.outer;
                }
            }

            let Some(value) = pending_values.pop() else {
                break;
            };

            match value {
                Object::Function(id, _) => {
                    if functions.insert(id) {
                        if let Some(closure) = store.functions.get(&id) {
                            let mut scope = Some(closure.scope);

                            while let Some(id) = scope {
                                scopes.insert(id);
                                scope = store.frames.get(&id).and_then(|frame| frame.outer);
                            }

                            pending_cells.extend(
                                closure
                                    .code
                                    .captures
                                    .iter()
                                    .map(|symbol| (closure.scope, symbol.clone())),
                            );
                        }
                    }
                }

                Object::List(element, values) if Self::contains_function(&element) => {
                    pending_values.extend(values.iter().cloned())
                }

                Object::Map(_, element, data) if Self::contains_function(&element) => {
                    pending_values.extend(data.entries.iter().map(|(_, value)| value.clone()))
                }

                Object::Record(fields) => pending_values.extend(
                    fields
                        .iter()
                        .filter(|(_, value)| Self::contains_function(&value.get_type()))
                        .map(|(_, value)| value.clone()),
                ),
                _ => {}
            }
        }

        store.frames.retain(|id, _| scopes.contains(id));

        for (id, frame) in &mut store.frames {
            frame
                .values
                .retain(|symbol, _| cells.contains(&(*id, symbol.clone())));
        }

        store.functions.retain(|id, _| functions.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::Evaluator;
    use crab::{binding::bindings::Bindings, compilation::Compilation};

    #[test]
    fn unreachable_recursive_closures_and_redeclared_cells_are_collected() {
        let mut bindings = Rc::new(RefCell::new(Bindings::default()));
        let mut environment = Rc::new(RefCell::new(Environment::default()));

        for _ in 0..50 {
            let source = "fun recurse() -> Number { return recurse() }";
            let compilation = Compilation::compile(source, Rc::clone(&bindings));
            assert!(compilation.diagnostic_bag.borrow().diagnostics.is_empty());
            let roots = compilation.bindings.borrow().symbols();
            let mut evaluator = Evaluator::new(
                compilation.bound_program,
                Rc::new(RefCell::new(environment.borrow().snapshot())),
                Rc::from(source),
                roots,
            );
            evaluator.evaluate().unwrap();
            bindings = compilation.bindings;
            environment = evaluator.bindings;
            let env = environment.borrow();
            let store = env.store.borrow();
            assert_eq!(store.functions.len(), 1);
            assert_eq!(store.frames.len(), 1);
            assert_eq!(store.frames[&0].values.len(), BUILTINS.len() + 1);
        }
    }

    #[test]
    fn ordinary_loop_scopes_are_released_without_waiting_for_collection() {
        let source = "var total = 0 for i in 1..1000 { total = total + i }";
        let compilation = Compilation::compile(source, Rc::new(RefCell::new(Bindings::default())));
        let environment = Rc::new(RefCell::new(Environment::default()));
        let mut evaluator = Evaluator::new(
            compilation.bound_program.clone(),
            Rc::clone(&environment),
            Rc::from(source),
            compilation.bindings.borrow().symbols(),
        );
        evaluator
            .evaluate_statements(&compilation.bound_program)
            .unwrap();
        assert_eq!(environment.borrow().store.borrow().frames.len(), 1);
    }
}
