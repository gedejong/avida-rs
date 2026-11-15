#!/usr/bin/env python3
"""Packs and unpacks the reference output of the consistency suite, so that
the suite can run where the C++ reference binary is not built (CI).

  suite_ref.py pack     SUITE_WORK/ref -> tools/suite-ref.tar.xz
  suite_ref.py unpack   tools/suite-ref.tar.xz -> SUITE_WORK/ref

The snapshot holds what the reference binary (built from the git tag
cpp-final) wrote for every test of avida-core/tests, made on macOS arm64.
Refresh it after a test is added or changed: delete SUITE_WORK/ref, run
tools/run_suite.py with the reference binary present, then pack.
"""
import io, os, re, sys, tarfile
WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = os.path.normpath(os.path.join(WS, '..', '..'))
S = os.environ.get('SUITE_WORK', os.path.join(WS, 'target', 'suite'))
REF = os.environ.get('SUITE_REF', os.path.join(REPO, 'cbuild', 'work', 'avida'))
SNAP = os.path.join(WS, 'tools', 'suite-ref.tar.xz')
cmd = sys.argv[1] if len(sys.argv) > 1 else ''
if cmd == 'pack':
    # Paths of the machine the snapshot was made on are taken out: the
    # reference binary's own path becomes AVIDA and a printed data directory
    # becomes D, exactly as run_suite.py normalises both sides when comparing.
    ref = os.path.join(S, 'ref')
    with tarfile.open(SNAP, 'w:xz') as tar:
        for dp, ds, fs in os.walk(ref):
            ds.sort()
            for f in sorted(fs):
                full = os.path.join(dp, f)
                data = open(full, 'rb').read()
                if f in ('.stdout', '.stderr'):
                    data = data.replace(REF.encode(), b'AVIDA')
                    data = re.sub(rb'Data Directory: [^\n]*', b'Data Directory: D', data)
                if ref.encode() in data or REF.encode() in data:
                    sys.exit('a local path is left in ' + full)
                ti = tarfile.TarInfo(os.path.relpath(full, S))
                ti.size = len(data); ti.mode = 0o644
                tar.addfile(ti, io.BytesIO(data))
    print(SNAP, os.path.getsize(SNAP), 'bytes')
elif cmd == 'unpack':
    os.makedirs(S, exist_ok=True)
    with tarfile.open(SNAP) as tar:
        tar.extractall(S, filter='data')
    print('unpacked into', os.path.join(S, 'ref'))
else:
    sys.exit(__doc__)
