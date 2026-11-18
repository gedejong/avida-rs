# Tutorial: an adder from NAND

**Question.** Can evolution build addition when the instruction set has no arithmetic? With only `nand` and shifts, addition takes a loop: the sum without carries is the XOR, the carries are the AND shifted left, repeated until no carry is left. There is no partial credit for a nearly right sum in the plain `add` task, so selection has to find the loop by other routes. This is an open-ended experiment, and it may never succeed.

**Arms.**

| Arm | Instruction set | Rewarded |
|---|---|---|
| `nand-only` | no arithmetic | addition |
| `nand-with-ladder` | no arithmetic | addition and the nine logic tasks |
| `with-arithmetic` | stock | addition (the control: one `add` instruction does it) |

## Steps

1. Start from the stock folder.
2. Create `instset-no-arithmetic.yaml`: a copy of `instset-heads.yaml` with the four entries `inc`, `dec`, `add` and `sub` removed. Keep the name `heads_default`, so the stock ancestor still loads:

   ```sh
   grep -vE '^ +- (inc|dec|add|sub) ' instset-heads.yaml > instset-no-arithmetic.yaml
   ```

3. Create `environment-addition.yaml` with the reward for addition (a factor of 64):

   ```yaml
   environment:
     reactions:
       - {name: ADD, task: add, process: {value: 6.0, type: pow}, requisite: {max_count: 1}}
   ```

   For the ladder arm, copy the nine reactions of the stock `environment` section of `avida.yaml` above it in a second file.
4. Run the arms, for example:

   ```sh
   . ./lab.sh
   run_arm nand-only 1 100000 -set instruction_sets instset-no-arithmetic.yaml -set environment environment-addition.yaml
   run_arm with-arithmetic 1 100000 -set environment environment-addition.yaml
   ```

   The control arm needs the environment that rewards `add` as well, so it gets `environment-addition.yaml` too.

5. Watch the `Add` column of `tasks.dat`.

Hand-written programs for the nine logic tasks, built from `nand` with a few `push`, `pop` and `swap` instructions, show what the logic ladder of the second arm looks like in instructions: see [hand-written programs for the nine logic tasks](hand-written-logic-programs.md).

## Pilot observations

In the pilot (2 seeds, 10,000 updates), the control found addition by update 500 and 800. Neither arm without arithmetic found it. The ladder arm reached EQU in both runs (updates 2,400 and 3,000), so XOR and AND, the parts of a half-adder, were present and rewarded from early on. Whether they ever get wired into a carry loop is the open question.

## Partial credit tasks of the port

The Rust port adds tasks that give partial credit for a sum that is right in its low bits: `add_lowbits`, `add_miss` and their 4-bit variants `add_lowbits4` and `add_miss4`, plus `inc_lowbits`, `dec_lowbits` and `arith_miss`. A reaction pair such as the following pays for every right low bit from 4 on and charges for wrong outputs:

```yaml
environment:
  reactions:
    - name: ADDLOW
      task: add_lowbits4
      process: {value: 32.0, type: pow}
      requisite: {max_count: 1}
    - {name: ADDMISS, task: add_miss4, process: {value: -2.0, type: pow}}
    - {name: ADD, task: add, process: {value: 0.0, type: pow}, requisite: {max_count: 1}}
```

The last line pays nothing; it makes `tasks.dat` count the organisms whose sum is right in all 32 bits. The maintainer's experiment folder has further variants, including an instruction set `instset-gates` that adds `and`, `xor` and `if-not-0`, and environments that reward increment and decrement as stepping stones. See [tasks](../reference/tasks.md).

## If a run reports addition

Check how. The dominant genome can be written every 10,000 updates with `PrintDominantGenotype`, and the [line-of-descent tutorial](tutorial-line-of-descent.md) traces its ancestry. Evolution is good at finding loopholes in how a task is judged, and a shortcut that is not a real adder would be a finding of its own.
