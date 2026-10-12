use std::fmt::{Display, Formatter, Result};

/// Signed integers with no fixed bit-width limit.
pub use num_bigint::BigInt as Number;
type Boolean = bool;

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Unit,
    Number,
    Boolean,
    String,
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Type::Unit => write!(f, "{self:?}"),
            Type::Number => write!(f, "{self:?}"),
            Type::Boolean => write!(f, "{self:?}"),
            Type::String => write!(f, "{self:?}"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Object {
    Unit,
    Number(Number),
    Boolean(Boolean),
    String(String),
}

impl Object {
    pub fn get_type(&self) -> Type {
        match self {
            Object::Unit => Type::Unit,
            Object::Number(_) => Type::Number,
            Object::Boolean(_) => Type::Boolean,
            Object::String(_) => Type::String,
        }
    }

    pub fn as_number(&self) -> Number {
        match self {
            Object::Number(n) => n.clone(),
            o => panic!("Can not convert {o} to {}", Type::Number),
        }
    }

    pub fn as_boolean(&self) -> Boolean {
        match self {
            Object::Boolean(b) => *b,
            o => panic!("Can not convert {o} to {}", Type::Boolean),
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Object::String(s) => s.clone(),
            o => panic!("Can not convert {o} to {}", Type::String),
        }
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Object::Unit => write!(f, "()"),
            Object::Number(n) => write!(f, "{n}"),
            Object::Boolean(b) => write!(f, "{b}"),
            Object::String(s) => write!(f, "{s}"),
        }
    }
}
