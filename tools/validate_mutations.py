#!/usr/bin/env python3
"""Checks the mutation code against the configured rates, without the
reference binary.

Copies of one ancestor are placed with empty cells around each, and the run
is stopped after their first divide and before any offspring can have divided
itself. Nothing is overwritten, so the organisms born are the direct,
unselected offspring of the ancestor, all of them. The differences between
those offspring and the ancestor are then counted and compared with what the
settings say: how often, where, and what is written.

  python3 tools/validate_mutations.py BINARY BASE_YAML ANCESTOR_ORG [WORKDIR] [--only WORD]

BASE_YAML is an experiment file whose instruction set has the gate
instructions (lab folder 25-gates-long, arm gates); ANCESTOR_ORG the modular
ancestor. The ancestor's filler is replaced by gate instructions: they
exercise the gate rule of h-copy, and since the 16 gates follow each other in
turn, the place of an insertion or a deletion can be read off.

Mutations of the parent's own memory (parent and point mutations) are judged
on a stretch of filler behind the copy loop that is never executed, so that a
mutation there cannot stop the parent from dividing; a count over all sites
would leave out the parents a mutation has broken.

The stock heads CPU and TransSMT are run with their stock set and ancestor
(avida-core/support/config). The stock experimental set names instructions
that hardware does not have (as in the original), so the experimental
hardware is run only with --experimental DIR, a folder with an avida.yaml
that writes out a set of hardware type 3 and the .org file of an ancestor
(lab folder 23-heads-ex/arms/ex). The plain SMT CPU (type 1) is not ported,
and GP8 and BCR have no stock ancestor; they are not run.

--only WORD runs the checks whose name has WORD. Every check has its own
seeds, so that no two checks see the same draws.
"""
import glob
import math
import os
import re
import string
import subprocess
import sys
import zlib

def option(name):
    """The value of `--name VALUE`, taken out of the arguments."""
    if name not in sys.argv:
        return ""
    i = sys.argv.index(name)
    value = sys.argv[i + 1]
    del sys.argv[i:i + 2]
    return value


ONLY = option("--only")
EXPERIMENTAL = option("--experimental")
args = sys.argv[1:]
BINARY, BASE, ANCESTOR = args[:3]
WORK = args[3] if len(args) > 3 else "/tmp/validate-mutations"
STOCK = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                     "avida-core", "support", "config")
LETTERS = string.ascii_lowercase + string.ascii_uppercase
SEEDS = (11, 12, 13, 14, 15, 16, 17, 18)
SIDE = 100
ALPHA = 0.001
TAIL = 43  # the unexecuted filler of the "tail" ancestor


# ---------------------------------------------------------------- statistics

def chi2_p(observed, expected):
    """Chi-square statistic, degrees of freedom and an approximate p-value
    (Wilson-Hilferty)."""
    stat = sum((o - e) ** 2 / e for o, e in zip(observed, expected) if e > 0)
    df = sum(1 for e in expected if e > 0) - 1
    if df <= 0:
        return stat, df, 1.0
    z = ((stat / df) ** (1 / 3) - (1 - 2 / (9 * df))) / math.sqrt(2 / (9 * df))
    return stat, df, 0.5 * math.erfc(z / math.sqrt(2))


def z_p(count, n, p):
    """Two-sided normal test of a count against a binomial expectation."""
    if n == 0 or p <= 0 or p >= 1:
        return float("nan"), float("nan")
    z = (count - n * p) / math.sqrt(n * p * (1 - p))
    return z, math.erfc(abs(z) / math.sqrt(2))


def binom_pmf(n, p, top):
    """P(0), ..., P(top - 1) and P(top or more) of a binomial."""
    pmf = [math.comb(n, k) * p ** k * (1 - p) ** (n - k) for k in range(top)]
    return pmf + [1 - sum(pmf)]


def poisson_pmf(m, top):
    pmf = [math.exp(-m) * m ** k / math.factorial(k) for k in range(top)]
    return pmf + [1 - sum(pmf)]


def hist(values, top):
    return [sum(1 for v in values if v == k) for k in range(top)] + [sum(1 for v in values if v >= top)]


GOOD = True


def report(name, ok, detail):
    global GOOD
    GOOD &= bool(ok)
    print(f"  {'ok  ' if ok else 'FAIL'} {name}: {detail}", flush=True)


