#!/usr/bin/env python3
"""Runs every avida-core/tests consistency test with the C++ reference binary
(cached) and the Rust binary, and compares stdout plus every file written.

usage: run_suite.py [name-substring ...]

Environment:
  SUITE_REF   the reference binary (default <repo>/cbuild/work/avida; the C++
              was retired from the tree, build it from the git tag cpp-final)
  SUITE_REF_TESTS  avida-core/tests in the old input format, for the reference
              binary (which cannot read avida.yaml): for example the tests
              of a cpp-final worktree. Needed only for tests whose reference
              output is not cached.
  SUITE_BIN   the Rust binary (default target/release/avida of this workspace)
  SUITE_WORK  where runs and cached reference output go (default target/suite)
  SUITE_OUT   name of the output directory of this run inside SUITE_WORK
  SUITE_JOBS  tests run at a time (default 8)
"""
import os, re, shutil, subprocess, sys, json, configparser
from concurrent.futures import ThreadPoolExecutor
WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = os.path.normpath(os.path.join(WS, '..', '..'))
TESTS = os.path.join(REPO, 'avida-core', 'tests')
REF = os.environ.get('SUITE_REF', os.path.join(REPO, 'cbuild', 'work', 'avida'))
RS = os.environ.get('SUITE_BIN', os.path.join(WS, 'target', 'release', 'avida'))
S = os.environ.get('SUITE_WORK', os.path.join(WS, 'target', 'suite'))
RSROOT = os.environ.get('SUITE_OUT', 'rs')
os.makedirs(S, exist_ok=True)
os.chdir(S)
if not os.path.exists(RS): sys.exit('missing binary: ' + RS)
# The reference binary is needed only for tests whose reference output is not
# cached yet; a snapshot unpacked into SUITE_WORK/ref (tools/suite_ref.py)
# makes the suite run without it.
STAMP = re.compile(rb'^# [A-Z][a-z][a-z] [A-Z][a-z][a-z] .*\n', re.M)
REF_TESTS = os.environ.get('SUITE_REF_TESTS')
def args_of(t, tests=TESTS):
    for line in open(os.path.join(tests, t, 'test_list'), errors='replace'):
        m = re.match(r'\s*args\s*=(.*)', line)
        if m:
            a = m.group(1).split(';')[0].strip()
            return a.replace('%(default_app)s', '').replace('%(cpus)s', '1').split()
    return []
def run(binary, t, root, timeout, tests=TESTS):
    d = os.path.join(root, t)
    shutil.rmtree(d, ignore_errors=True)
    shutil.copytree(os.path.join(tests, t, 'config'), d)
    before = set(files(d))
    try:
        p = subprocess.run([binary] + args_of(t, tests), cwd=d, capture_output=True, timeout=timeout)
        rc, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        rc, out, err = 'timeout', e.stdout or b'', e.stderr or b''
    open(os.path.join(d, '.stdout'), 'wb').write(out)
    open(os.path.join(d, '.stderr'), 'wb').write(err)
    open(os.path.join(d, '.rc'), 'w').write(str(rc))
    open(os.path.join(d, '.before'), 'w').write('\n'.join(sorted(before)))
def files(d):
    r = []
    for dp, _, fs in os.walk(d):
        for f in fs:
            r.append(os.path.relpath(os.path.join(dp, f), d))
    return r
def norm(path):
    return re.sub(rb'Data Directory: [^\n]*', b'Data Directory: D', re.sub(rb'Random Seed: -1 -> \d+', b'Random Seed: -1 -> T', STAMP.sub(b'', open(path, 'rb').read())))
# The reference output was made from the old input files (avida.cfg,
# environment.cfg, events.cfg and instruction-set files); the tests now give
# the same settings in one experiment file, avida.yaml. What the reference
# binary printed about the old files alone is taken out of its output before
# the comparison: the version-number and unknown-line warnings of avida.cfg,
# the note of a legacy instruction-set file, the name of a missing avida.cfg
# and the old names in the echo of -set (docs/reference/config-yaml.md, "Old
# names"; the file-selecting settings are folded into avida.yaml).
OLD_NAMES = {}
for line in open(os.path.join(REPO, 'docs', 'reference', 'config-yaml.md')):
    m = re.match(r'\| `([A-Z0-9_]+)` \| `([a-z0-9_]+\.[a-z0-9_]+)`', line)
    if m: OLD_NAMES[m.group(1).encode()] = m.group(2).encode()
