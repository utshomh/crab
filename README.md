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
print(factorial)
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
| [arrays.crab](example/arrays.crab) | Lists, records, dictionaries, and built-ins |
| [insertion_sort.crab](example/insertion_sort.crab) | Sorting with indexed mutation |
| [prime_sieve.crab](example/prime_sieve.crab) | Sieve of Eratosthenes |
| [frequency_count.crab](example/frequency_count.crab) | Word frequencies with a dictionary |
| [game_of_life_arrays.crab](example/game_of_life_arrays.crab) | Game of Life using boolean grids |
| [pathfinding.crab](example/pathfinding.crab) | Breadth-first search and path reconstruction |
| [functions.crab](example/functions.crab) | First-class functions, recursion, closures, and built-in aliases |
| [merge_sort.crab](example/merge_sort.crab) | Recursive merge sort |
| [functional_pipeline.crab](example/functional_pipeline.crab) | Typed callbacks, filtering, aggregation, and sorting records |
| [closure_accounts.crab](example/closure_accounts.crab) | Record-held functions sharing private mutable state |
| [game_of_life_functions.crab](example/game_of_life_functions.crab) | Modular Game of Life with callable record fields |
| [terminal_input.crab](example/terminal_input.crab) | Prompted input and recoverable parsing |
| [stdin_sum.crab](example/stdin_sum.crab) | Read integers until standard-input EOF |
| [file_statistics.crab](example/file_statistics.crab) | Read, parse, aggregate, and sort a text file |
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

## Functions and callable built-ins

`print` is now a built-in value, so use `print(value)` in place of the old
`print value` statement. It accepts any number of arguments, separates them with
spaces, and adds a newline. `print()` writes a blank line and returns `Unit`.

```text
fun square(n: Number) -> Number { return n * n }
fun apply(f: fun(Number) -> Number, n: Number) -> Number { return f(n) }
var operations = [square: square, double: fun(n: Number) -> Number { return n * 2 }]
print(apply(operations.square, 5))
var emit = print
emit(operations['double'](7))
```

Functions are first-class values: assign them, pass and return them, and store them
in lists, dictionaries, or records. Function types are `fun(T1, T2) -> R`; record
type annotations use `[field: Type, ...]`. Parameters require types. The return
type defaults to `Unit`; every non-Unit function needs an explicit return on every
branch (the checker conservatively does not assume a loop will run). `return`
exits nested blocks and loops. A newline immediately after `return` ends a bare
Unit return; put returned expressions on the same line as the keyword.

Named `fun name(...)` declarations are hoisted within their lexical scope and
support self/mutual recursion. Anonymous `fun(...)` expressions are evaluated
where they occur; use a named declaration for self-recursion. Captured variables
must be visible at the definition and initialized before a call reads them.
Functions capture lexical variable identities, so later shadowing or redeclaration
does not change their meaning. Captured mutable state survives the defining call.
Copies of a function share its closure identity and captured state; copying a
record containing functions preserves those handles. Record calls do not supply
an implicit `self`. Each for-loop iteration has its own captured loop binding.

Callees evaluate first, then arguments once from left to right. Built-ins use
normal name lookup and can be shadowed, aliased, and stored in records. Their
polymorphic signatures survive aliases; a typed function context specializes a
built-in, e.g. `var size: fun(List<Number>) -> Number = len`. User-defined generic
functions are not yet supported. Function equality compares callable identity.

Closure environments use an arena and transactional snapshots. Failed REPL
submissions roll back captured-state mutations, including callback failures.
Unreachable environments and recursive closure cycles are collected after
successful evaluation; ordinary scopes are released as they exit. Already printed
output remains visible. Runtime errors show function call traces and retain the
source of functions declared in earlier REPL submissions. Calls are limited to
128 nested user-function invocations, with a diagnostic when the limit is reached.

| Built-in | Behavior |
| --- | --- |
| `print(...values)` | Print space-separated values and a newline; return Unit |
| `map(list, f)` | Call `fun(T) -> U` once per element; return List<U> |
| `filter(list, predicate)` | Keep elements for which `fun(T) -> Boolean` returns true |
| `fold(list, initial, f)` | Accumulate left-to-right with `fun(A, T) -> A` |
| `each(list, f)` | Call `fun(T) -> Unit` for every element |
| `sort(list, compare)` | Stable merge sort; `fun(T, T) -> Number` returns negative/zero/positive |
| `str(value)` | Convert a value to its printed String representation |
| `abs(n)` | Absolute value of an arbitrary-precision integer |
| `min(a, b)`, `max(a, b)` | Smaller/larger of two integers |

Higher-order operations evaluate the list and callback once, then iterate a
snapshot. They leave the input collection unchanged; callbacks may change captured
state. Empty maps/filters retain their statically known element type. `sort`
propagates comparator errors and preserves the order of equal elements.

## Input and parsing

```text
var name = input('Name: ')
var age = parseNumber(input('Age: '))
var enabled = parseBoolean(input('Enabled (true/false): '))
var numbers = map(readLines('example/data/numbers.txt'), parseNumber)
```

