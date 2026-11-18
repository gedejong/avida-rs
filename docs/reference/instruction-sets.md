# Instruction sets and organism files

An *instruction set* is the list of instructions that organisms of an experiment can use, with an optional cost, failure probability and frequency for each. An *organism file* (`.org`) holds one genome as a list of instructions. This page gives the formats of both. For what the instructions of the default set do, see [virtual CPUs and instruction sets](../concepts/hardware.md).

## Naming the instruction sets of an experiment

The instruction sets are the section `instruction_sets` of the experiment file (`avida.yaml`). The section is a list; an entry is the name of a stock set or a set written out in full. The stock folder's file names one stock set:

```yaml
instruction_sets: [heads_default]
```

The stock sets are `heads_default`, `heads_sex`, `heads_modular`, `smt`, `transsmt` and `experimental` (see the table below). The first set of the list is the default set of the run. A list of several sets lets organisms with different instruction sets live in one run.

The section can also be a file of its own, which several experiments share:

```yaml
instruction_sets: instset-heads-sex.yaml
```

The command line replaces the section with a stock set or a file, for example `avida -set instruction_sets heads_sex`.

## Writing a set

```yaml
instruction_sets:
  - name: my_heads
    hardware_type: 0
    stack_size: 10          # optional
    uops_per_cycle: 20      # optional
    instructions:
      - nop-A
      - {name: nand, redundancy: 2, cost: 2}
      - h-copy
```

- `name` and `hardware_type` are required (see the [hardware types](../concepts/hardware.md#hardware-types)). The optional `stack_size` defaults to 10 and `uops_per_cycle` to 20 (the latter for the hardware types whose CPU cycle runs several micro-operations).
- Each entry of `instructions` adds one instruction: its name, or a mapping with its `name` and the options below. The instruction must exist in the instruction library of that hardware type. The order of the entries is the order of the letters in saved genomes: `a`, `b`, `c`, and so on, then capitals and digits.
- Comments start with `#`.

The stock set `heads_default` is written out in `avida-core/support/config/instset-heads.yaml`; its 26 instructions are `nop-A`, `nop-B`, `nop-C`, `if-n-equ`, `if-less`, `if-label`, `mov-head`, `jmp-head`, `get-head`, `set-flow`, `shift-r`, `shift-l`, `inc`, `dec`, `push`, `pop`, `swap-stk`, `swap`, `add`, `sub`, `nand`, `h-copy`, `h-alloc`, `h-divide`, `IO` and `h-search`.

### Options of an instruction

An instruction written as a mapping takes these keys besides `name`:

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

Example: make `nand` twice as common and twice as expensive by writing it as

```yaml
      - {name: nand, redundancy: 2, cost: 2}
```

in a copy of the stock set. A short run with this entry starts and finishes normally.

## Instruction sets in the stock folder

| File | Set name, hardware type | Notes |
|---|---|---|
| `instset-heads.yaml` | `heads_default`, 0 | The default. |
| `instset-heads-sex.yaml` | `heads_sex`, 0 | Same as the default, but `divide-sex` replaces `h-divide`. |
| `instset-heads-modular.yaml` | `heads_modular`, 0 | The default plus `nop-D` to `nop-F`, `call` and `return` (an extension of the port). |
| `instset-transsmt.yaml` | `transsmt`, 2 | For parasites. |
| `instset-experimental.yaml` | `experimental`, 3 | Avatars, predator and prey. |
| `instset-smt.yaml` | `smt`, 1 | Hardware type 1 is not available. |

Each of these sets can be named in an experiment file without its file. More sets are in `avida-core/support/config/misc`: `instset-heads-30-parasite.yaml`, `instset-no-heads-*.yaml` and others; name one by its file. The old C++ folder also had `instset-gx.cfg`, a set for the gene-expression hardware, which neither the C++ binary of the tag `cpp-final` nor the port has.

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

This is the usual error when you switch the instruction set (`-set instruction_sets heads_sex`) but still inject the stock ancestor. Use an ancestor written for the new set (`default-heads-sex.org` for `heads_sex`).

The ancestors in the stock folder: `default-heads.org`, `default-heads-sex.org`, `default-heads-modular.org`, `default-transsmt.org`, `default-smt.org` and `experimental.org`.

### Writing your own ancestor

1. Copy `default-heads.org`.
2. Edit the instructions. Keep the copy loop intact: `h-alloc`, `h-search`, the `mov-head` that places the write head, the loop (`h-search`, `h-copy`, `if-label`, `h-divide`, `mov-head`) and the closing `nop-A nop-B` label.
3. Inject it from the events: `- {at: begin, action: Inject, args: my-ancestor.org}`.
4. Test it with a short run and look at `count.dat`: if the number of organisms stays at 1, the organism cannot reproduce.

The [analyze-mode](analyze-mode.md) command `LOAD_ORGANISM` and the `TRACE` command run an organism in the test CPU and show how it executes, which helps to debug an ancestor.
