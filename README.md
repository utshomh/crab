# Crab

Crab is a small statically typed language with a Rust interpreter.

```sh
cargo run -p crabi -- path/to/program.crab
cargo run -p crabi
cargo test --workspace
```

In the REPL, end a submission with a blank line. Use `@exit` (`@e`) to exit,
`@syntax_tree` (`@st`) to toggle the syntax tree, and `@bound_tree` (`@bt`) to
toggle the bound tree. EOF executes any pending submission, then exits.

```text
var factorial = 1
for i = 1 to 5 {
    factorial = factorial * i
}
print factorial
```

## Language behavior

- Values are signed 32-bit integers, booleans, and strings. Integer overflow,
  including negating the minimum integer, and division by zero produce runtime errors.
  Division truncates toward zero.
- Strings may be empty, contain Unicode, or span multiple lines. Backslashes are
  literal characters; escape sequences are not currently supported.
- `&&` and `||` short-circuit. Assignment expressions return the assigned value.
- Blocks and every `if`, `else`, and loop body introduce lexical scopes, even
  without braces. Assignments can update enclosing variables. A `var` declaration
  replaces a same-scope binding or shadows an enclosing binding.
- `for i = lower to upper` visits an inclusive ascending integer range. Bounds are
  evaluated once. A reversed range performs no iterations. The iterator is local
  to the loop; assigning to it in the body does not alter the iteration sequence.
- Failed REPL submissions discard changes to bindings and variable values. Output
  already printed remains visible.
- File execution returns status 65 for syntax/type errors, 70 for runtime errors,
  and 74 for I/O errors.

## Layout

- `crab`: lexer, parser, typed bound tree, and diagnostics.
- `crabi`: evaluator and command-line REPL.
- `crab/tests`: frontend regression tests.
- `crabi/tests`: interpreter and CLI regression tests, including termination checks.
