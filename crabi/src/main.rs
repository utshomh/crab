mod collections;
mod environment;
mod evaluator;
mod functions;
mod io_builtins;

use crate::{environment::Environment, evaluator::Evaluator};
use colored::Colorize;
use crab::{binding::bindings::Bindings, common::diagnostic::Diagnostic, compilation::Compilation};
use std::{
    cell::RefCell,
    env::args,
    fs::read_to_string,
    io::{self, Write},
    process::ExitCode,
    rc::Rc,
};

fn main() -> ExitCode {
    // Keep the language call-depth guard below the native stack limit on Windows.
    match std::thread::Builder::new()
        .name("crab-runtime".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
    {
        Ok(worker) => worker.join().unwrap_or(ExitCode::from(70)),
        Err(error) => {
            eprintln!("Cannot start interpreter: {error}");
            ExitCode::from(74)
        }
    }
}

fn run() -> ExitCode {
    let args: Vec<String> = args().collect();

    match args.as_slice() {
        [_] => match run_repl() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("I/O error: {error}");
                ExitCode::from(74)
            }
        },
        [_, path] => run_file(path),
        _ => {
            eprintln!("Usage: crabi [file.crab]");
            ExitCode::from(65)
        }
    }
}

fn report_diagnostic(source: &str, diagnostic: &Diagnostic) {
    let source = diagnostic.source.as_deref().unwrap_or(source);
    let position = &diagnostic.position;
    let line = position.get_line(source);
    eprintln!("{}", format!("[error in line: {line}]").yellow());
    eprintln!("{}", format!("Error: {}.", diagnostic.message).red());
    // Spans are UTF-8 byte offsets; display only the first affected source line.
    let start = position.start.min(source.len());
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |index| start + index);
    let text = source[line_start..line_end].trim_end_matches('\r');
    let column = source[line_start..start].chars().count();
    let end = position.end.min(line_end).max(start);
    let width = source[start..end].chars().count().max(1);
    eprintln!("\t{text}");
    eprintln!(
        "\t{}{} --- here",
        " ".repeat(column),
        "^".repeat(width).yellow()
    );
}

fn run_repl() -> io::Result<()> {
    let mut bindings = Rc::new(RefCell::new(Bindings::default()));
    let mut environment = Rc::new(RefCell::new(Environment::default()));
    let mut show_syntax_tree = false;
    let mut show_bound_tree = false;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    loop {
        print!("> ");
        stdout.flush()?;
        let mut source = String::new();
        let mut eof = false;
        loop {
            let mut line = String::new();

            if stdin.read_line(&mut line)? == 0 {
                eof = true;
                break;
            }

            if line.trim().is_empty() {
                break;
            }

            source.push_str(&line);
            print!(".. ");
            stdout.flush()?;
        }

        match source.trim() {
            "@exit" | "@e" => break,
            "@syntax_tree" | "@st" => show_syntax_tree = !show_syntax_tree,
            "@bound_tree" | "@bt" => show_bound_tree = !show_bound_tree,
            "" => {}
            _ => {
                let compilation = Compilation::compile(&source, Rc::clone(&bindings));

                if show_syntax_tree {
                    println!("{:#?}", compilation.unbound_program);
                }

                if show_bound_tree {
                    println!("{:#?}", compilation.bound_program);
                }

                if compilation.diagnostic_bag.borrow().diagnostics.is_empty() {
                    // Discard all variable changes if execution fails. Printed output is immediate.
                    let tentative_environment =
                        Rc::new(RefCell::new(environment.borrow().snapshot()));
                    let mut evaluator = Evaluator::new(
                        compilation.bound_program,
                        tentative_environment,
                        Rc::from(source.as_str()),
                        compilation.bindings.borrow().symbols(),
                    );

                    match evaluator.evaluate() {
                        Ok(value) => {
                            println!("{value}");
                            environment = evaluator.bindings;
                            bindings = compilation.bindings;
                        }

                        Err(diagnostic) => report_diagnostic(&source, &diagnostic),
                    }
                } else {
                    for diagnostic in &compilation.diagnostic_bag.borrow().diagnostics {
                        report_diagnostic(&source, diagnostic);
                    }
                }
            }
        }

        if eof {
            break;
        }
    }

    Ok(())
}

fn run_file(path: &str) -> ExitCode {
    let source = match read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("Cannot read '{path}': {error}");

            return ExitCode::from(74);
        }
    };
    let compilation = Compilation::compile(&source, Rc::new(RefCell::new(Bindings::default())));

    if !compilation.diagnostic_bag.borrow().diagnostics.is_empty() {
        for diagnostic in &compilation.diagnostic_bag.borrow().diagnostics {
            report_diagnostic(&source, diagnostic);
        }

        return ExitCode::from(65);
    }

    let mut evaluator = Evaluator::new(
        compilation.bound_program,
        Rc::new(RefCell::new(Environment::default())),
        Rc::from(source.as_str()),
        compilation.bindings.borrow().symbols(),
    );

    match evaluator.evaluate() {
        Ok(_) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            report_diagnostic(&source, &diagnostic);
            ExitCode::from(70)
        }
    }
}
