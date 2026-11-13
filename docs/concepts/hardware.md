# Virtual CPUs and instruction sets

Every organism runs on a virtual CPU. The kind of CPU is the *hardware type*, and it decides which instructions exist and how they behave. An experiment picks its hardware type through the instruction set file that it includes.

## Hardware types

| Type | Name | Instruction-set file in the stock configuration | Use |
|---|---|---|---|
| 0 | Heads CPU (original) | `instset-heads.cfg`, `instset-heads-sex.cfg` | The default. Three registers, two stacks and four "heads" that point into the genome. Most Avida research uses it. |
| 2 | TransSMT | `instset-transsmt.cfg` | A memory-space based CPU with better thread support; used for the parasite experiments. |
| 3 | Experimental | `instset-experimental.cfg` | The CPU with avatars, predator and prey, sensors and messaging. |
| 4 | GP8 | none in the stock folder | A multi-threaded behavioural CPU (the setting description's words). |
| 5 | BCR | none in the stock folder | Another behavioural CPU, with its own instruction library. |

Hardware type 1 (the "new SMT" CPU) is not available: `instset-smt.cfg` and `default-smt.org` are still in the stock configuration folder, but the port reports an unsupported hardware type for an instruction set that asks for type 1.

The five types 0, 2, 3, 4 and 5 run, and the port's tests compare them with the C++ program. An instruction set that names an instruction whose handler is not ported stops at start-up with an `unsupported:` message. Parallel execution only speeds up the heads CPU; see [execution modes](../execution/index.md).

## The heads CPU

The heads CPU is the CPU of the stock experiment. It has:

- **Three registers**, named AX, BX and CX, each holding a 32-bit signed integer.
- **Two stacks** of 10 numbers each. One is active at a time; `swap-stk` switches.
- **Four heads**, which are pointers into the genome: the instruction pointer (IP), a read head, a write head and a flow head.
- **Memory** for the genome and a second memory area for the offspring that is being built.

### No-operation instructions as modifiers

The three instructions `nop-A`, `nop-B` and `nop-C` do nothing on their own, but they change the instruction before them. An instruction that works on a register uses a default register (usually BX), but if it is followed by `nop-A`, `nop-B` or `nop-C`, it uses AX, BX or CX instead. The same nops select a head for the head instructions. The notation `?BX?` in an instruction's description means "BX unless a following nop says otherwise". Instructions that work on two registers use the chosen register and its *complement*, which follows the same cycle as labels below (BX pairs with CX, CX with AX, AX with BX); that is why `if-n-equ` is described as comparing ?BX? with ?CX?. A nop that the preceding instruction consumed this way is skipped when it comes up for execution.

### Labels and templates

A run of nops after certain instructions forms a *label*. `h-search` looks forward in the genome for the *complement* of its label (`nop-A` pairs with `nop-B`, `nop-B` with `nop-C`, `nop-C` with `nop-A`) and puts the flow head there. That is how a genome finds the start of its copy loop without any numeric addresses, and why the ancestor ends with `nop-A nop-B`: the complement of the label `nop-C nop-A` at its start.

### The default instruction set

The 26 instructions of `instset-heads.cfg`, in their order (which is also the order of letters in saved genomes):

| Letter | Instruction | What it does |
|---|---|---|
| a, b, c | `nop-A`, `nop-B`, `nop-C` | No operation; modify the instruction before them. |
| d | `if-n-equ` | Execute the next instruction if ?BX? is not equal to ?CX?, else skip it. |
| e | `if-less` | Execute the next instruction if ?BX? is less than ?CX?, else skip it. |
| f | `if-label` | Execute the next instruction if the organism has just copied the complement of the attached label. |
| g | `mov-head` | Move the head ?IP? to the flow head. |
| h | `jmp-head` | Move the head ?IP? by the amount in CX; CX receives the old position. |
| i | `get-head` | Copy the position of the head ?IP? into CX. |
| j | `set-flow` | Set the flow head to the position in ?CX?. |
| k | `shift-r` | Shift the bits in ?BX? right by one (divide by two). |
| l | `shift-l` | Shift the bits in ?BX? left by one (multiply by two). |
| m | `inc` | Increment ?BX? by one. |
| n | `dec` | Decrement ?BX? by one. |
| o | `push` | Copy the number in ?BX? onto the active stack. |
| p | `pop` | Remove the top number from the active stack and place it in ?BX?. |
| q | `swap-stk` | Toggle which stack is active. |
| r | `swap` | Swap the contents of ?BX? with ?CX?. |
| s | `add` | Add CX to BX and place the result in ?BX?. |
| t | `sub` | Subtract CX from BX and place the result in ?BX?. |
| u | `nand` | Compute the bitwise NAND of BX and CX and place the result in ?BX?. |
| v | `h-copy` | Copy one instruction from the read head to the write head and advance both. This is where copy mutations happen. |
| w | `h-alloc` | Allocate the maximum allowed space for the offspring. |
| x | `h-divide` | Divide the code between the read head and the write head: the offspring is cut off and placed in the world. |
| y | `IO` | Output the number in ?BX? (which the environment checks against the tasks), then read a new input number into ?BX?. |
| z | `h-search` | Find the complement of the following label and put the flow head there. |

These descriptions are the instruction library's own, lightly expanded. The library for the heads CPU defines 563 instruction names in all (conditionals, flow control, stack and register variants, arithmetic, sensing, movement, messaging, donation and many more). Only those listed in an instruction set file are available to an organism. The `instset-*.cfg` files in `avida-core/support/config/misc` show other sets, such as sets with parasites or without nops.

### How logic tasks arise

The only way for an organism to put a number into the world is `IO`. To compute NOT of an input, an organism reads an input number into a register with `IO`, applies `nand` to the register and itself, and outputs it with `IO`. Everything the environment rewards is judged by these outputs. See [environment, tasks and resources](environment-and-tasks.md). Programs of 5 to 19 lines that compute each of the nine stock logic tasks by hand are in [hand-written programs for the nine logic tasks](../guides/hand-written-logic-programs.md).

## Choosing an instruction set

An instruction set is a block of the configuration. The stock `avida.cfg` pulls one in with this line:

```text
#include INST_SET=instset-heads.cfg
```

`INST_SET` is a variable name, and you can point it at another file from the command line:

```sh
avida -def INST_SET instset-heads-sex.cfg
```

The organism file names its instruction set at the top (`#inst_set heads_default` and `#hw_type 0` in `default-heads.org`), and the instruction set it names must exist and have the same hardware type as the organism, otherwise the organism cannot be created and the program reports an error. The file format is on the [instruction sets](../reference/instruction-sets.md) reference page.

## Next

[Environment, tasks and resources](environment-and-tasks.md).
