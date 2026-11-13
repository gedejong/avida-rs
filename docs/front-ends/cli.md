# The command-line program: `avida`

`avida` reads the configuration files in the current directory, runs the experiment that the events file describes, and writes data files. It prints one progress line per update on standard output unless you silence it.

## Usage

```sh
avida [options]
```

Run it inside an experiment folder (one that contains `avida.cfg` and the files it names). The options, as the program's own help lists them:

| Option | Meaning |
|---|---|
| `-a`, `-analyze` | Process `analyze.cfg` instead of doing a normal run. See [analyze mode](../reference/analyze-mode.md). |
| `-c FILE`, `-config FILE` | Use `FILE` as the configuration file instead of `avida.cfg`. |
| `-def NAME VALUE` | Define a variable for the configuration file's `#include` lines. Used to pick an instruction set, for example `-def INST_SET instset-heads-sex.cfg`. |
| `-e`, `-actions` | Print the list of all known actions with their arguments, and exit. See the [actions reference](../reference/actions.md). |
| `-h`, `-help` | Print the option list. |
| `-i`, `-interactive` | Run analyze mode interactively, reading commands from standard input. |
| `-l FILE`, `-load FILE` | Load a clone file. |
| `-r`, `-review` | Review the settings: print those that differ from their defaults, then exit. |
| `-s VALUE`, `-seed VALUE` | Set the random seed. The same seed on the same configuration and build gives the same run in the exact [execution mode](../execution/index.md). |
| `-set NAME VALUE` | Override a setting of `avida.cfg`. May be repeated. |
| `-v`, `-version` | Print the version number. |
| `-v0` to `-v4` | Set the output verbosity from 0 (least) to 4 (most). `-v0` drops the per-update progress lines. |
| `-w`, `-warn` | Warn whenever a default setting is used because the configuration file does not give it. |
| `--generate-config` | Write a default `avida.cfg` into the current directory. |

The version line the program prints reads `Avida 2.14.0`, the same as the C++ program whose output the port reproduces.

## Examples

A run with seed 7 on a smaller world, writing into `run-7`:

```sh
avida -s 7 -set WORLD_X 30 -set WORLD_Y 30 -set DATA_DIR run-7
```

Check which settings an invocation changes:

```sh
avida -r -set WORLD_X 20
```

You should see a banner and then `Non-Default Settings:` followed by ` WORLD_X 20 (default=60)`.

List the available actions:

```sh
avida -e | less
```

You should see a line for each of the 423 actions, with its arguments.

Run a different event file and instruction set without editing any file:

```sh
avida -s 1 -set EVENT_FILE events-sex.cfg -def INST_SET instset-heads-sex.cfg
```

## Settings that matter on the command line

- `EVENT_FILE` and `ENVIRONMENT_FILE` choose the events and environment files, so one folder can hold several variants of an experiment. The [tutorials](../guides/index.md) use this to run "arms" of an experiment from one folder.
- `DATA_DIR` sets where output goes.
- `RANDOM_SEED` in `avida.cfg` is overridden by `-s`.
- `EXECUTION_MODE 1` switches on the [parallel mode](../execution/index.md).

## Output

On standard output, `avida` prints the banner, the line `Random Seed: N`, and (at the default verbosity) one `UD:` line per update. Errors go to standard error. A missing configuration file reads `error: configuration file 'avida.cfg' not found`.

Data files go to `DATA_DIR`. See [output files](../getting-started/output-files.md).

## Environment variables

| Variable | Effect |
|---|---|
| `AVIDA_EXECUTION_STATS=1` | In the parallel mode, prints one line of scheduling statistics to standard error at the end of the run. Not a C++ output. See [execution modes](../execution/index.md). |

## Running many seeds

`avida` runs a single population on a single thread in the exact mode. To use all the cores of a machine, run several copies at once, each with its own seed and data directory. The shell loop below runs eight seeds, four at a time:

```sh
for s in 1 2 3 4 5 6 7 8; do
  echo "$s"
done | xargs -P 4 -I{} sh -c 'avida -s {} -set DATA_DIR run-{} > run-{}.log'
```

The [tutorials](../guides/index.md) use a small helper script of this kind. [How to run replicate seeds](../guides/how-to-replicates.md) explains it.