def note(text):
    print(f"       {text}", flush=True)


def rate_test(name, count, n, p):
    z, pv = z_p(count, n, p)
    report(name, pv > ALPHA, f"{count} of {n}, {count / n:.5f} against {p:.5f} expected (z = {z:+.2f})")


def dist_test(name, observed, pmf):
    total = sum(observed)
    stat, df, pv = chi2_p(observed, [total * q for q in pmf])
    report(name, pv > ALPHA, f"{observed} against {[round(total * q) for q in pmf]}, "
                             f"chi2 = {stat:.1f} on {df} df, p = {pv:.3f}")


def uniform_test(name, observed, expected=None):
    expected = expected or [sum(observed) / len(observed)] * len(observed)
    stat, df, pv = chi2_p(observed, expected)
    report(name, pv > ALPHA, f"chi2 = {stat:.1f} on {df} df, p = {pv:.3f}")


# -------------------------------------------------------------------- setups

class Setup:
    """An instruction set (names in the order of the saved sequences'
    letters), the experiment file's part after `instruction_sets:` and an
    ancestor."""

    def __init__(self, name, insts, mid, genome):
        self.name, self.insts, self.mid, self.genome = name, insts, mid, genome
        self.n = len(insts)
        self.anc = "".join(LETTERS[insts.index(g)] for g in genome)
        self.len = len(genome)

    def inst(self, letter):
        return self.insts[LETTERS.index(letter)]


def org_lines(path):
    return [n for n in (l.split("#")[0].strip() for l in open(path)) if n]


def gate_setup(family="gate", layout="front"):
    """The modular ancestor with gates as filler: `front` has all of the
    filler before the copy loop (it is executed), `tail` has TAIL of it
    behind the loop's jump back, where nothing is executed."""
    y = open(BASE).read()
    if family != "gate":
        y = re.sub(r"^(\s+)- gate-", rf"\1- {family}-", y, flags=re.M)
    mid = y.split("\ninstruction_sets:", 1)[1].split("\nevents:")[0]
    insts = re.findall(r"^\s+- (\S+)$", mid.split("instructions:")[1].split("\nenvironment:")[0], re.M)
    names, filler = org_lines(ANCESTOR), []
    for i, name in enumerate(names):
        if name == "nop-C" and names[i - 1] != "mov-head":
            filler.append(i)
    front = filler if layout == "front" else filler[:len(filler) - TAIL]
    genome = [n for i, n in enumerate(names) if i not in filler or i in front]
    if layout == "tail":
        # The jump back keeps a nop-A behind it (it names the head to move).
        genome = genome[:-2] + ["nop-A"] + ["nop-C"] * TAIL + genome[-2:]
    out, k = [], 0
    for i, name in enumerate(genome):
        if name == "nop-C" and genome[i - 1] != "mov-head":
            out.append(f"{family}-{k % 16}")
            k += 1
        else:
            out.append(name)
    return Setup(f"{family} filler" + (", unexecuted tail" if layout == "tail" else ""), insts, mid, out)


def folder_setup(name, folder):
    """The written-out instruction set and the ancestor of a lab folder."""
    y = open(f"{folder}/avida.yaml").read()
    mid = y.split("\ninstruction_sets:", 1)[1].split("\nevents:")[0]
    insts = re.findall(r"^\s+- (\S+)$", mid.split("instructions:")[1].split("\nenvironment:")[0], re.M)
    return Setup(name, insts, mid, org_lines(glob.glob(f"{folder}/*.org")[0]))


def stock_setup(set_name, set_file, org_file):
    text = open(f"{STOCK}/{set_file}").read()
    insts = re.findall(r"^\s+- (\S+)", text.split("instructions:")[1], re.M)
    stock = open(f"{STOCK}/avida.yaml").read()
    env = "\nenvironment:" + stock.split("\nenvironment:", 1)[1].split("\nevents:")[0]
    return Setup(f"stock {set_name}", insts, f"\n  - {set_name}\n" + env, org_lines(f"{STOCK}/{org_file}"))


# ---------------------------------------------------------------------- runs

def rows(path):
    return [l.split() for l in open(path) if l.strip() and l[0].isdigit()]


