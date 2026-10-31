"""Temporary fork-only experiment; not part of the upstream change."""
import json
import os
from pathlib import Path
import subprocess
import time

BASE = 'a4873dec445a5f619bc2200069f65a76dc170a0c'
SPLIT = '022306f5d8f9507f6d688bbe99bcc7865af3300f'
rows = []
report = {'toolchain': subprocess.check_output(['rustc', '-Vv'], text=True),
          'machine': subprocess.check_output(['lscpu'], text=True),
          'cache': 'One runner, same target directory; dependencies warmed on base; client package cleaned before each revision.',
          'limit': 'Single sample; debug profile; incremental gameplay edit is a comment, measuring best-case invalidation, not changed gameplay code generation.',
          'results': rows}

def run(label, args):
    start = time.monotonic()
    with open('build-split-' + label + '.log', 'w') as log:
        result = subprocess.run(args, stdout=log, stderr=subprocess.STDOUT)
    row = {'step': label, 'seconds': round(time.monotonic() - start, 2), 'exit': result.returncode, 'command': args}
    rows.append(row)
    Path('build-split-cost.json').write_text(json.dumps(report, indent=2))
    print(row, flush=True)
    if result.returncode:
        print(Path('build-split-' + label + '.log').read_text()[-6000:])
        raise SystemExit(result.returncode)

test = ['cargo', 'test', '--locked', '-p', 'horizon-client', '--bins', '--no-run']
subprocess.run(['git', 'checkout', BASE], check=True)
probe = next(p.stem for p in sorted(Path('crates/horizon-client/examples').glob('*_probe.rs')))
report['probe'] = probe
users = ['cargo', 'build', '--locked', '-p', 'horizon-client', '--bin', 'horizon-ctl', '--example', probe]
run('warm-dependencies', test)
for label, revision in [('base', BASE), ('split', SPLIT)]:
    subprocess.run(['git', 'checkout', revision], check=True)
    subprocess.run(['cargo', 'clean', '-p', 'horizon-client'], check=True)
    run(label + '-test-compile', test)
    run(label + '-users-warm', users)
    game = Path('crates/horizon-client/src/game.rs')
    with game.open('a') as stream:
        stream.write('\n// Temporary build-cost invalidation probe.\n')
    run(label + '-users-incremental', users)
    # Invalidate again: the preceding command warmed the library, so test
    # recompilation needs its own edit to measure both game copies.
    with game.open('a') as stream:
        stream.write('// Temporary executable-test invalidation probe.\n')
    run(label + '-test-incremental', test)
    subprocess.run(['git', 'restore', 'crates/horizon-client/src/game.rs'], check=True)

summary = os.environ.get('GITHUB_STEP_SUMMARY')
if summary:
    with open(summary, 'a') as out:
        out.write('## Build split cost\n\n| Step | Seconds |\n|---|---:|\n')
        for row in rows:
            out.write(f"| {row['step']} | {row['seconds']} |\n")
        out.write('\n' + report['cache'] + '\n\n' + report['limit'] + '\n')

# Replay the real arrow harness's commands against a freshly built client
# probe's dep-info. This isolates selection from running Java/game fixtures.
run('split-client-probe', ['cargo', 'build', '--locked', '-p', 'horizon-client', '--example', 'client_probe'])
import sys
sys.path.insert(0, str(Path.cwd() / 'tools'))
import parity
parity.TARGET = Path(os.environ['CARGO_TARGET_DIR'])
binary = parity.TARGET / 'debug/examples/client_probe'
trace = {'modules': [], 'read': [], 'commands': [
    ['cargo', 'build', '--locked', '-p', 'horizon-client', '--example', 'client_probe'],
    [str(binary), 'arrow_probe', '.local/horizon-client/26.3', '/tmp/arrow-replay', '--describe'],
]}
dependencies, artifacts = parity.dependencies(trace)
game_sources = [p for p in dependencies if p == 'crates/horizon-client/src/game.rs' or p.startswith('crates/horizon-client/src/game/')]
assert game_sources, 'The rebuilt probe must actually list gameplay sources.'
print('Rebuilt arrow probe lists', len(game_sources), 'gameplay sources', flush=True)
# Empty records exclude other harnesses from this one-harness experiment;
# this is not a measurement of the whole fleet's historical records.
records = {name: {'dependencies': [script.relative_to(parity.ROOT).as_posix()], 'artifacts': []}
           for name, script in parity.harnesses().items()}
records['arrow'] = {'dependencies': dependencies, 'artifacts': artifacts, 'status': 'fail'}
parity.STATE.parent.mkdir(parents=True, exist_ok=True)
parity.STATE.write_text(json.dumps({'harnesses': records, 'hashes': {}}))
Path('scope-changed.txt').write_text('crates/horizon-client/src/game/combat.rs\n')
run('split-arrow-scope-default', ['python3', 'tools/parity.py', '--changed-files', 'scope-changed.txt', '--list'])
run('split-arrow-scope-by-crate', ['python3', 'tools/parity.py', '--changed-files', 'scope-changed.txt', '--list', '--by-crate'])
for name in ('split-arrow-scope-default', 'split-arrow-scope-by-crate'):
    print(name + ':\n' + Path('build-split-' + name + '.log').read_text(), flush=True)
Path('build-split-probe-dependencies.log').write_text(json.dumps({'trace': trace, 'dependencies': dependencies, 'artifacts': artifacts}, indent=2))
