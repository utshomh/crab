use std::{cell::RefCell, rc::Rc};

use crate::{
    binding::{binder::Binder, bindings::Bindings, bound_tree::BoundStatement},
    common::diagnostic::DiagnosticBag,
    syntax::{lexer::Lexer, parser::Parser, syntax_tree::Statement},
};

pub struct Compilation {
    pub diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
    pub bindings: Rc<RefCell<Bindings>>,
    pub unbound_program: Vec<Statement>,
    pub bound_program: Vec<BoundStatement>,
}

impl Compilation {
    /// Parse and type-check without mutating the supplied bindings.
    ///
    /// On success, `bindings` contains the tentative declarations. A caller that
    /// executes the program should commit those bindings only after execution
    /// succeeds. On failure, the supplied bindings are returned unchanged.
    pub fn compile(source: &str, bindings: Rc<RefCell<Bindings>>) -> Self {
        let diagnostic_bag = Rc::new(RefCell::new(DiagnosticBag::new()));
        let mut lexer = Lexer::new(source, Rc::clone(&diagnostic_bag));
        let tokens = lexer.lex();
        let mut parser = Parser::new(tokens, Rc::clone(&diagnostic_bag), source);
        let unbound_program = parser.parse();
        // Binding is tentative: callers commit compilation.bindings after execution succeeds.
        let tentative_bindings = Rc::new(RefCell::new(bindings.borrow().snapshot()));
        let bound_program = if diagnostic_bag.borrow().diagnostics.is_empty() {
            let mut binder = Binder::new(
                Rc::clone(&tentative_bindings),
                Rc::clone(&diagnostic_bag),
                Rc::from(source),
            );
            binder.bind(unbound_program.clone())
        } else {
            Vec::new()
        };
        let bindings = if diagnostic_bag.borrow().diagnostics.is_empty() {
            tentative_bindings
        } else {
            bindings
        };
        Self {
            diagnostic_bag: Rc::clone(&diagnostic_bag),
            bindings,
            unbound_program,
            bound_program,
        }
    }
}