def run(tag, setup, rates, seed, last, step, extra=""):
    d = f"{WORK}/{tag}-{seed}"
    os.makedirs(d, exist_ok=True)
    open(f"{d}/anc.org", "w").write("\n".join(setup.genome) + "\n")
    head = (
        f"general:\n  random_seed: 1\nworld:\n  x: {SIDE}\n  y: {SIDE}\nmutation:\n"
        + "".join(f"  {k}: {v}\n" for k, v in rates.items()) + extra
    )
    events = (
        "\nevents:\n"
        # Every `step`-th cell of every `step`-th row: each ancestor has
        # empty neighbours for its offspring, so nothing is overwritten.
        + "".join(
            f"  - {{at: begin, action: Inject, args: anc.org {y * SIDE + x}}}\n"
            for y in range(0, SIDE, step) for x in range(0, SIDE, step))
        + "  - {at: 0, every: 1, until: end, action: PrintAverageData}\n"
          "  - {at: 0, every: 1, until: end, action: PrintCountData}\n"
          f"  - {{at: {last}, action: SavePopulation}}\n  - {{at: {last}, action: Exit}}\n"
    )
    open(f"{d}/avida.yaml", "w").write(head + "\ninstruction_sets:" + setup.mid + events)
    r = subprocess.run(
        [BINARY, "-c", "avida.yaml", "-s", str(seed), "-set", "general.verbosity", "0",
         "-set", "general.data_dir", f"{d}/out"],
        cwd=d, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"{tag}: the run failed:\n{r.stdout[-600:]}{r.stderr[-600:]}")
    return d


class Obs:
    pass


def observe(tag, setup, rates, waves=1, later_ok=False, skip_later=False, extra=""):
    """The offspring of the ancestors.

    waves=1: stopped after the ancestors' first divide. `n_off` organisms
    were born, `mutated` are the sequences that differ from the ancestor,
    `births` the number born per update.

    waves=2: stopped in the second round of divides, before a third.
    `everything` has every living sequence that differs from the ancestor,
    whatever its generation.

    `unmutated` is the number of living organisms with the ancestor's
    sequence beyond the ancestors themselves."""
    rates = {"copy_prob": 0.0, "divide_ins_prob": 0.0, "divide_del_prob": 0.0,
             "divide_slip_prob": 0.0, **rates}
    step = 2 if waves == 1 else 3
    n_anc = len(range(0, SIDE, step)) ** 2
    o = Obs()
    o.mutated, o.n_off, o.births, o.n_anc, o.later = [], 0, {}, n_anc * len(SEEDS), 0
    o.everything, o.unmutated = [], 0
    # The timing comes from a run without mutations: with them a broken
    # ancestor can divide early.
    d = run(tag + "-probe", setup, {k: 0.0 for k in rates}, SEEDS[0], 400, step, extra)
    first = next((int(r[0]) for r in rows(f"{d}/out/count.dat") if int(r[0]) > 0 and int(r[8]) > 0), None)
    if first is None:
        sys.exit(f"{tag}: the ancestor does not divide")
    o.first = first
    wave1 = int(1.6 * first)
    last = wave1 if waves == 1 else 3 * first - 2
    for seed in (s + 100 * (zlib.crc32(tag.encode()) % 1000) for s in SEEDS):
        d = run(tag, setup, rates, seed, last, step, extra)
        counts = [r for r in rows(f"{d}/out/count.dat") if 0 < int(r[0]) <= last]
        deaths = sum(int(r[9]) for r in counts)
        # With slips a few shortened offspring divide early and overwrite.
        assert deaths <= (25 if later_ok or skip_later or waves == 2 else 0), \
            f"{tag}: {deaths} organisms were overwritten"
        births = sum(int(r[8]) for r in counts if int(r[0]) <= wave1)
        if later_ok:
            # A much shortened offspring divides within the window: every
            # ancestor has divided once, and later generations are left out.
            assert births >= n_anc, f"{tag}: not every ancestor divided"
            births = n_anc
        if n_anc < births <= n_anc + 25:
            # A few mutated offspring divide within the window; their
            # offspring are left out below.
            births = n_anc
        assert births <= n_anc, f"{tag}: more births than ancestors"
        o.n_off += births
        for r in counts:
            if int(r[0]) <= wave1:
                o.births[int(r[0])] = o.births.get(int(r[0]), 0) + int(r[8])
        if waves == 2:
            assert births == n_anc, f"{tag}: {births} of {n_anc} ancestors divided in time"
        for f in rows(f"{d}/out/detail-{last}.spop"):
            units, gen, seq = int(f[4]), int(f[10]), f[16]
            if units and seq == setup.anc:
                o.unmutated += units - n_anc
            if units == 0 or seq == setup.anc:
                continue
            o.everything += [seq] * units
            if gen > 1:
                o.later += units
                continue
            o.mutated += [seq] * units
    return o


