use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

struct SourceFile(PathBuf);

impl SourceFile {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "crab-test-{}-{}.crab",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, source).unwrap();
        Self(path)
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn run(arguments: &[&std::ffi::OsStr], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_crabi"))
        .args(arguments)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let mut stdout_pipe = child.stdout.take().unwrap();
    let mut stderr_pipe = child.stderr.take().unwrap();
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout_pipe.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr_pipe.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Output {
                status,
                stdout: stdout_reader.join().unwrap(),
                stderr: stderr_reader.join().unwrap(),
            };
        }

        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            let _ = stdout_reader.join();
            let stderr = stderr_reader.join().unwrap();
            panic!(
                "interpreter did not terminate: {}",
                String::from_utf8_lossy(&stderr)
            );
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn file(source: &str) -> Output {
    let file = SourceFile::new(source);
    run(&[file.0.as_os_str()], "")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .replace("\r\n", "\n")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_output(source: &str, expected: &str) {
    let output = file(source);
    assert!(output.status.success(), "{source}: {}", stderr(&output));
    assert_eq!(stdout(&output), expected, "{source}");
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_files_and_strings_terminate() {
    assert_output("", "");
    assert_output(" \n\t", "");
    assert_output("print(\"\")", "\n");
    assert_output("print(\"a\")", "a\n");
}

#[test]
fn malformed_programs_terminate_with_syntax_errors() {
    for source in [
        "print(\")",
        "print(\")abc",
        "{ print(1)",
        "{{}",
        "print(",
        "\u{e9}",
    ] {
        let output = file(source);
        assert_eq!(
            output.status.code(),
            Some(65),
            "{source}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("panicked"));
    }
}

#[test]
fn arithmetic_precedence_associativity_and_unary_operators() {
    assert_output(
        "print((1 + 2) * 3) print(10 - 3 - 2) print(20 / 2 / 2) print(- -1) print(+3) print(!false)",
        "9\n5\n5\n1\n3\ntrue\n",
    );
}

#[test]
fn comparisons_boolean_operators_and_assignment_values() {
    assert_output("var x = 0 var y = 0 print((x = y = 3)) print(x) print(y) print(2 > 1) print(2 >= 2) print(1 < 2) print(2 <= 2) print(1 == 1) print(1 != 2) print(true == !false) print(true && false || true)", "3\n3\n3\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn logical_operators_skip_errors_and_side_effects() {
    assert_output("var x = 0 print(false && (1 / 0 == 0)) print(true || (1 / 0 == 0)) print(false && ((x = 1) == 1)) print(true || ((x = 2) == 2)) print(x) print(true && ((x = 3) == 3)) print(false || ((x = 4) == 4)) print(x)", "false\ntrue\nfalse\ntrue\n0\ntrue\ntrue\n4\n");
}

#[test]
fn strings_preserve_unicode_multiline_and_backslashes() {
    assert_output(
        "print(\"é🦀\" + \"!\") print(\"first\nsecond\") print(\"a\\nb\")",
        "\u{e9}\u{1f980}!\nfirst\nsecond\na\\nb\n",
    );
}

#[test]
fn for_ranges_are_inclusive_empty_when_reversed_and_safe_at_limits() {
    assert_output("for i = 1 to 3 print(i) for i = 3 to 3 print(i) for i = 3 to 1 print(i) for i = 2147483647 to 2147483647 print(i) for i = -2147483648 to -2147483648 print(i)", "1\n2\n3\n3\n2147483647\n-2147483648\n");
}

#[test]
fn for_bounds_are_evaluated_once_and_counter_assignment_does_not_change_range() {
    assert_output("var lower = 1 var upper = 3 var calls = 0 for i = (calls = calls + lower) to upper { print(i) upper = 0 i = 100 } print(calls) print(upper)", "1\n2\n3\n1\n0\n");
}

#[test]
fn loop_and_branch_scopes_preserve_outer_variables() {
    assert_output("var i = 99 for i = 1 to 2 { for i = 3 to 3 print(i) print(i) } print(i) if true var i = false print(i) while false var i = false print(i) if false var x = 1 else var x = true", "3\n1\n3\n2\n99\n99\n99\n");
}

#[test]
fn shadowing_and_outer_assignments_work() {
    assert_output("var x = 1 { var x = x + 1 print(x) { x = 3 } print(x) } print(x) { x = 4 } print(x) var x = false print(x)", "2\n3\n1\n4\nfalse\n");
}

#[test]
fn example_programs_still_work() {
    assert_output(
        "var factorial = 1 for i = 1 to 5 factorial = factorial * i print(factorial)",
        "120\n",
    );
    assert_output("var previous = 0 var current = 1 var n = 10 while n > 1 { var t = current current = previous + current previous = t n = n - 1 } print(current)", "55\n");
    assert_output(
        "var greet = \"hello\" var name = \"utsho\" print(greet + \", \" + name + \"!\")",
        "hello, utsho!\n",
    );
}

#[test]
fn if_else_and_else_if_choose_the_correct_branch() {
    assert_output(
        "if false print(1) else if true print(2) else print(3) if true { print(4) } else { print(5) }",
        "2\n4\n",
    );
}

#[test]
fn arithmetic_extends_beyond_fixed_width_integers() {
    assert_output(
        "print(2147483647 + 1) print(-2147483648 - 1) print(2147483647 * 2) print(-(-2147483648)) print(-2147483648 / -1)",
        "2147483648\n-2147483649\n4294967294\n2147483648\n2147483648\n",
    );
    assert_output(
        "var huge = 10000000000000000000000000000000000000000 print(huge * huge) print(huge + 1) print(-huge - 1) print(-huge < huge) print(huge == -(-huge)) print(huge != huge + 1)",
        "100000000000000000000000000000000000000000000000000000000000000000000000000000000\n10000000000000000000000000000000000000001\n-10000000000000000000000000000000000000001\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn division_truncates_toward_zero_and_remains_exact_for_large_integers() {
    assert_output(
        "print(7 / 3) print(-7 / 3) print(7 / -3) print(-7 / -3) print(10000000000000000000000000000000000000001 / 10)",
        "2\n-2\n-2\n2\n1000000000000000000000000000000000000000\n",
    );
}

#[test]
fn division_by_zero_is_still_a_runtime_error() {
    for source in [
        "print(1 / 0)",
        "print(1000000000000000000000000000000000 / (2 - 2))",
    ] {
        let output = file(source);
        assert_eq!(
            output.status.code(),
            Some(70),
            "{source}: {}",
            stderr(&output)
        );
        assert!(stderr(&output).contains("Division by zero"));
        assert!(!stderr(&output).contains("panicked"));
    }
}

#[test]
fn for_ranges_support_large_positive_and_negative_bounds() {
    assert_output(
        "for i = 1000000000000000000000000000000000 to 1000000000000000000000000000000002 print(i) for i = -1000000000000000000000000000000002 to -1000000000000000000000000000000000 print(i) for i = 1000000000000000000000000000000002 to 1000000000000000000000000000000000 print(i)",
        "1000000000000000000000000000000000\n1000000000000000000000000000000001\n1000000000000000000000000000000002\n-1000000000000000000000000000000002\n-1000000000000000000000000000000001\n-1000000000000000000000000000000000\n",
    );
}

#[test]
fn comment_syntax_preserves_division_strings_and_crlf() {
    assert_output(
        "// heading\r\nprint(8 / 2) // inline\r\nprint(\"https://example.com\")\n// final comment",
        "4\nhttps://example.com\n",
    );
}

#[test]
fn diagnostics_point_to_the_correct_line_and_unicode_column() {
    let output = file("print(\"é\")\nprint(missing)\nprint(1)\n");
    assert!(stderr(&output).contains("[error in line: 2]"));
    assert!(stderr(&output).contains("\tprint(missing)\n\t      ^^^^^^^ --- here"));
    let output = file("print(\"é\" + 1)");
    assert!(stderr(&output).contains("\tprint(\"\u{e9}\" + 1)\n\t      ^^^^^^^ --- here"));
}

#[test]
fn repl_preserves_newlines_and_executes_the_final_submission_at_eof() {
    let output = run(&[], "var x = 1\nvar y = x\nprint(y)\n");
    assert!(output.status.success());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert!(stdout(&output).contains("1\n()\n"));
    assert!(run(&[], "").status.success());
}

#[test]
fn repl_discards_bindings_from_compile_errors() {
    let output = run(&[], "var x = missing\n\nprint(x)\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).contains("Name 'missing' is not defined"));
    assert!(stderr(&output).contains("Name 'x' is not defined"));
    assert!(!stderr(&output).contains("panicked"));
}

#[test]
fn repl_rolls_back_runtime_errors_and_continues() {
    let output = run(&[], "var x = 1\n\nx = 2\nvar y = 3\n{ print(1 / 0) }\n\nprint(x)\n\nprint(y)\n\nx = x + 1\nprint(x)\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).contains("Division by zero"));
    assert!(stderr(&output).contains("Name 'y' is not defined"));
    assert!(stdout(&output).contains("1\n()\n"));
    assert!(stdout(&output).contains("2\n()\n"));
    assert!(!stderr(&output).contains("panicked"));
}

#[test]
fn unreadable_files_report_io_errors() {
    let file = SourceFile::new("");
    fs::remove_file(&file.0).unwrap();
    let output = run(&[file.0.as_os_str()], "");
    assert_eq!(output.status.code(), Some(74));
    assert!(stderr(&output).contains("Cannot read"));
}

#[test]
fn standalone_examples_run_with_the_documented_output() {
    for (source, expected) in [
        (include_str!("../../example/syntax.crab"), ""),
        (
            include_str!("../../example/hello_world.crab"),
            "Hello, Crab!\n",
        ),
        (include_str!("../../example/addition.crab"), "15\n3\n12\n"),
        (
            include_str!("../../example/conditions.crab"),
            "Passed\nfalse\n",
        ),
        (include_str!("../../example/scope.crab"), "20\n10\n"),
        (
            include_str!("../../example/factorial.crab"),
            "30414093201713378043612608166064768844377641568960512000000000000\n",
        ),
        (include_str!("../../example/fibonacci.crab"), "139423224561697880139724382870407283950070256587697307264108962948325571622863290691557658876222521294125\n"),
        (
            include_str!("../../example/big_integers.crab"),
            "115792089237316195423570985008687907853269984665640564039457584007913129639936\n",
        ),
        (include_str!("../../example/gcd.crab"), "21\n"),
        (
            include_str!("../../example/two_counter_machine.crab"),
            "Counter 1:\n0\nCounter 2:\n7\n",
        ),
    ] {
        assert_output(source, expected);
    }
}

#[test]
fn machine_interpreter_handles_all_opcodes_and_both_zero_test_branches() {
    let machine = include_str!("../../example/two_counter_machine.crab");

    for (program, r1, r2, expected1, expected2) in [
        ("2213", "7", "0", "0", "7"),     // transfer r1 to r2
        ("2213", "0", "0", "0", "0"),     // r1 zero branch
        ("1214", "0", "7", "7", "0"),     // transfer r2 to r1
        ("1214", "0", "0", "0", "0"),     // r2 zero branch
        ("32323011", "0", "0", "0", "1"), // increment and decrement r1
        ("31324012", "0", "0", "1", "0"), // increment and decrement r2
        (
            "11",
            "99999999999999999999999999999999999999999",
            "0",
            "100000000000000000000000000000000000000000",
            "0",
        ),
    ] {
        let source = machine
            .replace("var program = 2213", &format!("var program = {program}"))
            .replace("var r1 = 7", &format!("var r1 = {r1}"))
            .replace("var r2 = 0", &format!("var r2 = {r2}"));
        assert_output(
            &source,
            &format!("Counter 1:\n{expected1}\nCounter 2:\n{expected2}\n"),
        );
    }
}

#[test]
fn machine_program_supports_large_encodings_and_configurable_addresses() {
    let machine = include_str!("../../example/two_counter_machine.crab");
    // At base 20: address 0 increments r1 and jumps to address 12; address 12
    // decrements r2 and jumps to address 13, or address 14 when r2 is zero.
    // The remaining slots halt. Packing address 12 already requires a large integer.
    let source = format!("var encoded = 241 var position = 1 for i = 1 to 12 position = position * 8000 encoded = encoded + 5864 * position\n{}",
        machine.replace("var base = 10", "var base = 20")
            .replace("var program = 2213", "var program = encoded")
            .replace("var r1 = 7", "var r1 = 0")
            .replace("var r2 = 0", "var r2 = 1"));
    assert_output(&source, "Counter 1:\n1\nCounter 2:\n0\n");
}

fn life_output(width: usize, height: usize, generations: usize, seed: &[(usize, usize)]) -> String {
    let mut board = vec![vec![false; width]; height];

    for &(x, y) in seed {
        board[y][x] = true;
    }

    let mut output = String::new();

    for generation in 0..=generations {
        output.push_str(&format!("Generation:\n{generation}\n"));

        for row in &board {
            for &alive in row {
                output.push(if alive { '#' } else { '.' });
            }

            output.push('\n');
        }

        output.push('\n');
        let mut next = vec![vec![false; width]; height];

        for y in 0..height {
            for x in 0..width {
                let mut neighbors = 0;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }

                        if let (Some(nx), Some(ny)) =
                            (x.checked_add_signed(dx), y.checked_add_signed(dy))
                        {
                            if nx < width && ny < height && board[ny][nx] {
                                neighbors += 1;
                            }
                        }
                    }
                }

                next[y][x] = neighbors == 3 || (board[y][x] && neighbors == 2);
            }
        }

        board = next;
    }

    output
}

#[test]
fn game_of_life_example_evolves_a_glider() {
    let source = include_str!("../../example/game_of_life.crab");
    let seed = [(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)];
    assert_output(source, &life_output(10, 10, 4, &seed));
}

#[test]
fn game_of_life_handles_still_lifes_oscillators_edges_and_large_grids() {
    let example = include_str!("../../example/game_of_life.crab");
    let seed_start = example.find("        // BEGIN SEED").unwrap();
    let seed_end = example.find("        // END SEED").unwrap();

    for (width, height, generations, seed) in [
        (5, 5, 3, vec![(1, 1), (2, 1), (1, 2), (2, 2)]), // stable block
        (5, 5, 4, vec![(1, 2), (2, 2), (3, 2)]),         // blinker oscillator
        (3, 3, 1, vec![(1, 1)]),                         // isolated cell dies
        (3, 3, 2, vec![(0, 0), (1, 0), (2, 0)]),         // fixed edges, no row wrapping
        (1, 1, 1, vec![(0, 0)]),                         // all neighbors out of bounds
        (4, 3, 2, vec![]),                               // empty rectangular grid
        (10, 10, 2, vec![(8, 8), (9, 8), (8, 9), (9, 9)]), // bits beyond 64-bit storage
        (4, 3, 0, vec![(1, 1)]),                         // only the initial generation
    ] {
        let condition = if seed.is_empty() {
            "false".to_owned()
        } else {
            seed.iter()
                .map(|(x, y)| format!("(x == {x} && y == {y})"))
                .collect::<Vec<_>>()
                .join(" || ")
        };
        let mut source = example.to_owned();
        source.replace_range(
            seed_start..seed_end,
            &format!("        if {condition} {{ board = board + weight }}\n"),
        );
        source = source
            .replace("var width = 10", &format!("var width = {width}"))
            .replace("var height = 10", &format!("var height = {height}"))
            .replace(
                "var generations = 4",
                &format!("var generations = {generations}"),
            );
        assert_output(&source, &life_output(width, height, generations, &seed));
    }
}

#[test]
fn lists_records_dictionaries_and_nested_mutations_work() {
    assert_output("var a = [1, 2, 3,] a[1] = 9 print(a) print(a[1]) print([0; 0]) var empty: List<Number> = [] empty = push(empty, 4) print(empty) var p = [name: 'Crab', age: 1, active: true,] p.age = 2 print(p['name']) print(p.age) print(p) var d: Map<String, Number> = [:] d['first'] = 1 d.second = 2 d['first'] = 3 print(d) print(keys(d)) print(values(d))",
        "[1, 9, 3]\n9\n[]\n[4]\nCrab\n2\n[name: \"Crab\", age: 2, active: true]\n[\"first\": 3, \"second\": 2]\n[\"first\", \"second\"]\n[3, 2]\n");
    assert_output("var users = [a: [scores: [1, 2]]] users.a.scores[0] = 99 print(users.a.scores) var d = ['a': [1]] d['b'] = [2] d.b[0] = 3 print(d)", "[99, 2]\n[\"a\": [1], \"b\": [3]]\n");
}

#[test]
fn repetition_and_collection_assignment_have_independent_value_semantics() {
    assert_output("var grid = [[0; 2]; 2] var copy = grid grid[0][0] = 7 print(grid) print(copy) var row = grid[0] row[1] = 8 print(grid) print(row) var record = [a: grid] var other = record other.a[1][1] = 9 print(record.a) print(other.a) var d = ['x': grid] var e = d e.x[0][0] = 3 print(d.x) print(e.x)",
        "[[7, 0], [0, 0]]\n[[0, 0], [0, 0]]\n[[7, 0], [0, 0]]\n[7, 8]\n[[7, 0], [0, 0]]\n[[7, 0], [0, 9]]\n[[7, 0], [0, 0]]\n[[3, 0], [0, 0]]\n");
    assert_output(
        "var a = [1] { a[0] = 2 } print(a) { var a = [3] a[0] = 4 print(a) } print(a)",
        "[2]\n[4]\n[2]\n",
    );
}

#[test]
fn ranges_are_lazy_inclusive_and_support_arbitrary_precision() {
    assert_output("var r = 1..4 print(r) print(len(r)) print(collect(r)) print(collect(4..1)) print(len(4..1)) print(len(0..100000000000000000000000000000000000000)) for n in 1000000000000000000000000000000..1000000000000000000000000000002 print(n) for n in 4..1 print(n) print(collect(-2..1))",
        "1..4\n4\n[1, 2, 3, 4]\n[]\n0\n100000000000000000000000000000000000001\n1000000000000000000000000000000\n1000000000000000000000000000001\n1000000000000000000000000000002\n[-2, -1, 0, 1]\n");
}

#[test]
fn iteration_uses_a_snapshot_and_local_loop_variables() {
    assert_output("var a = [1, 2] var n = 99 for n in a { print(n) a[1] = 9 } print(a) print(n) for item in ['x': 4, 'y': 5] print(item)", "1\n2\n[1, 9]\n99\n4\n5\n");
}

#[test]
fn collection_builtins_are_typed_and_do_not_mutate_their_arguments() {
    assert_output("var a = [1, 2] print(len(a)) print(push(a, 3)) print(a) var result = pop(a) print(result.items) print(result.value) print(a) print(keys(a)) print(values(a)) print(has(a, -1)) print(has(a, 1)) print(get(a, 99, 7)) print(remove(a, 0)) var d = ['a': 1, 'b': 2, 'c': 3] var trimmed = remove(d, 'b') trimmed['d'] = 4 trimmed['c'] = 5 print(keys(trimmed)) print(values(trimmed)) print(d) print(get(d, 'missing', 0)) print(has(d, 'missing')) print(len('?')) var p = [x: 1, y: 2] print(keys(p)) print(values(p)) print(get(p, 'missing', 7)) print(remove(p, 'x')) print(has(p, 'missing'))",
        "2\n[1, 2, 3]\n[1, 2]\n[1]\n2\n[1, 2]\n[0, 1]\n[1, 2]\nfalse\ntrue\n7\n[2]\n[\"a\", \"c\", \"d\"]\n[1, 5, 4]\n[\"a\": 1, \"b\": 2, \"c\": 3]\n0\nfalse\n1\n[\"x\", \"y\"]\n[1, 2]\n7\n[y: 2]\nfalse\n");
}

#[test]
fn structural_equality_ignores_dictionary_and_record_insertion_order() {
    assert_output("print([x: 1, y: true] == [y: true, x: 1]) print(['a': 1, 'b': 2] == ['b': 2, 'a': 1]) print([[1, 2]] == [[1, 2]]) print([1, 2] != [2, 1]) print('a' == 'a') print(1..2 == 1..2)", "true\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn dictionary_keys_are_exact_and_duplicate_keys_replace_in_place() {
    assert_output("var a = ['1': 2, '01': 3, '1': 4] print(a) print(keys(a)) print(a['01']) var b = [10000000000000000000000: 1, -1: 2] print(b[-1]) print(keys(b)) print(remove(a, 'not present'))", "[\"1\": 4, \"01\": 3]\n[\"1\", \"01\"]\n3\n2\n[10000000000000000000000, -1]\n[\"1\": 4, \"01\": 3]\n");
}

#[test]
fn collection_expression_side_effects_run_once_in_order() {
    assert_output("var i = 0 var a = [[0, 0], [0, 0]] a[(i = i + 1)][(i = i - 1)] = (i = i + 7) print(a) print(i) var calls = 0 print([(calls = calls + 1); (calls = calls + 1)]) print(calls) print(get([1], 0, (calls = calls + 1))) print(calls)", "[[0, 0], [7, 0]]\n7\n[1, 1]\n2\n1\n3\n");
}

#[test]
fn collection_failures_are_runtime_diagnostics() {
    for source in [
        "print([1][-1])",
        "print([1][9])",
        "print([1][9999999999999999999999999])",
        "var a = [1] a[9] = 2",
        "var a = [1] a[-1] = 2",
        "print(['a': 1]['b'])",
        "print(pop([0; 0]))",
        "print([0; -1])",
        "print([0; 99999999999999999999999999])",
        "print([0; 18446744073709551615])",
        "print(collect(0..99999999999999999999999999))",
        "print(remove([1], 9))",
        "var d = ['a': [1]] d.b[0] = 2",
    ] {
        let output = file(source);
        assert_eq!(
            output.status.code(),
            Some(70),
            "{source}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("panicked"), "{source}");
    }
}

#[test]
fn collection_repl_failures_roll_back_nested_mutations_and_aliases() {
    let output = run(&[], "var a = [[1]]\nvar b = a\nvar d = ['x': a]\n\na[0][0] = 9\nd.x[0][0] = 8\nvar added = [1]\nprint(1 / 0)\n\nprint(a)\nprint(b)\nprint(d.x)\n\nprint(added)\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).contains("Division by zero"));
    assert!(stderr(&output).contains("Name 'added' is not defined"));
    assert!(
        stdout(&output).contains("[[1]]\n[[1]]\n[[1]]\n()\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn collection_programs_produce_the_expected_results() {
    assert_output(include_str!("../../example/arrays.crab"),
        "[10, 2, 3, 4, 5]\n[0, 0, 0]\n[[false, true], [false, false]]\n[42]\n[]\n42\nUtsho\n26\n[\"Alice\", \"Bob\"]\n[90, 85]\ntrue\n0\n[\"Alice\": 90]\n");
    assert_output(
        include_str!("../../example/insertion_sort.crab"),
        "[-3, 0, 2, 7, 7, 42, 1000000000000000000000000]\n",
    );
    let primes: Vec<_> = (2..=100)
        .filter(|n| !(2..*n).any(|divisor| n % divisor == 0))
        .collect();
    assert_output(
        include_str!("../../example/prime_sieve.crab"),
        &format!("{primes:?}\n"),
    );
    assert_output(
        include_str!("../../example/frequency_count.crab"),
        "[\"crab\": 3, \"rust\": 2, \"arrays\": 1]\ncrab\n3\nrust\n2\narrays\n1\n",
    );
}

#[test]
fn array_game_of_life_matches_the_reference_simulation() {
    assert_output(
        include_str!("../../example/game_of_life_arrays.crab"),
        &life_output(10, 10, 4, &[(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)]),
    );
}

#[test]
fn insertion_sort_handles_empty_singleton_duplicates_and_reversed_inputs() {
    let example = include_str!("../../example/insertion_sort.crab");

    for values in [
        vec![],
        vec![1],
        vec![3, 3, 3],
        vec![5, 4, 3, 2, 1],
        vec![-5, 0, -1, 2],
    ] {
        let input = if values.is_empty() {
            "[0; 0]".into()
        } else {
            format!("{values:?}")
        };
        let source = example.replace("[42, -3, 7, 7, 0, 1000000000000000000000000, 2]", &input);
        let mut sorted = values;
        sorted.sort();
        assert_output(&source, &format!("{sorted:?}\n"));
    }
}

#[test]
fn breadth_first_search_reconstructs_a_shortest_path_and_detects_unreachable_goals() {
    let source = include_str!("../../example/pathfinding.crab");
    assert_output(source, "Shortest path length:\n10\nPath:\n[x: 0, y: 0]\n[x: 1, y: 0]\n[x: 2, y: 0]\n[x: 3, y: 0]\n[x: 4, y: 0]\n[x: 5, y: 0]\n[x: 5, y: 1]\n[x: 5, y: 2]\n[x: 5, y: 3]\n[x: 5, y: 4]\n[x: 5, y: 5]\n");
    let source = source.replace(
        "distance[start.y][start.x] = 0",
        "distance[start.y][start.x] = 0 maze[goal.y][goal.x] = 1",
    );
    assert_output(&source, "No path found\n");
}

#[test]
fn empty_collection_context_flows_through_assignments_records_and_builtins() {
    assert_output("var a: List<List<Number>> = [[]] a = push(a, []) print(a) a[0] = [] print(a) var d: Map<String, List<Number>> = [:] d.x = [] print(get(d, 'x', [])) print(get(d, 'missing', [])) var p = [items: [1]] p = [items: []] print(get(p, 'items', [])) d = [:] print(d) print(keys(d)) print(values(d))",
        "[[], []]\n[[], []]\n[]\n[]\n[]\n[:]\n[]\n[]\n");
}

#[test]
fn print_is_variadic_first_class_and_shadowable() {
    assert_output("var emit = print emit() emit('hello', 1, true, [1, 2]) var output = [write: print] output.write('record call') var typed: fun(Number) -> Unit = print typed(42) var print = fun(n: Number) -> Number { return n + 1 } emit(print(5))", "\nhello 1 true [1, 2]\nrecord call\n42\n6\n");
}

#[test]
fn named_anonymous_recursive_and_record_functions_are_callable() {
    assert_output(
        include_str!("../../example/functions.crab"),
        "25\n14\n2432902008176640000\n1 2 3\nFirst-class built-ins: [4, 3]\n",
    );
    assert_output("print(even(10), odd(9)) fun even(n: Number) -> Boolean { if n == 0 { return true } return odd(n - 1) } fun odd(n: Number) -> Boolean { if n == 0 { return false } return even(n - 1) } print((fun(x: Number) -> Number { return x * 2 })(7)) var fs = [even, odd] print(fs[0](4))", "true true\n14\ntrue\n");
}

#[test]
fn closures_share_state_preserve_lexical_bindings_and_live_after_return() {
    assert_output("fun make() -> [next: fun() -> Number, read: fun() -> Number] { var n = 0 return [next: fun() -> Number { n = n + 1 return n }, read: fun() -> Number { return n }] } var a = make() var copy = a var b = make() print(a.next(), copy.next(), a.read(), b.read()) var n = 10 var read = fun() -> Number { return n } var n = false print(read()) { var n = 99 print(read()) }", "1 2 2 0\n10\n10\n");
    assert_output("var fs = [fun() -> Number { return 0 }; 0] for i in 1..3 { fs = push(fs, fun() -> Number { return i }) } print(fs[0](), fs[1](), fs[2]()) var more = [fun() -> Number { return 0 }; 0] for i = 4 to 5 { more = push(more, fun() -> Number { return i }) } print(more[0](), more[1]())", "1 2 3\n4 5\n");
}

#[test]
fn return_exits_nested_loops_and_unit_returns_stop_at_newlines() {
    assert_output("fun find() -> Number { for i in 1..10 { while true { if i == 1 { return i } } } return -1 } fun early() { return\nprint('unreachable') } print(find()) early() fun noop() {} print(noop())", "1\n()\n");
}

#[test]
fn higher_order_builtins_accept_user_functions_and_builtin_aliases() {
    assert_output("var a = [3, 1, 2] print(map(a, fun(n: Number) -> Number { return n * n })) print(filter(a, fun(n: Number) -> Boolean { return n > 1 })) print(fold(a, 0, fun(total: Number, n: Number) -> Number { return total + n })) each([1, 2], print) print(sort(a, fun(a: Number, b: Number) -> Number { return a - b })) print(a) var length = len print(map(['a', 'abc'], length)) var typed: fun(List<Number>) -> Number = length print(typed(a)) print(abs(-99), min(3, 1), max(3, 1), 'n=' + str(42))", "[9, 1, 4]\n[3, 2]\n6\n1\n2\n[1, 2, 3]\n[3, 1, 2]\n[1, 3]\n3\n99 1 3 n=42\n");
    assert_output("var xs: List<Number> = [] print(map(xs, fun(n: Number) -> String { return str(n) })) print(filter(xs, fun(n: Number) -> Boolean { return true })) print(fold(xs, 7, fun(a: Number, b: Number) -> Number { return a + b })) print(sort(xs, fun(a: Number, b: Number) -> Number { return a - b }))", "[]\n[]\n7\n[]\n");
}

#[test]
fn comparator_sort_is_stable_and_callbacks_can_mutate_captures() {
    assert_output("var calls = 0 var xs = [key: 2, label: 'a'] var ys = [key: 1, label: 'b'] var zs = [key: 2, label: 'c'] var sorted = sort([xs, ys, zs], fun(a: [key: Number, label: String], b: [key: Number, label: String]) -> Number { calls = calls + 1 return a.key - b.key }) print(sorted[0].label, sorted[1].label, sorted[2].label) print(calls > 0)", "b a c\ntrue\n");
}

#[test]
fn call_evaluation_order_is_callee_then_arguments_once() {
    assert_output("var order = 0 fun callee() -> fun(Number, Number) -> Number { order = order * 10 + 1 return fun(a: Number, b: Number) -> Number { return a + b } } fun arg(n: Number) -> Number { order = order * 10 + n return n } print(callee()(arg(2), arg(3))) print(order)", "5\n123\n");
}

#[test]
fn function_errors_include_call_traces_and_recursion_is_bounded() {
    let output = file("fun inner() -> Number { return 1 / 0 } fun outer() -> Number { return inner() } print(outer())");
    assert_eq!(output.status.code(), Some(70));
    assert!(stderr(&output).contains("Division by zero"));
    assert!(stderr(&output).contains("at inner"));
    assert!(stderr(&output).contains("at outer"));
    let output = file("fun recurse() -> Number { return recurse() } recurse()");
    assert_eq!(output.status.code(), Some(70), "{}", stderr(&output));
    assert!(stderr(&output).contains("Maximum function call depth"));
    assert!(!stderr(&output).contains("panicked"));
}

#[test]
fn repl_closure_state_rolls_back_and_older_function_sources_are_reported() {
    let input = "fun make() -> fun() -> Number { var n = 0 return fun() -> Number { n = n + 1 return n } }\nvar counter = make()\nvar alias = counter\n\nprint(counter())\n\nprint(alias())\nprint(1 / 0)\n\nprint(counter())\n\nfun old() -> Number { return 1 / 0 }\n\nold()\n\n@exit\n\n";
    let output = run(&[], input);
    assert!(output.status.success());
    assert!(stdout(&output).contains("1\n()\n"), "{}", stdout(&output));
    assert!(!stdout(&output).contains("3\n"));
    assert_eq!(stderr(&output).matches("Division by zero").count(), 2);
    assert!(stderr(&output).contains("fun old() -> Number { return 1 / 0 }"));
    assert!(stderr(&output).contains("at old"));
}

#[test]
fn sophisticated_function_examples_work() {
    assert_output(include_str!("../../example/merge_sort.crab"), "[-3, 0, 2, 7, 7, 42, 1000000000000000000000000]\n[42, -3, 7, 7, 0, 1000000000000000000000000, 2]\n");
    assert_output(
        include_str!("../../example/functional_pipeline.crab"),
        "Total income: 400\n[\"Dan\", \"Alice\", \"Cara\"]\nDan\nAlice\nCara\n[3, 5, 4]\n",
    );
    assert_output(
        include_str!("../../example/closure_accounts.crab"),
        "125\ntrue\n85\n50\nfalse\n85\n",
    );
    assert_output(
        include_str!("../../example/game_of_life_functions.crab"),
        &life_output(10, 10, 4, &[(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)]),
    );
}

#[test]
fn callback_failure_rolls_back_captures_and_sort_does_not_partially_replace_the_input() {
    let output = run(&[], "var count = 0\nvar a = [3, 1, 2]\n\na = sort(a, fun(x: Number, y: Number) -> Number { count = count + 1 return 1 / 0 })\n\nprint(count, a)\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).contains("Division by zero"));
    assert!(stdout(&output).contains("0 [3, 1, 2]\n()\n"));
}

#[test]
fn repl_collection_preserves_captures_after_global_redeclaration_and_builtin_shadowing() {
    let output = run(&[], "var n = 10\nvar read = fun() -> Number { return n }\nfun say() { print('original print') }\n\nvar n = false\nvar print = fun(n: Number) -> Number { return n + 1 }\n\nsay()\nread()\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("original print\n10\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn user_function_collection_arguments_are_values_and_callable_identity_is_shared() {
    assert_output("fun change(xs: List<Number>) -> List<Number> { xs[0] = 99 return xs } var a = [1, 2] print(change(a)) print(a) var f = fun() -> Number { return 1 } var g = f var h = fun() -> Number { return 1 } print(f == g, f == h) var emit: fun(Number) -> Unit = print var writes = [emit] writes[0](8)", "[99, 2]\n[1, 2]\ntrue false\n8\n");
}

#[test]
fn parsing_handles_signs_whitespace_arbitrary_precision_and_exact_booleans() {
    assert_output("print(parseNumber(' +00042 '), parseNumber('-00042'), parseNumber('-0')) print(parseNumber('999999999999999999999999999999999999999')) print(parseBoolean(' true '), parseBoolean('false')) var parse = parseNumber print(map(['1', '-2', '+3'], parse)) var p = [read: parseBoolean] print(p.read('true')) print(trim('  crab  ')) print(split('a,,b', ',')) print(split('', ','))",
        "42 -42 0\n999999999999999999999999999999999999999\ntrue false\n[1, -2, 3]\ntrue\ncrab\n[\"a\", \"\", \"b\"]\n[\"\"]\n");
}

#[test]
fn try_parsers_return_typed_results_without_aborting() {
    assert_output("print(tryParseNumber('0'), tryParseNumber('bad')) print(tryParseBoolean('false'), tryParseBoolean('yes')) print(tryParseNumber('12').value + 1) print(!tryParseBoolean('true').value)",
        "[ok: true, value: 0] [ok: false, value: 0]\n[ok: true, value: false] [ok: false, value: false]\n13\nfalse\n");
}

#[test]
fn invalid_parses_and_empty_split_separators_are_diagnostics() {
    for source in [
        "parseNumber('')",
        "parseNumber('+')",
        "parseNumber('--1')",
        "parseNumber('1.5')",
        "parseNumber('0xff')",
        "parseNumber('1 2')",
        "parseNumber('NaN')",
        "parseNumber('??')",
        "parseBoolean('TRUE')",
        "parseBoolean('1')",
        "parseBoolean('')",
        "split('abc', '')",
    ] {
        let output = file(source);
        assert_eq!(
            output.status.code(),
            Some(70),
            "{source}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("panicked"));
    }
}

#[test]
fn terminal_input_handles_prompts_crlf_blank_lines_and_eof() {
    let program = SourceFile::new("var ask = input print(ask('Name: ')) print(input()) var line = readLine() print(line.text, line.eof) line = readLine() print(line.text, line.eof)");
    let output = run(&[program.0.as_os_str()], "Crab\r\n\r\nlast line");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "Name: Crab\n\nlast line false\n true\n");
    let program = SourceFile::new("input()");
    let output = run(&[program.0.as_os_str()], "");
    assert_eq!(output.status.code(), Some(70));
    assert!(stderr(&output).contains("End of input"));
}

#[test]
fn stdin_reads_preserve_unicode_and_all_remaining_text() {
    let program = SourceFile::new("print(readAll())");
    let output = run(&[program.0.as_os_str()], "?\ncrab\r\n");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "?\ncrab\n\n");
    let program = SourceFile::new(include_str!("../../example/stdin_sum.crab"));
    let output = run(
        &[program.0.as_os_str()],
        "10\n\n -3 \n1000000000000000000000000\n",
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Count: 3\nTotal: 1000000000000000000000007\n"
    );
}

#[test]
fn repl_and_input_share_the_same_stdin_buffer() {
    let output = run(&[], "var name = input('Name: ')\nprint(name)\n\nCrab\nprint(parseNumber(input()))\n\n42\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("Name: Crab\n()\n"),
        "{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("42\n()\n"));
}

#[test]
fn utf8_file_reads_preserve_text_and_split_lines_correctly() {
    let data = SourceFile::new("?\r\n\r\nlast\n");
    let path = data.0.to_string_lossy();
    let source =
        format!("var reader = readFile print(reader('{path}')) print(readLines('{path}'))");
    let output = file(&source);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "?\n\nlast\n\n[\"?\", \"\", \"last\"]\n");
    fs::write(&data.0, "").unwrap();
    assert_output(&format!("print(readLines('{path}'))"), "[]\n");
    fs::write(&data.0, [0xff, 0xfe]).unwrap();
    assert_eq!(file(&source).status.code(), Some(70));
    fs::remove_file(&data.0).unwrap();
    let output = file(&source);
    assert_eq!(output.status.code(), Some(70));
    assert!(stderr(&output).contains("Cannot read"));
}

#[test]
fn input_and_file_examples_work() {
    let program = SourceFile::new(include_str!("../../example/terminal_input.crab"));
    let output = run(&[program.0.as_os_str()], "Crab\nwrong\n25\nyes\ntrue\n");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "Name: Age: Enter a signed decimal integer.\nAge: Verbose (true/false): Enter true or false.\nVerbose (true/false): Hello, Crab\n[name: \"Crab\", age: 25]\n");
    let data = SourceFile::new(include_str!("../../example/data/numbers.txt"));
    let source = include_str!("../../example/file_statistics.crab")
        .replace("example/data/numbers.txt", &data.0.to_string_lossy());
    assert_output(&source, "Count: 4\nTotal: 1000000000000000000000016\nSorted: [-3, 7, 12, 1000000000000000000000000]\n");
}
