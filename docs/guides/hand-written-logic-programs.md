# Hand-written programs for the nine logic tasks

How long must a program be to compute a logic function with the stock instruction set? These nine programs, one for each task of the stock environment, are the shortest known to the authors that do not depend on the initial contents of the registers and stacks. They come from the supplementary material of the 2003 Nature paper "The evolutionary origin of complex features". They are tools for thought: they show how the logic tasks can be built from `nand`, and how much longer the later tasks are, which is part of why evolution finds them later.

None of these programs can copy itself. Each one reads its inputs with `IO`, computes, and outputs the answer with `IO`. Nobody has proved that they are the shortest possible, and the old wiki said so too.

All nine were run in this port's analyze mode with the stock instruction set (`instset-heads.cfg`) and the stock environment (`environment.cfg`). Each of them, executed from the start, was credited with exactly its own task on the first pass through the program. The recipe is at the end of the page.

## The programs

A program is a list of instructions, one per line, in the format of an [organism file](../reference/instruction-sets.md). The *nop* lines are modifiers: `nop-C` after `pop` makes it fill register CX instead of BX, and a `nop-A` or `nop-C` after `nand` or `swap` changes which register is used. `X` and `Y` are the first and the second input that `IO` delivers. Comments after `#` were added for this page, following the register tables of the old page.

### Echo

Echo outputs its input. It is not one of the nine tasks of the stock environment, so it earns nothing, and it is here only as the smallest possible program with an output.

```text
IO     # BX = X
IO     # output X, BX = Y
```

### NOT (5 instructions and 1 modifier)

```text
IO     # BX = X
push   # stack: X
pop    # CX = X
nop-C
nand   # BX = X nand X = not X
IO     # output not X
```

### NAND (4 instructions and 1 modifier)

The first `IO` outputs the initial content of BX, which is 0 in the test CPU, and that matches no task.

```text
IO     # BX = X
IO     # BX still X; CX = Y because of the nop-C (the output is the old CX)
nop-C
nand   # BX = X nand Y
IO     # output X nand Y
```

### ORN, X or (not Y) (5 instructions and 1 modifier)

```text
IO
IO     # CX = Y
nop-C
nand   # BX = X nand Y
nand   # BX = (X nand Y) nand Y = X or (not Y)
IO     # output
```

### AND (7 instructions and 2 modifiers)

```text
IO
IO     # CX = Y
nop-C
nand   # BX = X nand Y
push   # stack: X nand Y
pop    # CX = X nand Y
nop-C
nand   # BX = (X nand Y) nand (X nand Y) = X and Y
IO     # output
```

### OR (11 instructions and 4 modifiers)

```text
IO     # BX = X
push
pop    # CX = X
nop-C
nand   # AX = not X, the result goes to AX because of the nop-A
nop-A
IO     # outputs X (no task for it), BX = Y
push
pop    # CX = Y
nop-C
nand   # BX = not Y
swap   # swaps CX and AX: CX = not X, AX = Y
nop-C
nand   # BX = (not Y) nand (not X) = X or Y
IO     # output
```

### ANDN, (not X) and Y (8 instructions and 2 modifiers)

```text
IO
IO     # CX = Y
nop-C
nand   # BX = X nand Y
nand   # BX = X or (not Y)
push
pop    # CX = X or (not Y)
nop-C
nand   # BX = not (X or (not Y)) = (not X) and Y
IO     # output
```

The stock task `andn` is defined over ordered pairs of inputs, so this program, which gives (not X) and Y, performs it with the inputs taken in the other order.

### NOR (14 instructions and 5 modifiers)

```text
IO
push
pop    # CX = X
nop-C
nand   # AX = not X
nop-A
IO     # outputs X, BX = Y
push
pop    # CX = Y
nop-C
nand   # BX = not Y
swap   # CX = not X, AX = Y
nop-C
nand   # BX = X or Y
push
pop    # CX = X or Y
nop-C
nand   # BX = not (X or Y)
IO     # output
```

### XOR (12 instructions and 3 modifiers)

