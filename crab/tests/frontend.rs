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
    for source in ["\"", "print(\")a", "print(\")a\nsecond"] {
        let compilation = compile(source);
        assert!(messages(&compilation)
            .iter()
            .any(|message| message == "Unterminated string literal"));
        assert!(compilation.bound_program.is_empty());
    }
}

#[test]
fn incomplete_and_invalid_syntax_is_diagnosed() {
    for source in [
        "{",
        "{ print(1)",
        "{{}",
        "print(",
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
fn integer_literals_have_arbitrary_precision() {
    for source in [
        "2147483647",
        "-2147483648",
        "-0002147483648",
        "2147483648",
        "-2147483649",
        "99999999999999999999999999999",
    ] {
        assert!(messages(&compile(source)).is_empty(), "{source}");
    }

    let digits = "9".repeat(1000);
    let compilation = compile(&digits);
    assert!(messages(&compilation).is_empty());
    let BoundStatement::Expression(statement) = &compilation.bound_program[0] else {
        panic!()
    };
    let BoundExpression::Literal(literal) = &statement.expression else {
        panic!()
    };
    assert_eq!(literal.value.to_string(), digits);
}

#[test]
fn comments_are_ignored_and_do_not_change_string_contents_or_positions() {
    for source in [
        "// comment",
        "// comment\r\n// another",
        "// \u{1f980} at EOF",
    ] {
        assert!(messages(&compile(source)).is_empty());
        assert!(compile(source).bound_program.is_empty());
    }

    let compilation = compile("// first\nprint(missing) // trailing");
    let bag = compilation.diagnostic_bag.borrow();
    assert_eq!(
        bag.diagnostics[0]
            .position
            .get_line("// first\nprint(missing) // trailing"),
        2
    );
    assert!(messages(&compile("print(\"https://example.com\") // comment")).is_empty());
}

#[test]
fn unicode_errors_use_byte_spans() {
    let source = "print(\"é\")\n🦀";
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
    let source = "print(missing)\nprint(1)\n";
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
    let source = "{ print(1)";
    let compilation = compile(source);
    let bag = compilation.diagnostic_bag.borrow();
    let position = &bag.diagnostics.last().unwrap().position;
    assert_eq!((position.start, position.end), (source.len(), source.len()));
}

#[test]
fn type_errors_are_rejected() {
    for source in [
        "print(missing)",
        "1 + true",
        "!1",
        "-true",
        "1 && 2",
        "var x = 1 x = false",
        "if 1 print(1)",
        "while 1 print(1)",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "{source}");
    }
}

#[test]
fn only_invalid_for_bounds_receive_type_errors() {
    for source in ["for i = true to 2 print(i)", "for i = 1 to false print(i)"] {
        assert_eq!(messages(&compile(source)).len(), 1, "{source}");
    }

    assert_eq!(
        messages(&compile("for i = true to false print(i)")).len(),
        2
    );
}

#[test]
fn control_flow_declarations_do_not_escape_their_scope() {
    for source in [
        "if false var x = 1 print(x)",
        "if true var x = 1 else print(x)",
        "while false var x = 1 print(x)",
        "for i = 2 to 1 var x = i print(x)",
        "for i = 1 to 2 print(i) print(i)",
        "{ var x = 1 } print(x)",
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

#[test]
fn collection_types_are_structural_and_empty_values_keep_their_type() {
    for (source, expected) in [
        ("var a = [1, 2] a", "List<Number>"),
        ("var a = ['a', 'b'] a", "List<String>"),
        (
            "var a: List<List<Boolean>> = [[], [true]] a",
            "List<List<Boolean>>",
        ),
        (
            "var a: Map<String, List<Number>> = [:] a",
            "Map<String, List<Number>>",
        ),
        (
            "var a = [name: 'Crab', age: 1] a",
            "[age: Number, name: String]",
        ),
        ("var a = 1..100000000000000000000000000000000000 a", "Range"),
    ] {
        let compilation = compile(source);
        assert!(
            messages(&compilation).is_empty(),
            "{source}: {:?}",
            messages(&compilation)
        );
        let BoundStatement::Expression(statement) = compilation.bound_program.last().unwrap()
        else {
            panic!()
        };
        assert_eq!(statement.expression.get_type().to_string(), expected);
    }
}

#[test]
fn invalid_collection_types_and_operations_are_rejected() {
    for source in [
        "var a = []",
        "var a = [:]",
        "var a = [1, false]",
        "var a = [[1], [false]]",
        "var a = ['x': 1, 'y': false]",
        "var a = [1: 2, '1': 3]",
        "var a = [true: 1]",
        "var a = [x: 1, x: 2]",
        "var a: List<Number> = [false]",
        "var a: Map<Boolean, Number> = [:]",
        "var a = [1; true]",
        "var a = true..2",
        "var a = [1] a[0] = false",
        "var a = [1] print(a['0'])",
        "print(1[0])",
        "var a = [x: 1] print(a.y)",
        "var a = [x: 1] var key = 'x' print(a[key])",
        "var a = [x: 1] a.x = false",
        "var a = [x: 1] a.y = 2",
        "var a = ['x': 1] a[2] = 3",
        "(1 + 2) = 4",
        "collect(1..2)[0] = 3",
        "push([1], false)",
        "len(1)",
        "len()",
        "len([1], [2])",
        "unknown([1])",
        "collect([1])",
        "get([1], 0, false)",
        "has([1], '0')",
        "pop(1)",
        "values([x: 1, y: false])",
        "var key = 'x' remove([x: 1], key)",
        "for value in 1 print(value)",
        "for value in [x: 1] print(value)",
        "for value in [1] print(value) print(value)",
        "var a = ['x': 1] a = ['x': false]",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "accepted {source}");
    }
}

#[test]
fn missing_collection_delimiters_and_invalid_entries_terminate() {
    for source in [
        "[",
        "[1,",
        "[1;",
        "[1; 2",
        "['x':",
        "[name:",
        "[name: 1,",
        "len(",
        "len([1]",
        "var a = [1] a[",
        "var a: List<Number = []",
        "var a: Map< = [:]",
        "[1, 'a': 2]",
        "[x: 1, 'y': 2]",
        "[1..]",
        "1...2",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "accepted {source}");
    }
}

#[test]
fn function_types_and_invalid_function_programs_are_checked() {
    for source in [
        "return 1",
        "fun f(n) {}",
        "fun f(n: Number, n: Number) {}",
        "fun f() -> Number {}",
        "fun f() -> Number { if true { return 1 } }",
        "fun f() -> Number { return false }",
        "fun f() -> Number { return }",
        "fun f() { return 1 }",
        "fun f(n: Number) {} f()",
        "fun f(n: Number) {} f(false)",
        "var f = 1 f()",
        "fun f() {} fun f() {}",
        "var f: fun(Number) -> String = abs",
        "var f: fun(Number) -> Number = print",
        "map([1], fun(x: String) -> Number { return 1 })",
        "filter([1], fun(x: Number) -> Number { return x })",
        "each([1], fun(x: Number) -> Number { return x })",
        "sort([1], fun(x: Number, y: Number) -> Boolean { return true })",
        "fold([1], 0, fun(x: Number, y: Number) -> String { return 'x' })",
        "abs(false)",
        "min(1)",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "accepted {source}");
    }

    for source in [
        "var f: fun(Number) -> Number = abs",
        "var p: [write: fun(Number) -> Unit] = [write: print] p.write(1)",
        "fun f() -> Unit { return }",
        "fun f(n: Number) -> Number { if n > 0 { return n } else { return 0 } }",
    ] {
        assert!(
            messages(&compile(source)).is_empty(),
            "{source}: {:?}",
            messages(&compile(source))
        );
    }
}

