# Blueprint examples

Working, commented Blueprint classes that ship with the Blueprints plugin. They live in the plugin's folder of
the virtual file system (`plugins/<Blueprints>/examples/`), read-only like every plugin folder. To try one,
copy its `.class` folder into your project (for example into `Content/` or `src/classes/`), open it in the
Blueprint Editor and press **F7** to compile.

## How to read an example

* Each example is a `<Name>.class` folder: `graph_save.json` (the whole Blueprint: event graph, macros, variables,
  custom events) and, once compiled, `events/`.
* **Comment boxes** inside the graph explain it: a grey-blue box at the top says what the example is about, and
  coloured boxes sit over each group of nodes (they move with the nodes they contain).
* The class description (Blueprint metadata) and every variable have a one-line description in the Variables panel.
* Every example is **compiled and run in CI** (`cargo test -p blueprint_editor_plugin --test examples`); examples with
  a stated result (a variable value after Begin Play) are checked against it, so what the comments promise is true.
* Where an example says "expected", change a literal on a node (or a variable's default), recompile and watch the
  result change.

## Basics and intermediate (`basics/`)


### Getting started (`basics/01_getting_started/`)

| Class | What it shows |
|---|---|
| `HelloWorld` | Begin Play prints a greeting: events, exec wires, print nodes. |
| `PrintingValues` | Print numbers, booleans, formatted text and log levels; data wires into a message. |

### Variables (`basics/02_variables/`)

| Class | What it shows |
|---|---|
| `Variables` | Class variables of four types: literals, read-modify-write, get/set nodes. |

### Math (`basics/03_math/`)

| Class | What it shows |
|---|---|
| `IntegerMath` | Integer arithmetic and bit operations stored into variables. |
| `FloatMath` | Decimal maths: sqrt, power, clamp, lerp, mean, map range, rounding, sine. |

### Logic and comparisons (`basics/04_logic/`)

| Class | What it shows |
|---|---|
| `Comparisons` | Equals, greater/less, between and nearly-equal into boolean variables. |
| `BooleanLogic` | And, Or, Xor, Not and the Select nodes. |

### Flow control (`basics/05_flow/`)

| Class | What it shows |
|---|---|
| `Branching` | Nested Branch nodes pick a label from a temperature. |
| `MultiBranch` | Multi Branch as an else-if chain choosing a letter grade. |
| `Switches` | Switch On Int / String, Range Switch and String Contains Switch, run by a Sequence. |
| `FlowGates` | Do Once, Do N, Flip Flop and Multi Gate driven by a For Loop and a Sequence. |
| `Gate` | A Gate opened by a condition lets only the later loop iterations through. |

### Loops (`basics/06_loops/`)

| Class | What it shows |
|---|---|
| `ForLoop` | For Loop summing 1..10 with a counter variable. |
| `WhileLoop` | While Loop doubling a number until it passes 100. |
| `NestedLoops` | A For Loop inside a For Loop: 3 x 4 = 12 inner passes. |

### Functions (macros) (`basics/07_macros/`)

| Class | What it shows |
|---|---|
| `Macros` | Two reusable macros (Clamp, Repeat) called from Begin Play. |

### Events (`basics/08_events/`)

| Class | What it shows |
|---|---|
| `CustomEvents` | A custom Greet event with name and times parameters, dispatched twice. |
| `Lifecycle` | Begin Play, On Tick (with delta time) and On End Play. |

### Timing (`basics/09_timing/`)

| Class | What it shows |
|---|---|
| `Delays` | Delay nodes sequence a three-stage timeline. |
| `Timers` | A looping engine timer started on Begin Play and handled by On TimerFired. |

### Strings (`basics/10_strings/`)

| Class | What it shows |
|---|---|
| `StringBasics` | Trim, case, length, contains, starts-with, replace, substring and reverse. |
| `StringSplitJoin` | Split text into an array, join it back, count the pieces, parse a number. |
| `StringFormatting` | Number/bool to text, left/right/center justify and repeat. |

### Arrays, maps and sets (`basics/11_collections/`)

| Class | What it shows |
|---|---|
| `Arrays` | Pure array nodes on a split string: length, contains, is-empty, slice, join. |
| `Maps` | A String-to-String inventory map: insert, length, contains, keys, JSON. |
| `Sets` | A set of tags that ignores duplicates. |

### Data types (`basics/12_data_types/`)

| Class | What it shows |
|---|---|
| `Vectors` | Make, dot, measure and add 3D vectors. |
| `StructLikeValues` | Colour blend and rectangle hit test with tuple-based struct values. |
| `EnumStyleStates` | Integer states named with Switch On Int, advanced by a loop and a custom event. |
| `Conversions` | Automatic int-to-float / anything-to-string conversion and explicit parse nodes. |

### Random, time and testing (`basics/13_utilities/`)

| Class | What it shows |
|---|---|
| `Random` | Random and seeded random numbers, verified with assert nodes. |
| `DateTime` | Wall-clock time nodes: year, timestamp, ISO text and second arithmetic. |
| `Assertions` | Assert nodes as self-checks, plus info logging and conditional printing. |

## Advanced (`advanced/`)

Larger graphs that combine these ideas (components, physics, input, gameplay systems) live in `advanced/`.

## Things worth knowing before you start

* **Exec vs data.** White wires run nodes in order; coloured wires carry values. Pure nodes (no white pins) are
  evaluated when their value is needed.
* **Unwired inputs** use the literal typed on the node (stored in the node's properties). Macro call nodes are the
  exception: wire their data inputs.
* **Automatic conversions:** i64 -> f64 and any value -> String. Everything else needs a node.
* **Values, not references:** Set stores into a variable; array and map nodes return a new array/map, so store the
  result back (`numbers = Push(numbers, 5)`).
* **For Loop** has only a Body pin and no index output; keep a counter variable. **While Loop** runs one pass per frame.
* **Delay** and loops with waits suspend the chain in game time without blocking the game.
* Generated files under `events/` (compiled module, Rust export) are rebuilt on every compile; never edit them.
