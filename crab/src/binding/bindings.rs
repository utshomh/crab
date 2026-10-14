use crate::common::types::{Type, BUILTINS};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Debug)]
pub struct Bindings {
    pub(crate) outer: Option<Rc<RefCell<Bindings>>>,
    bindings: HashMap<String, (Type, String)>,
    next: Rc<Cell<u64>>,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            outer: None,
            bindings: BUILTINS
                .iter()
                .map(|name| {
                    (
                        name.to_string(),
                        (Type::Builtin(name.to_string()), name.to_string()),
                    )
                })
                .collect(),
            next: Rc::new(Cell::new(0)),
        }
    }
}

impl Bindings {
    pub fn extend(with: Rc<RefCell<Bindings>>) -> Self {
        let next = Rc::clone(&with.borrow().next);
        Self {
            outer: Some(with),
            bindings: HashMap::new(),
            next,
        }
    }

    pub(crate) fn resolve(&self, name: &str) -> Option<(Type, String)> {
        self.bindings.get(name).cloned().or_else(|| {
            self.outer
                .as_ref()
                .and_then(|outer| outer.borrow().resolve(name))
        })
    }

    pub fn symbols(&self) -> Vec<String> {
        let mut symbols: Vec<_> = self
            .bindings
            .values()
            .map(|(_, symbol)| symbol.clone())
            .collect();

        if let Some(outer) = &self.outer {
            symbols.extend(outer.borrow().symbols());
        }

        symbols
    }

    pub fn snapshot(&self) -> Self {
        self.snapshot_with(Rc::new(Cell::new(self.next.get())))
    }

    fn snapshot_with(&self, next: Rc<Cell<u64>>) -> Self {
        Self {
            bindings: self.bindings.clone(),
            outer: self
                .outer
                .as_ref()
                .map(|outer| Rc::new(RefCell::new(outer.borrow().snapshot_with(Rc::clone(&next))))),
            next,
        }
    }

    pub(crate) fn set(&mut self, name: String, typ: Type) -> String {
        let id = self.next.get();
        self.next.set(id + 1);
        let symbol = format!("{name}#{id}");
        self.bindings.insert(name, (typ, symbol.clone()));
        symbol
    }
}
