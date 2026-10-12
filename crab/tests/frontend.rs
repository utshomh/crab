use crab::{
    binding::{
        bindings::Bindings,
        bound_tree::{BoundExpression, BoundStatement},
    },
    common::{diagnostic::Position, types::Object},
    compilation::Compilation,
};
use std::{cell::RefCell, rc::Rc};

fn compile(source: &str) -> Compilation {
    Compilation::compile(source, Rc::new(RefCell::new(Bindings::default())))
}

fn messages(compilation: &Compilation) -> Vec<String> {
    compilation
        .diagnostic_bag
        .borrow()
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn empty_and_whitespace_programs_are_valid() {
    for source in ["", " \t\r\n"] {
        let compilation = compile(source);
        assert!(messages(&compilation).is_empty());
        assert!(compilation.bound_program.is_empty());
    }
}

#[test]
fn strings_preserve_empty_unicode_multiline_and_literal_backslashes() {
    for value in [
        "",
        "a",
        "hello",
        "\u{e9}\u{1f980}",
        "first\nsecond",
        r"a\nb",
    ] {
        let compilation = compile(&format!("\"{value}\""));
        assert!(
            messages(&compilation).is_empty(),
            "{:?}",
            messages(&compilation)
        );
        let BoundStatement::Expression(statement) = &compilation.bound_program[0] else {
            panic!()
        };
        let BoundExpression::Literal(literal) = &statement.expression else {
            panic!()
        };
        assert_eq!(literal.value, Object::String(value.to_owned()));
    }
}

#[test]
fn unterminated_strings_are_diagnosed() {
    for source in ["\"", "print \"a", "print \"a\nsecond"] {
        let compilation = compile(source);
        assert_eq!(messages(&compilation), ["Unterminated string literal"]);
        assert!(compilation.bound_program.is_empty());
    }
}

#[test]
fn incomplete_and_invalid_syntax_is_diagnosed() {
    for source in [
        "{",
        "{ print 1",
        "{{}",
        "print",
        "var",
        "var x =",
        "(1 + 2",
        "for i = 1 to",
        "while true",
        "if true",
        "}",
        "1 & 2",
        "var = 1",
    ] {
        let compilation = compile(source);
        assert!(!messages(&compilation).is_empty(), "{source:?}");
        assert!(compilation.bound_program.is_empty(), "{source:?}");
    }
}

#[test]
fn integer_literal_limits_are_checked() {
    for source in ["2147483647", "-2147483648", "-0002147483648"] {
        assert!(messages(&compile(source)).is_empty(), "{source}");
    }
    for source in ["2147483648", "-2147483649", "99999999999999999999999999999"] {
        assert!(
            messages(&compile(source))
                .iter()
                .any(|m| m.contains("32-bit range")),
            "{source}"
        );
    }
}

#[test]
fn unicode_errors_use_byte_spans() {
    let source = "print \"\u{e9}\"\n\u{1f980}";
    let compilation = compile(source);
    let bag = compilation.diagnostic_bag.borrow();
    let diagnostic = &bag.diagnostics[0];
    assert_eq!(
        &source[diagnostic.position.start..diagnostic.position.end],
        "\u{1f980}"
    );
    assert_eq!(diagnostic.position.get_line(source), 2);
}

#[test]
fn diagnostics_report_the_affected_line() {
    let source = "print missing\nprint 1\n";
    let compilation = compile(source);
    let bag = compilation.diagnostic_bag.borrow();
    assert_eq!(bag.diagnostics[0].position.get_line(source), 1);
    assert_eq!(
        &source[bag.diagnostics[0].position.start..bag.diagnostics[0].position.end],
        "missing"
    );
    assert_eq!(
        Position {
            start: source.len(),
            end: source.len()
        }
        .get_line(source),
        3
    );
}

#[test]
fn eof_errors_have_zero_width_spans_at_the_end() {
    let source = "{ print 1";
    let compilation = compile(source);
    let bag = compilation.diagnostic_bag.borrow();
    let position = &bag.diagnostics.last().unwrap().position;
    assert_eq!((position.start, position.end), (source.len(), source.len()));
}

#[test]
fn type_errors_are_rejected() {
    for source in [
        "print missing",
        "1 + true",
        "!1",
        "-true",
        "1 && 2",
        "var x = 1 x = false",
        "if 1 print 1",
        "while 1 print 1",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "{source}");
    }
}

#[test]
fn only_invalid_for_bounds_receive_type_errors() {
    for source in ["for i = true to 2 print i", "for i = 1 to false print i"] {
        assert_eq!(messages(&compile(source)).len(), 1, "{source}");
    }
    assert_eq!(messages(&compile("for i = true to false print i")).len(), 2);
}

#[test]
fn control_flow_declarations_do_not_escape_their_scope() {
    for source in [
        "if false var x = 1 print x",
        "if true var x = 1 else print x",
        "while false var x = 1 print x",
        "for i = 2 to 1 var x = i print x",
        "for i = 1 to 2 print i print i",
        "{ var x = 1 } print x",
    ] {
        assert!(
            messages(&compile(source))
                .iter()
                .any(|m| m.contains("not defined")),
            "{source}"
        );
    }
}

#[test]
fn successful_compilation_requires_explicit_binding_commit() {
    let bindings = Rc::new(RefCell::new(Bindings::default()));
    let first = Compilation::compile("var x = 1", Rc::clone(&bindings));
    assert!(messages(&first).is_empty());
    assert!(!messages(&Compilation::compile("x", bindings)).is_empty());
    assert!(messages(&Compilation::compile("x", first.bindings)).is_empty());
}

#[test]
fn failed_compilation_preserves_committed_bindings() {
    let initial = compile("var x = 1");
    let committed = initial.bindings;
    let failed = Compilation::compile("var x = false var y = missing", Rc::clone(&committed));
    assert!(!messages(&failed).is_empty());
    assert!(messages(&Compilation::compile("x + 1", Rc::clone(&committed))).is_empty());
    assert!(!messages(&Compilation::compile("y", Rc::clone(&committed))).is_empty());
    assert!(Rc::ptr_eq(&committed, &failed.bindings));
}

#[test]
fn nested_scope_snapshots_do_not_mutate_parent_bindings() {
    let initial = compile("var x = 1");
    let child = Rc::new(RefCell::new(Bindings::extend(Rc::clone(&initial.bindings))));
    let compilation = Compilation::compile("x = 2 var y = true", child);
    assert!(messages(&compilation).is_empty());
    assert!(!messages(&Compilation::compile("y", initial.bindings)).is_empty());
}
