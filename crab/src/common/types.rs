use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Display, Formatter, Result},
    rc::Rc,
};

/// Signed integers with no fixed bit-width limit.
pub use num_bigint::BigInt as Number;
type Boolean = bool;

#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    Unit,
    Number,
    Boolean,
    String,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Record(BTreeMap<String, Type>),
    Range,
    Function(Vec<Type>, Box<Type>),
    Builtin(String),
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Type::Function(parameters, result) => {
                write!(f, "fun(")?;

                for (index, typ) in parameters.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{typ}")?;
                }

                write!(f, ") -> {result}")
            }

            Type::Builtin(name) => write!(f, "builtin<{name}>"),
            Type::List(element) => write!(f, "List<{element}>"),
            Type::Map(key, value) => write!(f, "Map<{key}, {value}>"),
            Type::Record(fields) => {
                write!(f, "[")?;

                for (index, (name, typ)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{name}: {typ}")?;
                }

                write!(f, "]")
            }

            _ => write!(f, "{self:?}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Object {
    Unit,
    Number(Number),
    Boolean(Boolean),
    String(String),
    List(Type, Rc<Vec<Object>>),
    Map(Type, Type, Rc<MapData>),
    Record(Rc<Vec<(String, Object)>>),
    Range(Number, Number),
    Function(u64, Type),
    Builtin(String, Option<Type>),
}

impl Object {
    pub fn get_type(&self) -> Type {
        match self {
            Object::Unit => Type::Unit,
            Object::Number(_) => Type::Number,
            Object::Boolean(_) => Type::Boolean,
            Object::String(_) => Type::String,
            Object::List(element, _) => Type::List(Box::new(element.clone())),
            Object::Map(key, value, _) => Type::Map(Box::new(key.clone()), Box::new(value.clone())),
            Object::Record(fields) => Type::Record(
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.get_type()))
                    .collect(),
            ),
            Object::Range(_, _) => Type::Range,
            Object::Function(_, typ) => typ.clone(),
            Object::Builtin(name, signature) => signature
                .clone()
                .unwrap_or_else(|| Type::Builtin(name.clone())),
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
            Object::List(_, values) => {
                write!(f, "[")?;

                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }

                    display_element(f, value)?;
                }

                write!(f, "]")
            }

            Object::Map(_, _, data) => {
                write!(f, "[")?;

                for (index, (key, value)) in data.entries.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }

                    match key {
                        Key::Number(n) => write!(f, "{n}")?,
                        Key::String(s) => write!(f, "{s:?}")?,
                    }

                    write!(f, ": ")?;
                    display_element(f, value)?;
                }

                if data.entries.is_empty() {
                    write!(f, ":")?;
                }

                write!(f, "]")
            }

            Object::Record(fields) => {
                write!(f, "[")?;

                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }

                    write!(f, "{name}: ")?;
                    display_element(f, value)?;
                }

                write!(f, "]")
            }

            Object::Range(start, end) => write!(f, "{start}..{end}"),
            Object::Function(_, typ) => write!(f, "<{typ}>"),
            Object::Builtin(name, _) => write!(f, "<builtin {name}>"),
        }
    }
}

fn display_element(f: &mut Formatter<'_>, value: &Object) -> Result {
    if let Object::String(text) = value {
        write!(f, "{text:?}")
    } else {
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Number(Number),
    String(String),
}

impl Key {
    pub fn from_object(value: &Object) -> Option<Self> {
        match value {
            Object::Number(n) => Some(Self::Number(n.clone())),
            Object::String(s) => Some(Self::String(s.clone())),
            _ => None,
        }
    }

    pub fn to_object(&self) -> Object {
        match self {
            Self::Number(n) => Object::Number(n.clone()),
            Self::String(s) => Object::String(s.clone()),
        }
    }
}

/// Ordered dictionary storage with hashed lookups. Object clones share this data
/// until mutation, so independent values and REPL snapshots remain inexpensive.
#[derive(Debug, Clone, Default)]
pub struct MapData {
    pub entries: Vec<(Key, Object)>,
    pub indices: HashMap<Key, usize>,
}

impl PartialEq for MapData {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

impl MapData {
    pub fn get(&self, key: &Key) -> Option<&Object> {
        self.indices.get(key).map(|&index| &self.entries[index].1)
    }

    pub fn insert(&mut self, key: Key, value: Object) {
        if let Some(&index) = self.indices.get(&key) {
            self.entries[index].1 = value;
        } else {
            self.indices.insert(key.clone(), self.entries.len());
            self.entries.push((key, value));
        }
    }

    pub fn remove(&mut self, key: &Key) {
        if let Some(index) = self.indices.remove(key) {
            self.entries.remove(index);

            for (index, (key, _)) in self.entries.iter().enumerate().skip(index) {
                self.indices.insert(key.clone(), index);
            }
        }
    }
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unit, Self::Unit) => true,
            (Self::Function(a, _), Self::Function(b, _)) => a == b,
            (Self::Builtin(a, _), Self::Builtin(b, _)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::List(at, a), Self::List(bt, b)) => at == bt && a == b,
            (Self::Map(ak, av, a), Self::Map(bk, bv, b)) => ak == bk && av == bv && a == b,
            (Self::Record(a), Self::Record(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(name, value)| {
                        b.iter().any(|(key, other)| name == key && value == other)
                    })
            }

            (Self::Range(a, b), Self::Range(c, d)) => a == c && b == d,
            _ => false,
        }
    }
}

pub const BUILTINS: &[&str] = &[
    "print",
    "len",
    "push",
    "pop",
    "keys",
    "values",
    "has",
    "get",
    "remove",
    "collect",
    "map",
    "filter",
    "fold",
    "each",
    "sort",
    "str",
    "abs",
    "min",
    "max",
    "input",
    "readLine",
    "readAll",
    "readFile",
    "readLines",
    "parseNumber",
    "parseBoolean",
    "tryParseNumber",
    "tryParseBoolean",
    "trim",
    "split",
];
