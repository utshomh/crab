use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::common::types::Type;

#[derive(Debug, Default)]
pub struct Bindings {
    pub(crate) outer: Option<Rc<RefCell<Bindings>>>,
    bindings: HashMap<String, Type>,
}

impl Bindings {
    pub fn extend(with: Rc<RefCell<Bindings>>) -> Self {
        Self {
            outer: Some(with),
            bindings: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<Type> {
        if let Some(object) = self.bindings.get(name) {
            Some(object.clone())
        } else if let Some(outer) = &self.outer {
            outer.borrow().get(name)
        } else {
            None
        }
    }

    /// Clone this scope and its parents without sharing mutable binding state.
    pub fn snapshot(&self) -> Self {
        Self {
            bindings: self.bindings.clone(),
            outer: self
                .outer
                .as_ref()
                .map(|outer| Rc::new(RefCell::new(outer.borrow().snapshot()))),
        }
    }

    pub(crate) fn set(&mut self, name: String, typ: Type) {
        self.bindings.insert(name, typ);
    }
}
