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
2. Create `instset-no-arithmetic.cfg`: a copy of `instset-heads.cfg` with the four lines `INST inc`, `INST dec`, `INST add` and `INST sub` removed. Keep the name `heads_default` on the `INSTSET` line, so the stock ancestor still loads.
3. Create `environment-addition.cfg` with the reward for addition (a factor of 64):

   ```text
   REACTION  ADD  add  process:value=6.0:type=pow  requisite:max_count=1
   ```

   For the ladder arm, put the nine stock reactions above it in the same file.
4. Run the arms, for example:

   ```sh
   . ./lab.sh
   run_arm nand-only 1 100000 -def INST_SET instset-no-arithmetic.cfg -set ENVIRONMENT_FILE environment-addition.cfg
   run_arm with-arithmetic 1 100000
   ```

   For the control arm, the environment must reward `add` as well: use `environment-addition.cfg` there too.

5. Watch the `Add` column of `tasks.dat`.

Hand-written programs for the nine logic tasks, built from `nand` with a few `push`, `pop` and `swap` instructions, show what the logic ladder of the second arm looks like in instructions: see [hand-written programs for the nine logic tasks](hand-written-logic-programs.md).

## Pilot observations

In the pilot (2 seeds, 10,000 updates), the control found addition by update 500 and 800. Neither arm without arithmetic found it. The ladder arm reached EQU in both runs (updates 2,400 and 3,000), so XOR and AND, the parts of a half-adder, were present and rewarded from early on. Whether they ever get wired into a carry loop is the open question.

## Partial credit tasks of the port

The Rust port adds tasks that give partial credit for a sum that is right in its low bits: `add_lowbits`, `add_miss` and their 4-bit variants `add_lowbits4` and `add_miss4`, plus `inc_lowbits`, `dec_lowbits` and `arith_miss`. A reaction pair such as the following pays for every right low bit from 4 on and charges for wrong outputs:

```text
REACTION  ADDLOW  add_lowbits4  process:value=32.0:type=pow  requisite:max_count=1
REACTION  ADDMISS add_miss4     process:value=-2.0:type=pow
REACTION  ADD     add           process:value=0.0:type=pow   requisite:max_count=1
```

The last line pays nothing; it makes `tasks.dat` count the organisms whose sum is right in all 32 bits. The maintainer's experiment folder has further variants, including an instruction set `instset-gates` that adds `and`, `xor` and `if-not-0`, and environments that reward increment and decrement as stepping stones. See [tasks](../reference/tasks.md).

## If a run reports addition

Check how. The dominant genome can be written every 10,000 updates with `PrintDominantGenotype`, and the [line-of-descent tutorial](tutorial-line-of-descent.md) traces its ancestry. Evolution is good at finding loopholes in how a task is judged, and a shortcut that is not a real adder would be a finding of its own.
