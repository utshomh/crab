use std::{
    fs,
    io::Write,
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
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "interpreter did not terminate: {}",
                String::from_utf8_lossy(&output.stderr)
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
    assert_output("print \"\"", "\n");
    assert_output("print \"a\"", "a\n");
}

#[test]
fn malformed_programs_terminate_with_syntax_errors() {
    for source in [
        "print \"",
        "print \"abc",
        "{ print 1",
        "{{}",
        "print",
        "2147483648",
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
        "print (1 + 2) * 3 print 10 - 3 - 2 print 20 / 2 / 2 print - -1 print +3 print !false",
        "9\n5\n5\n1\n3\ntrue\n",
    );
}

#[test]
fn comparisons_boolean_operators_and_assignment_values() {
    assert_output("var x = 0 var y = 0 print (x = y = 3) print x print y print 2 > 1 print 2 >= 2 print 1 < 2 print 2 <= 2 print 1 == 1 print 1 != 2 print true == !false print true && false || true", "3\n3\n3\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn logical_operators_skip_errors_and_side_effects() {
    assert_output("var x = 0 print false && (1 / 0 == 0) print true || (1 / 0 == 0) print false && ((x = 1) == 1) print true || ((x = 2) == 2) print x print true && ((x = 3) == 3) print false || ((x = 4) == 4) print x", "false\ntrue\nfalse\ntrue\n0\ntrue\ntrue\n4\n");
}

#[test]
fn strings_preserve_unicode_multiline_and_backslashes() {
    assert_output(
        "print \"\u{e9}\u{1f980}\" + \"!\" print \"first\nsecond\" print \"a\\nb\"",
        "\u{e9}\u{1f980}!\nfirst\nsecond\na\\nb\n",
    );
}

#[test]
fn for_ranges_are_inclusive_empty_when_reversed_and_safe_at_limits() {
    assert_output("for i = 1 to 3 print i for i = 3 to 3 print i for i = 3 to 1 print i for i = 2147483647 to 2147483647 print i for i = -2147483648 to -2147483648 print i", "1\n2\n3\n3\n2147483647\n-2147483648\n");
}

#[test]
fn for_bounds_are_evaluated_once_and_counter_assignment_does_not_change_range() {
    assert_output("var lower = 1 var upper = 3 var calls = 0 for i = (calls = calls + lower) to upper { print i upper = 0 i = 100 } print calls print upper", "1\n2\n3\n1\n0\n");
}

#[test]
fn loop_and_branch_scopes_preserve_outer_variables() {
    assert_output("var i = 99 for i = 1 to 2 { for i = 3 to 3 print i print i } print i if true var i = false print i while false var i = false print i if false var x = 1 else var x = true", "3\n1\n3\n2\n99\n99\n99\n");
}

#[test]
fn shadowing_and_outer_assignments_work() {
    assert_output("var x = 1 { var x = x + 1 print x { x = 3 } print x } print x { x = 4 } print x var x = false print x", "2\n3\n1\n4\nfalse\n");
}

#[test]
fn example_programs_still_work() {
    assert_output(
        "var factorial = 1 for i = 1 to 5 factorial = factorial * i print factorial",
        "120\n",
    );
    assert_output("var previous = 0 var current = 1 var n = 10 while n > 1 { var t = current current = previous + current previous = t n = n - 1 } print current", "55\n");
    assert_output(
        "var greet = \"hello\" var name = \"utsho\" print greet + \", \" + name + \"!\"",
        "hello, utsho!\n",
    );
}

#[test]
fn if_else_and_else_if_choose_the_correct_branch() {
    assert_output(
        "if false print 1 else if true print 2 else print 3 if true { print 4 } else { print 5 }",
        "2\n4\n",
    );
}

#[test]
fn numeric_failures_are_runtime_errors_instead_of_panics() {
    for (source, message) in [
        ("print 1 / 0", "Division by zero"),
        ("print 2147483647 + 1", "Integer overflow"),
        ("print -2147483648 - 1", "Integer overflow"),
        ("print 2147483647 * 2", "Integer overflow"),
        ("print -(-2147483648)", "Integer overflow"),
        ("print -2147483648 / -1", "Integer overflow"),
    ] {
        let output = file(source);
        assert_eq!(
            output.status.code(),
            Some(70),
            "{source}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains(message),
            "{source}: {}",
            stderr(&output)
        );
        assert!(!stderr(&output).contains("panicked"));
    }
}

#[test]
fn diagnostics_point_to_the_correct_line_and_unicode_column() {
    let output = file("print \"\u{e9}\"\nprint missing\nprint 1\n");
    assert!(stderr(&output).contains("[error in line: 2]"));
    assert!(stderr(&output).contains("\tprint missing\n\t      ^^^^^^^ --- here"));
    let output = file("print \"\u{e9}\" + 1");
    assert!(stderr(&output).contains("\tprint \"\u{e9}\" + 1\n\t      ^^^^^^^ --- here"));
}

#[test]
fn repl_preserves_newlines_and_executes_the_final_submission_at_eof() {
    let output = run(&[], "var x = 1\nvar y = x\nprint y\n");
    assert!(output.status.success());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    assert!(stdout(&output).contains("1\n()\n"));
    assert!(run(&[], "").status.success());
}

#[test]
fn repl_discards_bindings_from_compile_errors() {
    let output = run(&[], "var x = missing\n\nprint x\n\n@exit\n\n");
    assert!(output.status.success());
    assert!(stderr(&output).contains("Name 'missing' is not defined"));
    assert!(stderr(&output).contains("Name 'x' is not defined"));
    assert!(!stderr(&output).contains("panicked"));
}

#[test]
fn repl_rolls_back_runtime_errors_and_continues() {
    let output = run(&[], "var x = 1\n\nx = 2\nvar y = 3\n{ print 1 / 0 }\n\nprint x\n\nprint y\n\nx = x + 1\nprint x\n\n@exit\n\n");
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
