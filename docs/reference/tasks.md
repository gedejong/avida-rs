# Task reference

A *task* is a test that an environment applies to an organism's output (see [environment, tasks and resources](../concepts/environment-and-tasks.md)). You attach a task to a reward with a `REACTION` line in the [environment file](environment-file.md):

```text
REACTION  NAME  task_name[:argument=value,argument=value]  process:...  requisite:...
```

The task names below are the ones the port registers. They come from `crates/tasks/src/registry_table.rs` (the fixed names) and `crates/tasks/src/task_lib.rs` (the tasks with arguments). Task names are written in lower case in the file.

## The nine classic logic tasks

`not`, `nand`, `and`, `orn`, `or`, `andn`, `nor`, `xor` and `equ`. Each is true when the output equals the bitwise logic function of the organism's inputs, for any one input (`not`) or any pair of inputs (the others); `orn` is a OR (NOT b) and `andn` is a AND (NOT b), over ordered pairs. Each has a duplicate under the name with `_dup` appended (`not_dup`, `nand_dup`, ...), which lets one environment reward the same function twice, for instance with different requisites.

## Fixed tasks (no arguments)

| Family | Names | What they test |
|---|---|---|
| Echo and arithmetic | `echo`, `echo_dup`, `add`, `add3`, `sub`, `dontcare` | The output equals an input (`echo`), the sum of two inputs (`add`), of three (`add3`), the difference of two (`sub`), or nothing is tested (`dontcare`). |
| Three-input logic | `logic_3AA` to `logic_3CP` (68 names) | The output equals a particular logic function of three inputs. `logic_3AA` is the function (A+B+C == 0). |
| One-input math | `math_1AA` to `math_1AS` (except `math_1AQ` and `math_1AR`, which are not registered) | Arithmetic functions of one input, for example `math_1AA` is 2X and `math_1AS` is 3Y. |
| Two-input math | `math_2AA` to `math_2AZ`, and `math_2AAA` (except `math_2AW`, which is not registered) | Functions of two inputs, for example `math_2AA` is sqrt(X+Y), `math_2AZ` is 4A+6B and `math_2AAA` is 3A-2B. |
| Three-input math | `math_3AA` to `math_3AM` | Functions of three inputs, for example `math_3AA` is X^2+Y^2+Z^2. |
| Fibonacci | `fib_1`, `fib_2`, `fib_4` to `fib_10` | The output equals a Fibonacci number: 0, 1, 2, 3, 5, 8, 13, 21, 34 for `fib_1`, `fib_2`, `fib_4`, `fib_5` and so on. |
| Resource-dependent | `xor-max`, `nand-resourceDependent`, `nor-resourceDependent` | Variants named for a dependence on a resource; see their evaluators in `crates/tasks/src`. |
| Communication | `comm_echo`, `comm_not` | Echo or NOT of a neighbour's input. |
| Movement | `move`, `move_up_gradient`, `move_neutral_gradient`, `move_down_gradient`, `move_not_up_gradient`, `move_to_right_side`, `move_to_left_side`, `movetotarget`, `movetoevent`, `movebetweenevent`, `move_to_event` | The organism moved, or moved in a particular way. Needs the hardware that moves. |
| Events and cost | `event_killed`, `exploded`, `exploded2`, `perfect_strings`, `consume-public-good`, `produce-public-good`, `ai-display-cost` | Special-purpose tasks (an organism exploded, was killed by an event, produced and stored perfect strings, produced or consumed a public good, paid an autoinducer cost). |

The exact function of every `logic_3`, `math_` and `fib_` task is in the task registry; the descriptions printed in `tasks.dat` come from it.

## Tasks added by the Rust port

These do not exist in the C++ program. They change nothing for an environment that does not name them. They were made for the [adder-from-NAND experiment](../guides/tutorial-adder-from-nand.md), where an all-or-nothing `add` gives selection nothing to act on.

| Name | Meaning |
|---|---|
| `add_lowbits` | `add` with partial credit: the quality is the number of low bits in which the output agrees with the sum of two inputs before the first wrong bit, divided by 32. Fewer than 8 right bits count as none. |
| `add_miss` | Scores 1 for an output that `add_lowbits` gives nothing, once two inputs have been read. With a small negative reaction value, wrong outputs cost merit. |
| `add_lowbits4`, `add_miss4` | The same two with a minimum of 4 right low bits instead of 8. |
| `inc_lowbits`, `dec_lowbits` | The increment and the decrement of an input, with the same partial credit (minimum 6 right low bits; only inputs whose lowest bit makes the step carry count). |
| `arith_miss` | The miss task for environments that reward `inc_lowbits`, `dec_lowbits` and `add_lowbits4` together. |

## Tasks with arguments

Arguments are `name=value` pairs separated by commas after the task name and a colon, for example `mult:threshold=100,halflife=50`.

| Task | Arguments (default) | What it tests |
|---|---|---|
| `matchstr` | `string` (required), `partial` (0), `binary` (1), `pow` (2.0) | The outputs match a target bit string. |
| `matchprodstr` | `string` (required), `partial` (0), `binary` (1), `pow` (2.0), `tag` (-1) | A variant of `matchstr` for producing and storing strings. (The registered evaluator is the `matchstr` one.) |
| `match_number` | `target` (required), `threshold` (-1), `halflife` (required) | The output is close to a target number. |
| `sort_inputs` | `size` (required), `direction` (0), `contiguous` (1), `halflife` (required) | The organism sorts its inputs; `direction` below 0 is descending. |
| `fibonacci_seq` | `target` (required), `penalty` (0.0) | The output continues a Fibonacci sequence. |
| `mult`, `div`, `log`, `log2`, `log10`, `sqrt`, `sine`, `cosine` | `threshold` (-1), `halflife` (required) | The output equals the function of the inputs, to within a threshold, with a half-life scale for the quality. |
| `optimize` | many; see the ARCHITECTURE notes | Optimisation of a test function. Arguments on which the reference binary crashes or hangs stop the environment at load with `unsupported: optimize ...`. |
| `sg_path_traversal` | | Registered; relates to state grids (the `GRID` directive). |
| `form-group`, `form-group-id`, `live-on-patch-id` | | Registered; group-related tasks. |
| `collect-odd-cell`, `eat-target`, `eat-target-echo`, `eat-target-nand`, `eat-target-and`, `eat-target-orn`, `eat-target-or`, `eat-target-andn`, `eat-target-nor`, `eat-target-xor`, `eat-target-equ`, `move-ft` | | Registered; target-eating and foraging tasks. |
| `all-ones`, `royal-road`, `royal-road-wd` | | Registered; bit-pattern tasks. |
| `opinion_is` | | Registered; relates to the organism's opinion. |

For the rows marked "Registered", the page lists the name only: read the task's evaluator in `crates/tasks/src` before relying on one.

## Checking which tasks an environment defines

Run `avida` for a few updates with the environment and read the legend of `tasks.dat`: it lists one column per reaction, in file order, with the task's description. The [first run](../getting-started/first-run.md) page shows an example.
