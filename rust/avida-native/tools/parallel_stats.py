#!/usr/bin/env python3
"""Statistical comparison of EXECUTION_MODE 0 (exact) and 1 (parallel).

The parallel mode is not byte-identical to the exact one, so the check is
statistical: many seeds of the same configuration in each mode, a handful
of evolutionary outcomes per run, medians and a two-sided Mann-Whitney U
test per outcome.

usage:
  parallel_stats.py run CONFIG_DIR OUT_DIR [--bin B] [--seeds 101-116]
                        [--updates 5000] [--jobs 8] [--modes exact,parallel]
  parallel_stats.py compare OUT_DIR

`run` copies CONFIG_DIR (a directory with avida.cfg, the environment,
instruction set and ancestor; its events.cfg is replaced) once per mode and
seed into OUT_DIR/<mode>/<seed>, writes an event file that injects the
ancestor named by the first `Inject` of CONFIG_DIR/events.cfg, prints the
task counts every update and the averages, counts and a population save at
the end, and runs the binary with `-s SEED -set VERBOSITY 0` (plus
`-set EXECUTION_MODE 1 -set EXECUTION_THREADS 1` for the parallel mode; the
parallel mode's output does not depend on its thread count, and single
threaded runs let `--jobs` runs share the machine). Runs that already have
their final population save are skipped.

`compare` reads every finished run and prints, per outcome, the median
(and quartiles) of each mode, the Mann-Whitney U statistic and its
two-sided p value (exact when there are no ties, otherwise the normal
approximation with tie and continuity corrections).

Outcomes, all at the final update unless named otherwise:
  log10_fitness   log10 of the average fitness (average.dat column 4)
  genome_length   average genome length of the living organisms (the save)
  tasks_held      tasks performed by at least 5% of the organisms
                  (tasks.dat counts against count.dat's organism count)
  first_not       first update at which any organism has NOT in its merit
  first_nand      the same for NAND (never: updates + 1)
  generation      average generation (average.dat column 13)
  gestation       average gestation time (average.dat column 3)
  organisms       number of organisms (count.dat column 3)
  insts_per_update  instructions executed in the final update (count.dat
                  column 2)
"""
import math
import os
import re
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODES = {
    'exact': [],
    'parallel': ['-set', 'EXECUTION_MODE', '1', '-set', 'EXECUTION_THREADS', '1'],
}
TASK_SHARE = 0.05


def parse_seeds(text):
    seeds = []
    for part in text.split(','):
        if '-' in part:
            a, b = part.split('-')
            seeds.extend(range(int(a), int(b) + 1))
        else:
            seeds.append(int(part))
    return seeds


def ancestor_of(config_dir):
    for line in open(os.path.join(config_dir, 'events.cfg')):
        m = re.match(r'\s*u\s+begin\s+Inject\s+(\S+)', line)
        if m:
            return m.group(1)
    sys.exit('no "u begin Inject <file>" in ' + os.path.join(config_dir, 'events.cfg'))


def events(ancestor, updates):
    return (
        f'u begin Inject {ancestor}\n'
        'u 0:1:end PrintTasksData\n'
        f'u {updates} PrintAverageData\n'
        f'u {updates} PrintCountData\n'
        f'u {updates} SavePopulation\n'
        f'u {updates} Exit\n'
    )


def final_save(run_dir, updates):
    return os.path.join(run_dir, 'data', f'detail-{updates}.spop')


def one_run(binary, config_dir, out_dir, mode, seed, updates):
    d = os.path.join(out_dir, mode, str(seed))
    if os.path.exists(final_save(d, updates)):
        return f'{mode} {seed}: done before'
    shutil.rmtree(d, ignore_errors=True)
    shutil.copytree(config_dir, d)
    with open(os.path.join(d, 'events_stats.cfg'), 'w') as f:
        f.write(events(ancestor_of(config_dir), updates))
    args = [binary, '-s', str(seed), '-set', 'EVENT_FILE', 'events_stats.cfg',
            '-set', 'VERBOSITY', '0'] + MODES[mode]
    t = time.time()
    p = subprocess.run(args, cwd=d, capture_output=True)
    wall = time.time() - t
    with open(os.path.join(d, 'run.log'), 'wb') as f:
        f.write(p.stdout + p.stderr)
    with open(os.path.join(d, 'wall'), 'w') as f:
        f.write(f'{wall:.2f}\n')
    if p.returncode != 0:
        return f'{mode} {seed}: exit {p.returncode}'
    return f'{mode} {seed}: {wall:.1f} s'


def data_rows(path):
    rows = []
    for line in open(path):
        line = line.strip()
        if line and not line.startswith('#'):
            rows.append(line.split())
    return rows


def outcomes(run_dir, updates):
    avg = data_rows(os.path.join(run_dir, 'data', 'average.dat'))[-1]
    cnt = data_rows(os.path.join(run_dir, 'data', 'count.dat'))[-1]
    tasks = data_rows(os.path.join(run_dir, 'data', 'tasks.dat'))
    organisms = int(cnt[2])
    first = {}
    for col, name in ((1, 'first_not'), (2, 'first_nand')):
        first[name] = next((int(r[0]) for r in tasks if int(r[col]) > 0), updates + 1)
    held = sum(1 for v in tasks[-1][1:] if int(v) >= TASK_SHARE * organisms)
    units = length = 0
    for r in data_rows(final_save(run_dir, updates)):
        units += int(r[4])
        length += int(r[4]) * int(r[6])
    fitness = float(avg[3])
    return {
        'log10_fitness': math.log10(fitness) if fitness > 0 else float('-inf'),
        'genome_length': length / units if units else 0.0,
        'tasks_held': held,
        'first_not': first['first_not'],
        'first_nand': first['first_nand'],
        'generation': float(avg[12]),
        'gestation': float(avg[2]),
        'organisms': organisms,
        'insts_per_update': int(cnt[1]),
    }


