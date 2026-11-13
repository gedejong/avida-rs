# Instruction sets and organism files

An *instruction set* is the list of instructions that organisms of an experiment can use, with an optional cost, failure probability and frequency for each. An *organism file* (`.org`) holds one genome as a list of instructions. This page gives the formats of both. For what the instructions of the default set do, see [virtual CPUs and instruction sets](../concepts/hardware.md).

## Including an instruction set in `avida.cfg`

Instruction sets are part of the configuration, but they are written in separate files that `avida.cfg` pulls in with a preprocessor line:

```text
#include INST_SET=instset-heads.cfg
```

After `#include` comes a file name, optionally preceded by a variable name and `=`. `INST_SET` is only a name; it lets you swap the file from the command line:

```sh
avida -def INST_SET instset-heads-sex.cfg
```

You can include several sets (for example `#include instset-heads.cfg` and `#include instset-heads-sex.cfg`), which lets organisms with different instruction sets live in one run.

## File format

```text
INSTSET name:hw_type=N[:stack_size=N][:uops_per_cycle=N]
INST instruction-name[:option=value...]
INST ...
```

- `INSTSET` starts a set and gives it a name and the hardware type (`hw_type`, required; see the [hardware types](../concepts/hardware.md#hardware-types)). The optional `stack_size` defaults to 10 and `uops_per_cycle` to 20 (the latter for the hardware types whose CPU cycle runs several micro-operations).
- Each `INST` line adds one instruction. The instruction must exist in the instruction library of that hardware type. The order of the lines is the order of the letters in saved genomes: `a`, `b`, `c`, and so on, then capitals and digits.
- A file can hold several `INSTSET` blocks; each `INST` belongs to the nearest `INSTSET` above it. An `INST` before any `INSTSET` is an error.
- Comments start with `#`.

The stock set (`instset-heads.cfg`):

```text
INSTSET heads_default:hw_type=0
INST nop-A
INST nop-B
INST nop-C
INST if-n-equ
INST if-less
INST if-label
INST mov-head
INST jmp-head
INST get-head
INST set-flow
INST shift-r
INST shift-l
INST inc
INST dec
INST push
INST pop
INST swap-stk
INST swap
INST add
INST sub
INST nand
INST h-copy
INST h-alloc
INST h-divide
INST IO
INST h-search
```

### Options of an `INST` line

Options follow the instruction name, separated by colons, as `name=value`:

| Option | Default | Meaning |
|---|---|---|
| `redundancy` | 1 | The frequency of the instruction among random choices. An instruction with redundancy 2 is twice as likely as one with redundancy 1 to be picked by a mutation. Redundancy 0 allows the instruction in injected genomes but never as the result of a mutation. |
| `cost` | 1 | CPU cycles the instruction takes. A costly instruction pauses the thread that executed it, not the whole organism. |
| `initial_cost` | 0 | Extra cost paid only the first time the instruction is executed. Lowers the diversity of instructions in a genome. |
| `energy_cost` | 0 | Energy units the instruction needs; see [the energy model](../concepts/energy.md). |
| `addl_time_cost` | 0 | Extra time added to the organism's age, without extra CPU cycles. |
| `prob_fail` | 0 | Probability that the instruction does nothing when executed but still costs its cycles. |
| `inst_code` | | A string of 1s and 0s that gives the instruction a numeric code, for operations that turn a sequence into a number. |
| `res_cost` | 0 | Cost in units of internal resource. An organism that cannot pay dies. |
| `post_cost` | 0 | CPU cost paid after the first successful execution. |
| `bonus_cost` | 0 | Cost in units of the organism's current bonus. |
| `female_cost`, `choosy_female_cost`, `fem_res_cost` | 0 | Extra costs for females, for choosy females, and in internal resource for females (mating types). |

An unknown option is an error (`unrecognized argument`). `SetRedundancy` and `SetProbFail` change an instruction's redundancy and failure probability during a run, for every organism at once.

Example: make `nand` twice as common and twice as expensive:

```text
INST nand:redundancy=2:cost=2
```

A short run with this line in the stock instruction set starts and finishes normally.

## Instruction sets in the stock folder

| File | `INSTSET` name, hardware type | Notes |
|---|---|---|
| `instset-heads.cfg` | `heads_default`, 0 | The default. |
| `instset-heads-sex.cfg` | `heads_sex`, 0 | Same as the default, but `divide-sex` replaces `h-divide`. |
| `instset-transsmt.cfg` | `transsmt`, 2 | For parasites. |
| `instset-experimental.cfg` | `experimental`, 3 | Avatars, predator and prey. |
| `instset-gx.cfg` | (old format) | A gene-expression set in a legacy format. |
| `instset-smt.cfg` | `smt`, 1 | Hardware type 1 is not available. |

More sets are in `avida-core/support/config/misc`: `instset-heads-30-parasite.cfg`, `instset-no-heads-*.cfg` and others.

## Organism files

An organism file is one instruction name per line, in genome order. Text after `#` on a line is a comment. The first lines name the instruction set and hardware type:

```text
#inst_set heads_default
#hw_type 0

h-alloc    # Allocate space for child
h-search   # Locate the end of the organism
nop-C
...
```

`#inst_set` must name an instruction set defined in the run, and `#hw_type` must match its hardware type. If it does not exist, `Inject` fails with:

```text
error: invalid instruction set 'heads_default' defined in organism 'default-heads.org'
```

This is the usual error when you switch the instruction set with `-def INST_SET ...` but still inject the stock ancestor. Use an ancestor written for the new set (`default-heads-sex.org` for `instset-heads-sex.cfg`).

The ancestors in the stock folder: `default-heads.org`, `default-heads-sex.org`, `default-transsmt.org`, `default-smt.org`, `default-gx.org` and `experimental.org`.

### Writing your own ancestor

1. Copy `default-heads.org`.
2. Edit the instructions. Keep the copy loop intact: `h-alloc`, `h-search`, the `mov-head` that places the write head, the loop (`h-search`, `h-copy`, `if-label`, `h-divide`, `mov-head`) and the closing `nop-A nop-B` label.
3. Inject it from the events file: `u begin Inject my-ancestor.org`.
4. Test it with a short run and look at `count.dat`: if the number of organisms stays at 1, the organism cannot reproduce.

The [analyze-mode](analyze-mode.md) command `LOAD_ORGANISM` and the `TRACE` command run an organism in the test CPU and show how it executes, which helps to debug an ancestor.