```text
IO     # BX = X
IO     # CX = Y
nop-C
push   # stack: X
nand   # BX = X nand Y
swap   # swaps BX and CX: BX = Y, CX = X nand Y
nand   # BX = Y nand (X nand Y) = X or (not Y)
swap   # swaps AX and BX (the nop-A names AX): AX = X or (not Y)
nop-A
pop    # BX = X, taken from the stack
nand   # BX = X nand (X nand Y) = Y or (not X)
swap   # swaps CX and AX (the nop-C): CX = X or (not Y), AX = X nand Y
nop-C
nand   # BX = (Y or (not X)) nand (X or (not Y)) = X xor Y
IO     # output
```

### EQU (15 instructions and 4 modifiers)

EQU is the XOR program followed by one more negation, done with the same push, pop and nand as in NOR. The comments of the first 14 lines are those of XOR.

```text
IO
IO
nop-C
push
nand
swap
nand
swap
nop-A
pop
nand
swap
nop-C
nand   # BX = X xor Y
push
pop    # CX = X xor Y
nop-C
nand   # BX = not (X xor Y) = X equ Y
IO     # output
```

## Why the lengths grow

Counting every line, the programs have 2 (echo), 5 (NAND), 6 (NOT and ORN), 9 (AND), 10 (ANDN), 15 (OR and XOR) and 19 lines (NOR and EQU). The shortest are the ones that are one or two uses of `nand` away from the inputs. The longer ones need the inputs held apart in registers and negated one at a time, because `nand` is the only logic instruction. The stock environment rewards the tasks with `type=pow` and values 1 to 5: NOT and NAND multiply merit by 2, AND and ORN by 4, OR and ANDN by 8, NOR and XOR by 16, and EQU by 32. The order of the lengths follows the order of the rewards only roughly: XOR, at 15 lines, is shorter than NOR, at 19.

## Running a program yourself

1. Copy the stock configuration folder: `cp -r avida-core/support/config logic && cd logic`.
2. Save a program in a file, for example the NOT program as `not.org`, one instruction per line.
3. Write an analyze script `trace.cfg` that loads the file and traces it:

   ```text
   PURGE_BATCH
   LOAD_ORGANISM not.org
   SET_BATCH 0
   TRACE
   ```

4. Run `avida -a -set ANALYZE_FILE trace.cfg`. The program writes `data/archive/not.org.trace`, one block of registers, stacks and counters for each step.
5. List the distinct task counts that appear, in order:

   ```sh
   grep "Task Count" data/archive/not.org.trace | uniq
   ```

   The first line is all zeros. The second shows the first task credited, in the order NOT, NAND, AND, ORN, OR, ANDN, NOR, XOR, EQU: for the NOT program, `1 (1)` in the first position and zeros in the other eight.

For the Echo program the second line does not exist, because nothing is credited.

The trace keeps going for 100 or more steps, because a program that does not divide goes round again from its first instruction. The registers and stacks then keep their old contents, so a later pass can earn a second task. For example, the traces of AND, ANDN, XOR and EQU end with a second task credited (NAND, ORN, ORN and XOR respectively) after their first pass. This is an effect of repeating the program with leftovers in the registers, and not a flaw of the programs.

A plain `RECALCULATE` of these genomes shows no tasks at all, because a genome that does not divide is not viable and its tasks are not recorded. Use `TRACE`, as above.

## Where the old page differs

The old wiki page (`Hand-written-Logic-Nine-Programs`) was a copy of an HTML table of register contents for each step, written for the original instruction names (`IO`, `push`, `pop`, `nand`, `swap`, `nop-A` and `nop-C`). The names are unchanged in the port, so the programs work as they are. The table is not reproduced; the comments above replace it.

## See also

- [Environment, tasks and resources](../concepts/environment-and-tasks.md) for how the nine tasks are paid.
- [Virtual CPUs and instruction sets](../concepts/hardware.md#the-default-instruction-set) for the instruction table.
- [An adder from NAND](tutorial-adder-from-nand.md), which asks evolution to find something like these programs.
