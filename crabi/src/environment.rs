use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crab::common::types::Object;

#[derive(Debug, Default)]
pub(crate) struct Environment {
    pub(crate) outer: Option<Rc<RefCell<Environment>>>,
    bindings: HashMap<String, Object>,
}

impl Environment {
    pub(crate) fn extend(with: Rc<RefCell<Environment>>) -> Self {
        Self {
            outer: Some(with),
            bindings: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<Object> {
        if let Some(object) = self.bindings.get(name) {
            Some(object.clone())
        } else if let Some(outer) = &self.outer {
            outer.borrow().get(name)
        } else {
            None
        }
    }

    pub(crate) fn reset(&mut self, name: String, object: Object) -> bool {
        if let Some(value) = self.bindings.get_mut(&name) {
            *value = object;
            true
        } else if let Some(outer) = &self.outer {
            outer.borrow_mut().reset(name, object)
        } else {
            false
        }
    }

    pub(crate) fn snapshot(&self) -> Self {
        Self {
            bindings: self.bindings.clone(),
            outer: self
                .outer
                .as_ref()
                .map(|outer| Rc::new(RefCell::new(outer.borrow().snapshot()))),
        }
    }

    pub(crate) fn set(&mut self, name: String, object: Object) {
        self.bindings.insert(name, object);
    }
}
