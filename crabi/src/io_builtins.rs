use super::evaluator::{EvaluationResult, Signal};
use crab::common::{
    diagnostic::{Diagnostic, Position},
    types::{Number, Object, Type},
};
use std::{
    fs,
    io::{self, Read, Write},
    rc::Rc,
};

fn number(text: &str) -> Option<Number> {
    let text = text.trim();
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);

    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    text.parse().ok()
}

fn boolean(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn read_line() -> io::Result<(String, bool)> {
    let mut text = String::new();
    let eof = io::stdin().read_line(&mut text)? == 0;

    if text.ends_with('\n') {
        text.pop();

        if text.ends_with('\r') {
            text.pop();
        }
    }

    Ok((text, eof))
}

fn parsed(ok: bool, value: Object) -> Object {
    Object::Record(Rc::new(vec![
        ("ok".into(), Object::Boolean(ok)),
        ("value".into(), value),
    ]))
}

pub(super) fn invoke(name: &str, args: &[Object], position: &Position) -> EvaluationResult {
    let error = |message: String| Signal::Error(Diagnostic::new(position.clone(), message));
    let text = |index: usize| match args.get(index) {
        Some(Object::String(text)) => Ok(text.as_str()),
        _ => Err(error(format!("'{name}' requires String arguments"))),
    };

    match name {
        "input" => {
            if !args.is_empty() {
                let mut stdout = io::stdout().lock();
                write!(stdout, "{}", text(0)?)
                    .and_then(|_| stdout.flush())
                    .map_err(|failure| error(format!("Cannot write input prompt: {failure}")))?;
            }

            let (line, eof) = read_line()
                .map_err(|failure| error(format!("Cannot read terminal input: {failure}")))?;

            if eof {
                return Err(error(
                    "End of input; use readLine() to handle EOF explicitly".into(),
                ));
            }

            Ok(Object::String(line))
        }

        "readLine" => {
            let (text, eof) = read_line()
                .map_err(|failure| error(format!("Cannot read terminal input: {failure}")))?;

            Ok(Object::Record(Rc::new(vec![
                ("text".into(), Object::String(text)),
                ("eof".into(), Object::Boolean(eof)),
            ])))
        }

        "readAll" => {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .map_err(|failure| error(format!("Cannot read standard input: {failure}")))?;

            Ok(Object::String(text))
        }

        "readFile" | "readLines" => {
            let path = text(0)?;
            let contents = fs::read_to_string(path)
                .map_err(|failure| error(format!("Cannot read '{path}': {failure}")))?;

            if name == "readFile" {
                Ok(Object::String(contents))
            } else {
                Ok(Object::List(
                    Type::String,
                    Rc::new(
                        contents
                            .lines()
                            .map(|line| Object::String(line.to_owned()))
                            .collect(),
                    ),
                ))
            }
        }

        "parseNumber" | "tryParseNumber" => {
            let value = number(text(0)?);

            if name == "tryParseNumber" {
                Ok(parsed(
                    value.is_some(),
                    Object::Number(value.unwrap_or_default()),
                ))
            } else {
                value.map(Object::Number).ok_or_else(|| {
                    error("Invalid number: expected a signed decimal integer".into())
                })
            }
        }

        "parseBoolean" | "tryParseBoolean" => {
            let value = boolean(text(0)?);

            if name == "tryParseBoolean" {
                Ok(parsed(
                    value.is_some(),
                    Object::Boolean(value.unwrap_or(false)),
                ))
            } else {
                value
                    .map(Object::Boolean)
                    .ok_or_else(|| error("Invalid boolean: expected 'true' or 'false'".into()))
            }
        }

        "trim" => Ok(Object::String(text(0)?.trim().to_owned())),
        "split" => {
            let separator = text(1)?;

            if separator.is_empty() {
                return Err(error("split separator must not be empty".into()));
            }

            Ok(Object::List(
                Type::String,
                Rc::new(
                    text(0)?
                        .split(separator)
                        .map(|part| Object::String(part.to_owned()))
                        .collect(),
                ),
            ))
        }

        _ => Err(error(format!("Unknown I/O built-in '{name}'"))),
    }
}