def changes(setup, seqs):
    """(site, letter written) of every substitution in the sequences of the
    ancestor's length."""
    return [(i, s[i]) for s in seqs if len(s) == setup.len
            for i in range(setup.len) if s[i] != setup.anc[i]]


def tail_changes(setup, seqs):
    """The substitutions in the unexecuted filler among the sequences whose
    other sites are the ancestor's, and the number of sequences left out."""
    L, lo, hi = setup.len, setup.len - 2 - TAIL, setup.len - 2
    out, left_out = [], 0
    for s in seqs:
        if len(s) != L or s[:lo] != setup.anc[:lo] or s[hi:] != setup.anc[hi:]:
            left_out += 1
            continue
        out += [(i, s[i]) for i in range(lo, hi) if s[i] != setup.anc[i]]
    return out, left_out


def first_diff(setup, s):
    n = min(len(s), setup.len)
    return next((i for i in range(n) if s[i] != setup.anc[i]), n)


# -------------------------------------------------------------------- checks

def check_copy(setup, mu):
    """copy_prob: a substitution per copied site."""
    L, N, anc = setup.len, setup.n, setup.anc
    o = observe(f"copy-{setup.name}".replace(" ", "_"), setup, {"copy_prob": mu})
    gate_site = [setup.inst(c).startswith("gate-") for c in anc]
    report("offspring sampled", o.n_off > 0.95 * o.n_anc, f"{o.n_off} first offspring of {o.n_anc} ancestors")
    report("no length change", all(len(s) == L for s in o.mutated),
           f"{sum(len(s) != L for s in o.mutated)} of {len(o.mutated)} mutated offspring differ in length")
    subs = changes(setup, o.mutated)
    # A site is seen to change when the mutation writes another instruction.
    p_site = [mu * (1 - (0.25 if g else 1.0) / N) for g in gate_site]
    rate_test("rate per copied site", len(subs), o.n_off * L, sum(p_site) / L)
    rate_test("unmutated offspring", o.n_off - len(o.mutated), o.n_off, math.prod(1 - q for q in p_site))
    per_site = [0] * L
    for i, _ in subs:
        per_site[i] += 1
    uniform_test("every site equally often", per_site, [len(subs) * q / sum(p_site) for q in p_site])
    plain = [(anc[i], c) for i, c in subs if not gate_site[i]]
    obs = [sum(1 for a, c in plain if c == l) for l in LETTERS[:N]]
    exp = [sum(1 for a, c in plain if a != l) / (N - 1) for l in LETTERS[:N]]
    uniform_test(f"ordinary sites ({len(plain)} changes): a random instruction", obs, exp)
    gates = [(setup.inst(anc[i]), setup.inst(c)) for i, c in subs if gate_site[i]]
    if gates:
        near_bit, far = [0] * 4, 0
        for a, b in gates:
            x = int(a.split("-")[1]) ^ int(b.split("-")[1]) if b.startswith("gate-") else 0
            if x in (1, 2, 4, 8):
                near_bit[(1, 2, 4, 8).index(x)] += 1
        rate_test("gate sites: share to a table one entry away", sum(near_bit), len(gates),
                  (0.75 + 0.25 * 4 / N) / (1 - 0.25 / N))
        uniform_test(f"gate sites: each table entry equally often {near_bit}", near_bit)
    elif any(setup.inst(c).startswith("pgate-") for c in anc):
        sites = [(setup.inst(anc[i]), c) for i, c in subs if setup.inst(anc[i]).startswith("pgate-")]
        obs = [sum(1 for a, c in sites if c == l) for l in LETTERS[:N]]
        exp = [sum(1 for a, c in sites if setup.inst(l) != a) / (N - 1) for l in LETTERS[:N]]
        uniform_test(f"plain gate sites ({len(sites)} changes): a random instruction, no gate rule", obs, exp)


