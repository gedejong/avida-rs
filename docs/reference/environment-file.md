# Environment reference (`environment`)

The environment defines the resources, the tasks that organisms can be rewarded for, and what each reward does. It is the section `environment` of the experiment file (`avida.yaml`), or a file of its own that the section names (`environment: environment-adder-ladder.yaml`); the command line can name another file with `-set environment FILE`. For the ideas behind it, see [environment, tasks and resources](../concepts/environment-and-tasks.md).

All rules below were checked against the parser in the `avida-environment` crate (`crates/environment/src/environment.rs`). The program turns each entry into a line of the C++ environment file (`RESOURCE glucose:inflow=100:outflow=0.01`) and reads that line as the C++ did; the C++ directive of each list is named below, because messages and the old documentation use it.

## Structure

`environment` is a mapping with these keys, each a list:

| Key | C++ directive | Purpose |
|---|---|---|
| `resources` | `RESOURCE`, `CELL`, `GRADIENT_RESOURCE` | Declare resources; `kind: cell` gives a resource amounts or flows in chosen cells, `kind: gradient` declares a resource whose distribution moves or has a landscape. |
| `grids` | `GRID` | Define a state grid for the state-grid tasks. |
| `reactions` | `REACTION` | Link a task to a reward. |
| `set_active` | `SET_ACTIVE` | Switch a reaction on or off. |

The lists are read in this order, whatever the order in the file. An unknown key is an error that names the nearest key. The old wiki also described a `MUTATION` directive; neither this program nor the C++ program at the tag `cpp-final` accepts it.

`input_and_mask` holds the mask of the C++ line `INPUT_AND_MASK` (hex with `0x`, such as `0xFF6DB6DB`, or decimal), which is passed on as written.

A value is one word: a number, a name or a list of cells. Comments start with `#`.

## Resources (`RESOURCE`)

```yaml
environment:
  resources:
    - {name: NAME, OPTION: VALUE, ...}
```

Each entry declares one resource. The name `infinite`, or no resource at all in a reaction, means the unlimited built-in resource. The options:

| Option | Default | Meaning |
|---|---|---|
| `inflow` | 0 | Units entering per update. For a global resource, evenly through the update. For a spatial resource, added every update to the cells of the inflow rectangle. |
| `outflow` | 0.0 | Fraction of the resource removed per update. |
| `initial` | 0 | Amount at the start of the run (spread evenly over cells for a spatial resource). |
| `geometry` | global | `global` (one pool for everybody), `grid` (an amount per cell, no flow over the edges) or `torus` (per cell, flow wraps around). |
| `cells` | | A list of cells the resource applies to. |
| `inflowx1`, `inflowx2`, `inflowy1`, `inflowy2` | determined | The rectangle that receives inflow. `inflowx` and `inflowy` are accepted as aliases of `inflowx1` and `inflowy1`. When `inflow` is given without a rectangle, a rectangle is assigned deterministically. |
| `outflowx1`, `outflowx2`, `outflowy1`, `outflowy2` | unset | The rectangle that loses the outflow fraction. |
| `xdiffuse`, `ydiffuse` | 1.0 | How fast the resource diffuses along each axis (0.0 to 1.0). |
| `xgravity`, `ygravity` | 0 | How fast the resource moves in one direction along each axis (-1.0 to 1.0). |
| `deme` | false | The resource belongs to each [deme](../concepts/demes.md). |
| `energy` | false | An energy resource. Needs `energy.enabled: true`, otherwise the environment is rejected. |
| `collectable` | | Whether organisms can collect it into internal bins with instructions. See [internal resources](../concepts/internal-resources.md). |
| `hgt` | | A pool of genome fragments for horizontal gene transfer; needs `hgt.enabled: true`, and no other options except the defaults. |

Examples:

```yaml
environment:
  resources:
    - {name: glucose, initial: 10000}
    - {name: maltose, initial: 10000, inflow: 100, outflow: 0.01}
```

The first is a fixed pool without flows. The second is a chemostat: with nobody consuming, the pool settles at inflow divided by outflow, 100 / 0.01 = 10,000.

A spatial resource that enters at the top row and leaves at the bottom:

```yaml
environment:
  resources:
    - name: lactose
      geometry: grid
      initial: 100000
      inflow: 100
      outflow: 0.1
      inflowx1: 0
      inflowx2: 59
      inflowy1: 0
      inflowy2: 0
      outflowx1: 0
      outflowx2: 59
      outflowy1: 59
      outflowy2: 59
      ygravity: 0.5
```

Non-global resources assume a cell has up to eight neighbours, which may conflict with the world geometry you chose.

## Cell resources (`kind: cell`, the C++ `CELL`)

```yaml
environment:
  resources:
    - {name: NAME, kind: cell, cells: '0..9,15', inflow: X, outflow: X, initial: X}
```

Gives the resource `NAME` its inflow, outflow or initial amount in the listed cells only (`inflow`, `outflow` and `initial` are optional). If the resource does not exist yet, the entry creates it as a grid resource without flows. Quote a cell list that holds a comma.

## Gradient resources (`kind: gradient`, the C++ `GRADIENT_RESOURCE`)

```yaml
environment:
  resources:
    - {name: NAME, kind: gradient, OPTION: VALUE, ...}
```

A gradient resource is a spatial resource laid out as a cone, a halo, hills, walls or nests. Cones and halos can move over time (moving resources need a bounded grid, `world.geometry: 1`). The parser accepts these options; the description comes from the program's old documentation, which has more detail:

| Option | Meaning |
|---|---|
| `habitat` | Type: 0 gradient (standard or halo), 1 hills, 2 barriers, 3 nests (hidden), 4 dens (visible), 5 predatory; above 5, other kinds of food. |
| `peakx`, `peaky` | Initial position of the cone's peak (non-moving cones). |
| `height` | Height of the cone at its peak. A cell's value on the slope is height divided by (distance from centre + 1). |
| `spread` | Radius of the cone in cells. |
| `plateau` | Resource value capping the cone; -1 switches it off. |
| `decay` | Updates until the resource is refreshed after being consumed. |
| `min_x`, `max_x`, `min_y`, `max_y` | The box in which a standard gradient moves. |
| `updatestep` | Updates between changes of movement direction (or regeneration of hills and walls). |
| `move_a_scaler`, `move_speed` | Smoothness and speed of movement. |
| `halo`, `halo_inner_radius`, `halo_width`, `halo_anchor_x`, `halo_anchor_y` | A halo orbits an anchor cell, between the inner radius and the inner radius plus the width. |
| `initial`, `common`, `floor` | Initial plateau value, whether plateau cells are one common pool, and a floor for cone values. |
| `plateau_inflow`, `plateau_outflow`, `cone_inflow`, `cone_outflow`, `gradient_inflow` | Refill and drain rates for the plateau and the cone. |
| `count`, `min_size`, `max_size`, `config`, `resistance` | Number, size range, orientation and movement resistance of hills and walls. |
| `threshold` | Value above which look instructions count a cell as edible. |
| `refuge`, `damage`, `deadly`, `path`, `hammer` | Refuges from predation, damaging, deadly and path resources. |

Example, a halo food source orbiting the centre of a 60 by 60 bounded world (this entry was run for 60 updates as part of the preparation of this page):

```yaml
environment:
  resources:
    - name: food1
      kind: gradient
      height: 10
      spread: 18
      plateau: 1
      decay: 1
      move_a_scaler: 3.8
      updatestep: 20
      halo: 1
      halo_inner_radius: 5
      halo_anchor_x: 30
      halo_anchor_y: 30
      halo_width: 10
      move_speed: 1
```

Deadly, damaging and predatory gradients act on the population; path and den resources next to a habitat still stop the run. See [unsupported features](unsupported.md).

## Reactions (`REACTION`)

```yaml
environment:
  reactions:
    - name: NAME
      task: TASK
      args: {ARGUMENT: VALUE, ...}       # optional
      process: {OPTION: VALUE, ...}      # optional; a list for several
      requisite: {OPTION: VALUE, ...}    # optional; a list for several
      context_requisite: {OPTION: VALUE, ...}
```

- `name` is the reaction's name, used in other reactions' requisites, in `set_active` and in the output files.
- `task` is a task name from the [task list](tasks.md), for example `not` or `equ`. Tasks that take arguments get them in `args`, as a mapping (`{threshold: 100, halflife: 50}`) or as the text the C++ line had after the task's colon (`threshold=100,halflife=50`). The arguments are passed on as written and the task checks them.
- An entry with only a `name` is a pre-declaration, so that an earlier reaction can refer to a later one.
- Redefining a reaction prints a warning.