#[test]
fn incomplete_function_syntax_is_diagnosed_without_hanging() {
    for source in [
        "fun",
        "fun f(",
        "fun f(x:",
        "fun f(x: fun(Number) ->",
        "fun f() ->",
        "fun f() {",
        "fun f() { return",
        "var f = fun(",
        "var f: fun(Number",
        "var r: [f: fun() -> Number = [:]",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "accepted {source}");
    }
}

#[test]
fn io_and_parsing_builtins_enforce_arity_and_string_types() {
    for source in [
        "input(1)",
        "input('a', 'b')",
        "readLine('x')",
        "readAll('x')",
        "readFile()",
        "readFile(1)",
        "readLines(false)",
        "parseNumber(1)",
        "parseBoolean(true)",
        "tryParseNumber()",
        "tryParseBoolean(0)",
        "trim(1)",
        "split('x')",
        "split('x', 1)",
        "var read: fun(Number) -> String = readFile",
        "var parse: fun(String) -> Number = parseBoolean",
    ] {
        assert!(!messages(&compile(source)).is_empty(), "accepted {source}");
    }

    for source in [
        "var ask: fun() -> String = input",
        "var ask: fun(String) -> String = input",
        "var parse: fun(String) -> Number = parseNumber",
        "var parser = [boolean: parseBoolean] parser.boolean('true')",
        "map(['1', '2'], parseNumber)",
        "var reader: fun() -> [text: String, eof: Boolean] = readLine",
    ] {
        assert!(
            messages(&compile(source)).is_empty(),
            "{source}: {:?}",
            messages(&compile(source))
        );
    }
}