def ranks(values):
    order = sorted(range(len(values)), key=lambda i: values[i])
    r = [0.0] * len(values)
    ties = []
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        for k in range(i, j + 1):
            r[order[k]] = (i + j) / 2 + 1
        if j > i:
            ties.append(j - i + 1)
        i = j + 1
    return r, ties


def exact_u_cdf(n1, n2):
    """counts[u] = number of rankings of n1 + n2 distinct values whose U
    statistic for the first sample is u."""
    # f[i][j][u] by recursion on the largest value.
    from functools import lru_cache

    @lru_cache(maxsize=None)
    def f(i, j):
        if i == 0 or j == 0:
            return (1,)
        a = f(i - 1, j)  # largest belongs to sample 1: U grows by j
        b = f(i, j - 1)
        out = [0] * (i * j + 1)
        for u, c in enumerate(a):
            out[u + j] += c
        for u, c in enumerate(b):
            out[u] += c
        return tuple(out)

    return f(n1, n2)


def mann_whitney(x, y):
    """Two-sided Mann-Whitney U test: (U of x, p)."""
    n1, n2 = len(x), len(y)
    r, ties = ranks(list(x) + list(y))
    u1 = sum(r[:n1]) - n1 * (n1 + 1) / 2
    if not ties:
        counts = exact_u_cdf(n1, n2)
        total = sum(counts)
        lo = min(u1, n1 * n2 - u1)
        p = 2 * sum(counts[: int(math.floor(lo)) + 1]) / total
        return u1, min(1.0, p)
    n = n1 + n2
    mu = n1 * n2 / 2
    tie_term = sum(t ** 3 - t for t in ties) / (n * (n - 1))
    sigma = math.sqrt(n1 * n2 / 12 * ((n + 1) - tie_term))
    if sigma == 0:
        return u1, 1.0
    z = (abs(u1 - mu) - 0.5) / sigma
    p = math.erfc(max(z, 0) / math.sqrt(2))
    return u1, min(1.0, p)


def quantiles(v):
    s = sorted(v)

    def q(f):
        k = (len(s) - 1) * f
        a, b = int(math.floor(k)), int(math.ceil(k))
        return s[a] + (s[b] - s[a]) * (k - a)

    return q(0.25), q(0.5), q(0.75)


def compare(out_dir):
    results = {}
    updates = None
    for mode in MODES:
        mdir = os.path.join(out_dir, mode)
        if not os.path.isdir(mdir):
            continue
        for seed in sorted(os.listdir(mdir), key=int):
            d = os.path.join(mdir, seed)
            saves = [f for f in os.listdir(os.path.join(d, 'data'))
                     if re.match(r'detail-\d+\.spop$', f)] if os.path.isdir(os.path.join(d, 'data')) else []
            if not saves:
                continue
            u = int(re.match(r'detail-(\d+)', saves[0]).group(1))
            updates = u
            results.setdefault(mode, {})[int(seed)] = outcomes(d, u)
    if len(results) < 2:
        sys.exit('need finished runs of both modes in ' + out_dir)
    names = list(next(iter(results['exact'].values())).keys())
    ne, np_ = len(results['exact']), len(results['parallel'])
    print(f'{ne} exact and {np_} parallel runs, {updates} updates each')
    print(f'{"outcome":17} {"exact median [IQR]":>28} {"parallel median [IQR]":>28} {"U":>6} {"p":>7}')
    for name in names:
        x = [r[name] for r in results['exact'].values()]
        y = [r[name] for r in results['parallel'].values()]
        qx, qy = quantiles(x), quantiles(y)
        u, p = mann_whitney(x, y)
        fmt = lambda q: f'{q[1]:.4g} [{q[0]:.4g}, {q[2]:.4g}]'
        print(f'{name:17} {fmt(qx):>28} {fmt(qy):>28} {u:6.1f} {p:7.3f}')
    walls = {}
    for mode in results:
        w = []
        for seed in results[mode]:
            f = os.path.join(out_dir, mode, str(seed), 'wall')
            if os.path.exists(f):
                w.append(float(open(f).read()))
        if w:
            walls[mode] = quantiles(w)[1]
    if walls:
        print('median wall time per run: ' + ', '.join(f'{m} {t:.1f} s' for m, t in walls.items()))


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ('run', 'compare'):
        sys.exit(__doc__)
    if sys.argv[1] == 'compare':
        compare(sys.argv[2])
        return
    config_dir, out_dir = os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[3])
    opts = {'--bin': os.path.join(WS, 'target', 'release', 'avida'), '--seeds': '101-116',
            '--updates': '5000', '--jobs': '8', '--modes': 'exact,parallel'}
    rest = sys.argv[4:]
    while rest:
        if rest[0] not in opts or len(rest) < 2:
            sys.exit(__doc__)
        opts[rest[0]] = rest[1]
        rest = rest[2:]
    binary = os.path.abspath(opts['--bin'])
    updates = int(opts['--updates'])
    jobs = [(m, s) for m in opts['--modes'].split(',') for s in parse_seeds(opts['--seeds'])]
    with ThreadPoolExecutor(int(opts['--jobs'])) as ex:
        for line in ex.map(lambda j: one_run(binary, config_dir, out_dir, j[0], j[1], updates), jobs):
            print(line, flush=True)


if __name__ == '__main__':
    main()