| Built-in | Behavior |
| --- | --- |
| `input()` / `input(prompt)` | Read one line from stdin, optionally displaying and flushing a String prompt; return String; EOF is an error |
| `readLine()` | Read one line; return `[text: String, eof: Boolean]` |
| `readAll()` | Read remaining stdin through EOF as one String |
| `readFile(path)` | Read an entire UTF-8 file as String |
| `readLines(path)` | Read a UTF-8 file as List<String>, excluding line endings |
| `parseNumber(text)` | Parse a signed arbitrary-precision decimal integer; invalid input errors |
| `parseBoolean(text)` | Parse exactly `true` or `false`; invalid input errors |
| `tryParseNumber(text)` | Return `[ok: Boolean, value: Number]`; failure value is 0 |
| `tryParseBoolean(text)` | Return `[ok: Boolean, value: Boolean]`; failure value is false |
| `trim(text)` | Remove surrounding Unicode whitespace |
| `split(text, separator)` | Split by an exact, nonempty String separator; retain empty parts |

All arguments are Strings, and these built-ins can be aliased, stored in records,
and passed as typed callbacks. Parsers trim surrounding whitespace. Numbers accept
an optional `+` or `-` followed by ASCII decimal digits; decimals, hexadecimal,
underscores, and internal whitespace are rejected. Boolean parsing is case-sensitive
and does not coerce `1`, `0`, or `yes`. Check `.ok` before using a try-parser value;
a valid false boolean or zero integer still has `ok: true`.

Line reads remove LF or CRLF, preserve other whitespace, and distinguish a blank
line (`text: '', eof: false`) from EOF (`text: '', eof: true`). File line reads
preserve blank lines but do not create an extra line for a final newline; an empty
file yields `[]`. Paths are relative to the interpreter's working directory unless
absolute. Missing/unreadable files, invalid UTF-8, and parsing failures produce
runtime diagnostics. In the REPL, finish the code submission with a blank line,
then provide the data requested by its input calls. Consumed input and displayed
prompts are external effects and remain consumed/displayed if a submission fails.

Try `cargo run -p crabi -- example/terminal_input.crab` or
`cargo run -p crabi -- example/file_statistics.crab` from the repository root.

## Collections

```text
var xs = [10, 20, 30]
xs[1] = 99
var grid = [[false; 10]; 10]
grid[2][3] = true
var person = [name: 'Utsho', age: 25]
person.age = 26
var counts: Map<String, Number> = [:]
counts['crab'] = get(counts, 'crab', 0) + 1
for i in 1..10 { print(i) }
```

Lists are homogeneous and zero-indexed. Records have fixed field names, with a
separate type for each field; `.name` and `['name']` are equivalent. Record access
requires a literal field name, while dictionaries allow calculated keys. Dictionary
keys are either all integers or all strings, and values share one type. Dot access
also works on string-keyed dictionaries. Keys are never coerced between types.

Optional annotations use `Number`, `Boolean`, `String`, `Range`, `List<T>`, and
`Map<K, V>`, `Unit`, function signatures, and structural record types. A standalone empty list `[]` or dictionary `[:]` needs an annotation;
assignment to an existing variable, nested literals, and built-in value arguments
can supply the expected type.
`[value; count]` evaluates both expressions once and repeats the value. Negative
counts and sizes beyond addressable memory fail with runtime diagnostics.

Collections have value semantics with copy-on-write storage. Copies and repeated
nested rows behave independently; nested writes update the nearest enclosing
variable. Index expressions run once, left-to-right, followed by the assigned
value; mutation uses the root variable's value after these evaluations. A failed
REPL submission discards nested writes as well as ordinary variable changes.

`start..end` is an inclusive, ascending, lazy integer range. Reversed ranges are
empty. `collect` allocates a list; `len(range)` can report arbitrary-precision
lengths without allocating. `for item in list`, `for item in map`, and
`for item in range` evaluate the iterable once. Lists and dictionaries iterate a
snapshot of their values; use `keys(map)` to iterate dictionary keys.

| Built-in | Behavior |
| --- | --- |
| `len(value)` | List/map/record size, Unicode character count for strings, or range length |
| `push(list, value)` | Return a list with one appended element |
| `pop(list)` | Return `[items: remainingList, value: lastElement]`; empty lists error |
| `keys(collection)` | List indices, dictionary keys, or record field names |
| `values(collection)` | Values as a list; record fields must share one type |
| `has(collection, key)` | Whether a key/index exists; invalid list indices return false |
| `get(collection, key, default)` | Existing value or a default of the same type |
| `remove(collection, key)` | Return an updated collection; list indices shift after removal |
| `collect(range)` | Materialize a range into `List<Number>` |

All built-in arguments, including `get` defaults, evaluate eagerly left-to-right.
Collection built-ins return new values without mutating their arguments. Retain
updates with `xs = push(xs, value)`. Removing a record field changes its structural
type, so bind the result to a new variable. `get` and `remove` require literal keys
on records; `has` permits dynamic string keys. Empty or heterogeneous records cannot
produce a typed `values` list.

Dictionary and record iteration/printing preserve insertion order. A duplicate
dictionary key replaces the earlier value in its original position; duplicate
record fields are compile-time errors. Removing a missing dictionary/record key
is a no-op. Ordinary missing-key reads, invalid list reads/writes/removals, and
empty `pop` operations produce runtime diagnostics. Equality is structural:
record/dictionary entry order does not affect equality, while list order does.
Allocation requests use fallible reservations; actual execution still depends
on available memory and time.

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