### `process` options

A reaction can have one or more processes (a mapping, or a list of mappings):

| Option | Default | Meaning |
|---|---|---|
| `resource` | the unlimited resource | Name of the resource to consume. |
| `value` | 1.0 | Bonus per unit of resource consumed. |
| `type` | `add` | `add`, `mult`, `pow`, `lin`, `energy`, `enzyme` or `exp`. See [how rewards combine](../concepts/environment-and-tasks.md#reactions-what-a-task-is-worth). |
| `max` | 1.0 | Maximum amount consumed per completion. |
| `min` | 0.0 | Minimum amount that must be available; otherwise nothing is consumed. |
| `frac` | 1.0 | Maximum fraction of the available resource consumed (values above 1 are clipped to 1). |
| `ksubm` | 0.0 | The half-saturation constant for `type=enzyme`. |
| `product` | none | A resource produced by the reaction. |
| `conversion` | 1.0 | Units of product per unit consumed. |
| `inst` | | An instruction to run when the reaction fires. |
| `random` | 0 | Flags the reaction's resource as a random resource (rarely used). |
| `lethal` | 0.0 | Probability that the reaction kills the organism. |
| `sterilize` | 0 | Sterilise the organism. |
| `deme` | 0.0 | Fraction of the bonus that goes to the organism's deme. |
| `germline` | 0 | The reward changes the germline propensity instead of the bonus. |
| `detect`, `threshold`, `detectionerror` | | A detection resource, its threshold, and the error rate of detection. |
| `string` | | A match string. |
| `depletable` | 1 | Whether the reaction depletes the resource. |
| `phenplastbonus` | default | For plastic phenotypes: `nobonus`, `fracbonus` or `fullbonus`. |
| `internal` | 0 | Consume from the organism's internal resource bins (see [internal resources](../concepts/internal-resources.md)). |

### `requisite` options

A requisite gives conditions under which the reaction applies. If several requisites are given (a list), the reaction applies when at least one of them holds. Within one requisite, all conditions must hold.

| Option | Meaning |
|---|---|
| `reaction: NAME` | The named reaction must have been triggered (a list names several). |
| `noreaction: NAME` | The named reaction must not have been triggered (a list names several). |
| `min_count: N`, `max_count: N` | The organism must have performed this task at least `N` times, or fewer than `N` times. `max_count: 1` therefore means "only the first time". |
| `reaction_min_count: N`, `reaction_max_count: N` | The same for this reaction's own count. |
| `min_tot_count: N`, `max_tot_count: N` | The same for the total over all reactions. |
| `divide_only: N` | 0 (the default): the reaction applies on output (`IO`) only. 1: only when the organism divides. 2: always. |
| `parasite_only: N` | Any value other than 0 restricts the reaction to parasites. |

A reaction with no `requisite` at all applies on output but not at divide. `context_requisite` takes the same options, evaluated against the context of the output.

The counts are the organism's counts in its current life cycle: an offspring starts at zero.

## Switching reactions (`set_active`, the C++ `SET_ACTIVE`)

```yaml
environment:
  set_active:
    - {type: REACTION, name: NAME, active: 0}
```

Switches the reaction on (`active: 1`, the default) or off (`active: 0`). Only reactions can be switched. Events can do the same during a run with `SetReactionValue`, `SetReactionValueMult`, `SetReactionInst`, `SetReactionMaxTaskCount`, `SetReactionMinTaskCount` and `SetReactionTask` (see [actions](actions.md)).

## A small environment with a limited resource

This is the NOT task of the limited-resources environment used in the [limited-resources tutorial](../guides/tutorial-limited-resources.md):

```yaml
environment:
  resources:
    - {name: resNOT, inflow: 100, outflow: 0.01}
  reactions:
    - {name: NOT, task: not, process: {resource: resNOT, value: 1.0, frac: 0.0025, max: 25}}
```

Each time an organism completes NOT, it consumes the fraction 0.0025 of the pool, but at most 25 units, and the bonus it gains is `value` times the amount consumed (the default process type is `add`, so it is added to the bonus). A pool of 10,000 units gives 25 units at once; as the pool shrinks, so does the reward. The pool refills at 100 units per update and drains by 1% per update, so with nobody consuming it settles at 10,000.