FOLDED = (b'EVENT_FILE', b'ENVIRONMENT_FILE', b'INST_SET')
def old_format(data):
    data = re.sub(rb"(?m)^warning: config file version number mismatch -- [^\n]*\n", b'', data)
    data = re.sub(rb"(?m)^warning: unknown lines in input file '[^'\n]*'\.\n(  \S+:\d+: [^\n]*\n)*", b'', data)
    data = re.sub(rb"(?m)^using default instruction set: [^\n]*\n", b'', data)
    data = data.replace(b"error: unable to open file 'avida.cfg'.", b"error: unable to open file 'avida.yaml'.")
    data = data.replace(b"error: configuration file 'avida.cfg' not found", b"error: configuration file 'avida.yaml' not found")
    def cmdline_set(m):
        if m.group(1) in FOLDED: return b''
        return b'CmdLine Set: ' + OLD_NAMES.get(m.group(1), m.group(1)) + b' ' + m.group(2) + b'\n'
    return re.sub(rb'(?m)^CmdLine Set: (\S+) ([^\n]*)\n', cmdline_set, data)
def one(t):
    refd = os.path.join(S, 'ref', t)
    if not os.path.exists(os.path.join(refd, '.rc')):
        if not os.path.exists(REF): return t, 'NO-REF', 'no cached reference output and no reference binary ' + REF
        if not REF_TESTS or not os.path.exists(os.path.join(REF_TESTS, t, 'config')):
            return t, 'NO-REF', 'no cached reference output and no old-format copy of the test (SUITE_REF_TESTS)'
        run(REF, t, os.path.join(S, 'ref'), 900, REF_TESTS)
    run(RS, t, os.path.join(S, RSROOT), 900)
    rsd = os.path.join(S, RSROOT, t)
    rrc = open(os.path.join(refd, '.rc')).read(); src = open(os.path.join(rsd, '.rc')).read()
    err = open(os.path.join(rsd, '.stderr'), errors='replace').read()
    m = re.search(r'unsupported[^\n]*', err)
    if m: return t, 'UNSUPPORTED', m.group(0)[:150]
    if rrc == 'timeout': return t, 'REF-TIMEOUT', ''
    diffs = []
    if rrc != src: diffs.append(f'rc {rrc}!={src}')
    rf, sf = set(files(refd)), set(files(rsd))
    # An input file of one side only (an old configuration file of the
    # reference run, avida.yaml of this one) is not output.
    before = lambda d: set(open(os.path.join(d, '.before')).read().split('\n'))
    inputs = (before(refd) - sf) | (before(rsd) - rf)
    names = sorted((rf | sf) - {'.rc', '.before', '.stderr'} - inputs)
    argv0 = lambda b: b.replace(REF.encode(), b'AVIDA').replace(RS.encode(), b'AVIDA')
    for f in names:
        a, b = os.path.join(refd, f), os.path.join(rsd, f)
        if not os.path.exists(b): diffs.append('missing ' + f)
        elif not os.path.exists(a): diffs.append('extra ' + f)
        elif (old_format(norm(a)) if f == '.stdout' else norm(a)) != norm(b): diffs.append(f)
    if old_format(argv0(norm(os.path.join(refd, '.stderr')))) != argv0(norm(os.path.join(rsd, '.stderr'))): diffs.append('.stderr')
    if not diffs and rrc != '0' and not t.startswith('analyze_resize'): return t, 'trivial', 'reference exits ' + rrc
    if diffs: return t, 'DIFF', ', '.join(diffs)[:300] + ((' | ' + err.strip().splitlines()[-1][:150]) if err.strip() and src != '0' else '')
    return t, 'same', ''
tests = sorted(t for t in os.listdir(TESTS) if os.path.exists(os.path.join(TESTS, t, 'test_list')))
if len(sys.argv) > 1: tests = [t for t in tests if any(s in t for s in sys.argv[1:])]
with ThreadPoolExecutor(int(os.environ.get('SUITE_JOBS', '8'))) as ex: res = list(ex.map(one, tests))
json.dump(res, open(os.path.join(S, 'results-' + RSROOT + '.json'), 'w'), indent=1)
count = {}
for t, st, d in res:
    count[st] = count.get(st, 0) + 1
    print(f'{st:12} {t}  {d}')
print(count)
# For CI: SUITE_EXPECT="same=85,trivial=34,UNSUPPORTED=1,REF-TIMEOUT=1" fails
# the run when the counts differ; a DIFF or NO-REF always fails it.
expect = os.environ.get('SUITE_EXPECT')
bad = count.get('DIFF', 0) + count.get('NO-REF', 0)
if expect:
    want = {k: int(v) for k, v in (kv.split('=') for kv in expect.split(','))}
    if want != count:
        print('expected', want); sys.exit(1)
if bad: sys.exit(1)
