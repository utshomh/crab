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

## Examples

Each program in [example/](example/) runs independently:

```sh
cargo run -p crabi -- example/hello_world.crab
cargo run -p crabi -- example/two_counter_machine.crab
```

| File | Topic |
| --- | --- |
| [syntax.crab](example/syntax.crab) | Syntax reference (comments only) |
| [hello_world.crab](example/hello_world.crab) | Printing and string concatenation |
| [addition.crab](example/addition.crab) | Arithmetic, assignment values, and grouping |
| [conditions.crab](example/conditions.crab) | Conditions and short-circuit logic |
| [scope.crab](example/scope.crab) | Local variables and shadowing |
| [factorial.crab](example/factorial.crab) | Computing 50! |
| [fibonacci.crab](example/fibonacci.crab) | Computing the twentieth Fibonacci number |
| [big_integers.crab](example/big_integers.crab) | Computing 2^256 |
| [gcd.crab](example/gcd.crab) | Euclid's algorithm |
| [game_of_life.crab](example/game_of_life.crab) | Game of Life on an integer-encoded grid |
| [two_counter_machine.crab](example/two_counter_machine.crab) | Encoded machine interpreter and Turing-completeness construction |

Run Game of Life with `cargo run -p crabi -- example/game_of_life.crab`.
It prints a glider on a 10 by 10 grid for generations 0 through 4. Edit the
size, generation count, and seed condition in the file to try other patterns.
Cells outside the grid are dead; the edges do not wrap.

Change the machine program and starting counters in `two_counter_machine.crab`
to try other computations. With unbounded integers and these machine operations,
Crab can simulate a universal computation model (assuming unlimited memory and
time). See [Dudenhefner's two-counter machine study](https://doi.org/10.4230/LIPIcs.FSCD.2022.16)
for the underlying model.

## Language behavior

- Values are signed arbitrary-precision integers, booleans, and strings. Integers
  have no fixed bit-width limit and do not wrap. Division truncates toward zero;
  division by zero produces a runtime error.
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
- `example/`: syntax reference, standalone basic programs, and machine simulation.