def check_indel(setup, kind, prob, positions=True):
    """divide_ins_prob, divide_del_prob: at most one per divide."""
    L, N, anc = setup.len, setup.n, setup.anc
    o = observe(f"divide-{kind}-{setup.name}".replace(" ", "_"), setup, {f"divide_{kind}_prob": prob})
    want = L + 1 if kind == "ins" else L - 1
    hits = [s for s in o.mutated if len(s) == want]
    report("only this length change", len(hits) == len(o.mutated),
           f"{len(o.mutated) - len(hits)} of {len(o.mutated)} mutated offspring are something else")
    rate_test("rate per divide", len(hits), o.n_off, prob)
    pos = [first_diff(setup, s) for s in hits]
    if positions:
        bins, span = 10, want if kind == "ins" else L
        uniform_test("every position equally often", [sum(1 for i in pos if min(i * bins // span, bins - 1) == b)
                                                      for b in range(bins)])
    if kind == "ins":
        written = [s[i] for s, i in zip(hits, pos)]
        uniform_test("a random instruction is inserted", [written.count(l) for l in LETTERS[:N]])


def check_slip(setup, prob):
    L = setup.len
    o = observe("slip", setup, {"divide_slip_prob": prob}, later_ok=True)
    longer = sum(len(s) > L for s in o.mutated)
    shorter = sum(len(s) < L for s in o.mutated)
    # Two positions drawn in 0..L: equal positions change nothing.
    rate_test("rate per divide", len(o.mutated), o.n_off, prob * (1 - 1 / (L + 1)))
    z, p = z_p(longer, longer + shorter, 0.5)
    report("duplications and deletions equally often", p > ALPHA,
           f"{longer} longer, {shorter} shorter, {len(o.mutated) - longer - shorter} of the same length (z = {z:+.2f})")
    mean = sum(abs(len(s) - L) for s in o.mutated) / max(len(o.mutated), 1)
    note(f"mean length change {mean:.1f} instructions (two uniform positions give about {L / 3:.1f})")


def check_divide_sub(setup, prob):
    """divide_prob: at most one substitution per divide."""
    L, N = setup.len, setup.n
    o = observe("divide-sub", setup, {"divide_prob": prob})
    subs = changes(setup, o.mutated)
    report("one substitution and nothing else", len(subs) == len(o.mutated) and all(len(s) == L for s in o.mutated),
           f"{len(subs)} changes in {len(o.mutated)} mutated offspring")
    rate_test("rate per divide", len(o.mutated), o.n_off, prob * (1 - 1 / N))
    per_site = [0] * L
    for i, _ in subs:
        per_site[i] += 1
    uniform_test("every site equally often", per_site)
    obs = [sum(1 for i, c in subs if c == l) for l in LETTERS[:N]]
    exp = [sum(1 for i, c in subs if setup.anc[i] != l) / (N - 1) for l in LETTERS[:N]]
    uniform_test("a random instruction is written", obs, exp)


def check_per_site(setup, kind, q, poisson=False):
    """div_prob, div_ins_prob, div_del_prob (a binomial number per divide)
    and the divide_poisson_*_mean settings (a Poisson number)."""
    L, N = setup.len, setup.n
    key = {"sub": "divide_poisson_mean" if poisson else "div_prob",
           "ins": "divide_poisson_ins_mean" if poisson else "div_ins_prob",
           "del": "divide_poisson_del_mean" if poisson else "div_del_prob"}[kind]
    o = observe(f"{key}", setup, {key: q})
    unmut = o.n_off - len(o.mutated)
    if kind == "sub":
        report("no length change", all(len(s) == L for s in o.mutated), f"{len(o.mutated)} mutated offspring")
        # A site shows a change when it was hit and got another instruction.
        hit = 1 - math.exp(-q / L) if poisson else 1 - (1 - q / L) ** L
        vis = hit * (1 - 1 / N)
        counts = [sum(1 for i in range(L) if s[i] != setup.anc[i]) for s in o.mutated if len(s) == L]
        rate_test("changed sites", sum(counts), o.n_off * L, vis)
        dist_test("changes per offspring (0, 1, 2, 3, 4 or more)", [unmut] + hist(counts, 4)[1:], binom_pmf(L, vis, 4))
        subs = changes(setup, o.mutated)
        per_site = [0] * L
        for i, _ in subs:
            per_site[i] += 1
        uniform_test("every site equally often", per_site)
        obs = [sum(1 for i, c in subs if c == l) for l in LETTERS[:N]]
        exp = [sum(1 for i, c in subs if setup.anc[i] != l) / (N - 1) for l in LETTERS[:N]]
        uniform_test("a random instruction is written", obs, exp)
    else:
        sign = 1 if kind == "ins" else -1
        delta = [sign * (len(s) - L) for s in o.mutated]
        report("only this length change", all(d > 0 for d in delta),
               f"{sum(1 for d in delta if d <= 0)} of {len(delta)} mutated offspring are something else")
        pmf = poisson_pmf(q, 4) if poisson else binom_pmf(L, q, 4)
        dist_test("number per offspring (0, 1, 2, 3, 4 or more)", [unmut] + hist(delta, 4)[1:], pmf)
        one = [s for s in o.mutated if abs(len(s) - L) == 1]
        pos = [first_diff(setup, s) for s in one]
        span = L + 1 if kind == "ins" else L
        uniform_test("every position equally often (offspring with one)",
                     [sum(1 for i in pos if min(i * 10 // span, 9) == b) for b in range(10)])
        if kind == "ins":
            written = [s[i] for s, i in zip(one, pos)]
            uniform_test("a random instruction is inserted", [written.count(l) for l in LETTERS[:N]])


def check_uniform(setup, prob):
    """divide_uniform_prob: one of 2N+1 equally likely mutations (N
    substitutions, N insertions, one deletion)."""
    L, N = setup.len, setup.n
    o = observe("divide-uniform", setup, {"divide_uniform_prob": prob})
    ins = sum(1 for s in o.mutated if len(s) == L + 1)
    dele = sum(1 for s in o.mutated if len(s) == L - 1)
    sub = sum(1 for s in o.mutated if len(s) == L)
    other = len(o.mutated) - ins - dele - sub
    report("at most one mutation", other == 0 and len(changes(setup, o.mutated)) == sub, f"{other} other offspring")
    k = 2 * N + 1
    pmf = [1 - prob + prob / k, prob * (N - 1) / k, prob * N / k, prob / k]
    dist_test("unmutated, substitution, insertion, deletion", [o.n_off - len(o.mutated), sub, ins, dele], pmf)


def filler_counts(setup, seqs, kind):
    """For the sequences that are the ancestor's before and behind the
    unexecuted filler: the number of changes in the filler (substitutions,
    or the length gained or lost), and the number of sequences left out."""
    L, lo = setup.len, setup.len - 2 - TAIL
    counts, left_out = [], 0
    for s in seqs:
        if s[:lo] != setup.anc[:lo] or s[-2:] != setup.anc[-2:] or (kind == "sub" and len(s) != L):
            left_out += 1
            continue
        mid = s[lo:len(s) - 2]
        if kind == "sub":
            counts.append(sum(1 for a, b in zip(mid, setup.anc[lo:L - 2]) if a != b))
        else:
            counts.append((len(mid) - TAIL) * (1 if kind == "ins" else -1))
    return counts, left_out


def check_parent(setup, kind, q):
    """parent_prob, parent_ins_prob, parent_del_prob: the parent's own
    memory at divide, seen in its second offspring.

    Which of the unchanged organisms are second offspring cannot be told,
    so the number of changes in the filler is judged among the offspring
    that have at least one: a wrong rate shifts the shares of 1, 2, 3."""
    L, N = setup.len, setup.n
    key = {"sub": "parent_prob", "ins": "parent_ins_prob", "del": "parent_del_prob"}[kind]
    o = observe(key + "-first", setup, {key: q})
    report("the first offspring is not touched", not o.mutated,
           f"{len(o.mutated)} of {o.n_off} first offspring differ from the ancestor")
    o = observe(key, setup, {key: q}, waves=2)
    counts, left_out = filler_counts(setup, o.everything, kind)
    bad = sum(1 for c in counts if c < 0)
    counts = [c for c in counts if c > 0]
    note(f"{len(o.everything)} organisms differ from the ancestor after the second round; "
         f"{len(counts)} with a change in the filler only, {left_out} with a change elsewhere")
    # An inserted nop-B behind the jump's nop-A makes an end label there,
    # and the offspring ends at it.
    report("only this kind of change", bad <= 0.005 * len(counts), f"{bad} with the opposite length change")
    # The changes are spread over the memory; the filler gets its share.
    share = {"sub": TAIL / L * (1 - 1 / N), "ins": (TAIL + 1) / (L + 1), "del": TAIL / L}[kind]
    pmf = binom_pmf(L, q * share, 4)
    cut = [x / (1 - pmf[0]) for x in pmf[1:]]
    dist_test("changes in the filler, given at least one (1, 2, 3, 4 or more)", hist(counts, 4)[1:], cut)
    if kind == "sub":
        lo = L - 2 - TAIL
        tail = [(i, s[i]) for s in o.everything if len(s) == L and s[:lo] == setup.anc[:lo]
                and s[-2:] == setup.anc[-2:] for i in range(lo, L - 2) if s[i] != setup.anc[i]]
        per_site = [0] * TAIL
        for i, _ in tail:
            per_site[i - lo] += 1
        uniform_test("every filler site equally often", per_site)
        obs = [sum(1 for i, c in tail if c == l) for l in LETTERS[:N]]
        exp = [sum(1 for i, c in tail if setup.anc[i] != l) / (N - 1) for l in LETTERS[:N]]
        uniform_test("a random instruction is written", obs, exp)


def check_point(setup, r):
    """point_prob: every site of every organism's memory, once per update."""
    L, N = setup.len, setup.n
    o = observe("point", setup, {"point_prob": r}, skip_later=True)
    lo, hi = L - 2 - TAIL, L - 2
    tail, left_out = tail_changes(setup, o.mutated)
    n_in = o.unmutated + len(o.mutated) - left_out
    note(f"{o.unmutated + len(o.mutated)} first offspring, {left_out} left out: a change outside the filler")
    # The pass runs at the end of every update. A birth in the line of
    # update u comes after u passes: the ancestor's gestation of about 347
    # instructions at 30 per update ends in the twelfth round on average,
    # and the births are in the lines 9 to 13.
    for shift, label in ((0, "u passes before a birth in line u"), (1, "u - 1 passes"), (-1, "u + 1 passes")):
        exp = (sum(b * (1 - (1 - r) ** max(u - shift, 0)) for u, b in o.births.items()) * (1 - 1 / N)
               / sum(o.births.values()))
        if shift == 0:
            rate_test("changed sites in the unexecuted filler", len(tail), n_in * TAIL, exp)
        else:
            z, _ = z_p(len(tail), n_in * TAIL, exp)
            note(f"with {label}: {exp:.5f} expected (z = {z:+.2f})")
    per_site = [0] * TAIL
    for i, _ in tail:
        per_site[i - lo] += 1
    uniform_test("every filler site equally often", per_site)
    obs = [sum(1 for i, c in tail if c == l) for l in LETTERS[:N]]
    exp = [sum(1 for i, c in tail if setup.anc[i] != l) / (N - 1) for l in LETTERS[:N]]
    uniform_test("a random instruction is written", obs, exp)


def check_copy_indel(setup, kind, q):
    """copy_ins_prob and copy_del_prob on the heads CPU. As in the original
    (HeadCopy_ErrorCorrect), the write head moves on after the insertion or
    the removal, so neither changes the length: the insertion is overwritten
    by the next copy and leaves its random instruction in place of the one
    copied, and the removal leaves the site empty (the set's first
    instruction)."""
    L, N, anc = setup.len, setup.n, setup.anc
    o = observe(f"copy-{kind}", setup, {f"copy_{kind}_prob": q})
    report("no length change (inherited)", all(len(s) == L for s in o.mutated),
           f"{sum(len(s) != L for s in o.mutated)} of {len(o.mutated)} mutated offspring differ in length")
    subs = changes(setup, o.mutated)
    if kind == "ins":
        rate_test("a substitution per copied site", len(subs), o.n_off * L, q * (1 - 1 / N))
        obs = [sum(1 for i, c in subs if c == l) for l in LETTERS[:N]]
        exp = [sum(1 for i, c in subs if anc[i] != l) / (N - 1) for l in LETTERS[:N]]
        uniform_test("a random instruction is written", obs, exp)
    else:
        blank = sum(1 for i, c in subs if c == LETTERS[0])
        report(f"the site is left as {setup.insts[0]}", blank == len(subs), f"{blank} of {len(subs)} changes")
        rate_test("a blanked site per copied site", len(subs), o.n_off * L, q * sum(c != LETTERS[0] for c in anc) / L)
    per_site = [0] * L
    for i, _ in subs:
        per_site[i] += 1
    seen = [i for i in range(L) if kind == "ins" or anc[i] != LETTERS[0]]
    uniform_test("every site equally often", [per_site[i] for i in seen])


def describe(setup, key, value, extra=""):
    """What a setting does to the first offspring, without a judgement."""
    L = setup.len
    o = observe(f"describe-{key}", setup, {key: value}, extra=extra)
    by_len = {}
    for s in o.mutated:
        by_len[len(s) - L] = by_len.get(len(s) - L, 0) + 1
    subs = changes(setup, o.mutated)
    note(f"{o.n_off} of {o.n_anc} ancestors divided by update {int(1.6 * o.first)} (first birth in update {o.first})")
    note(f"unmutated {o.n_off - len(o.mutated)}; by length change {dict(sorted(by_len.items()))}; "
         f"{len(subs)} changed sites in offspring of the ancestor's length")
    return o, subs


# ---------------------------------------------------------------------- main

def section(name):
    """Whether to run a section, printing its heading."""
    if ONLY and ONLY not in name:
        return False
    print(name, flush=True)
    return True


front = gate_setup("gate", "front")
print(f"{front.n} instructions in the gate set; world {SIDE}x{SIDE}; {len(SEEDS)} seeds per check")

if section("copy mutations: copy_prob 0.01, gate filler"):
    check_copy(front, 0.01)
if section("copy mutations: copy_prob 0.01, pgate filler"):
    check_copy(gate_setup("pgate", "front"), 0.01)
if section("divide insertion: divide_ins_prob 0.2"):
    check_indel(front, "ins", 0.2)
if section("divide deletion: divide_del_prob 0.2"):
    check_indel(front, "del", 0.2)
if section("divide slip: divide_slip_prob 0.2"):
    check_slip(front, 0.2)
if section("divide substitution: divide_prob 0.3"):
    check_divide_sub(front, 0.3)
if section("divide uniform: divide_uniform_prob 0.3"):
    check_uniform(front, 0.3)
if section("per-site divide substitution: div_prob 0.01"):
    check_per_site(front, "sub", 0.01)
if section("per-site divide insertion: div_ins_prob 0.01"):
    check_per_site(front, "ins", 0.01)
if section("per-site divide deletion: div_del_prob 0.01"):
    check_per_site(front, "del", 0.01)
if section("poisson divide substitution: divide_poisson_mean 1.0"):
    check_per_site(front, "sub", 1.0, poisson=True)
if section("poisson divide insertion: divide_poisson_ins_mean 1.0"):
    check_per_site(front, "ins", 1.0, poisson=True)
if section("poisson divide deletion: divide_poisson_del_mean 1.0"):
    check_per_site(front, "del", 1.0, poisson=True)

tail = gate_setup("gate", "tail")
if section("parent substitution: parent_prob 0.03"):
    check_parent(tail, "sub", 0.03)
if section("parent insertion: parent_ins_prob 0.03"):
    check_parent(tail, "ins", 0.03)
if section("parent deletion: parent_del_prob 0.03"):
    check_parent(tail, "del", 0.03)
if section("point substitution: point_prob 0.001"):
    check_point(tail, 0.001)

if section("copy insertion: copy_ins_prob 0.005"):
    check_copy_indel(front, "ins", 0.005)
if section("copy deletion: copy_del_prob 0.005"):
    check_copy_indel(front, "del", 0.005)
if section("copy uniform: copy_uniform_prob 0.005 (described)"):
    describe(front, "copy_uniform_prob", 0.005)

hardware = [("heads_default", lambda: stock_setup("heads_default", "instset-heads.yaml", "default-heads.org")),
            ("transsmt", lambda: stock_setup("transsmt", "instset-transsmt.yaml", "default-transsmt.org"))]
if EXPERIMENTAL:
    hardware.append(("experimental", lambda: folder_setup("experimental hardware", EXPERIMENTAL)))
for set_name, make in hardware:
    if not ONLY or ONLY in f"hardware {set_name}":
        s = make()
        if section(f"hardware {set_name}: copy_prob 0.01"):
            check_copy(s, 0.01)
        if section(f"hardware {set_name}: divide_ins_prob 0.2"):
            check_indel(s, "ins", 0.2, positions=False)
        if section(f"hardware {set_name}: divide_del_prob 0.2"):
            check_indel(s, "del", 0.2, positions=False)

print("all checks passed" if GOOD else "SOME CHECKS FAILED")
sys.exit(0 if GOOD else 1)
