#!/usr/bin/env python3
"""Run maqi and zsh against iTerm's real parser/screen, without GUI automation.

Requires an already buildable iTerm source checkout and Xcode. The checkout's
engine is not modified; a temporary XCTest hosts the screen until the driver
finishes. Artifacts include every retained history row, not just the live screen.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--iterm', type=Path, required=True)
parser.add_argument('--build-dir', type=Path, required=True)
parser.add_argument('--packages', type=Path, required=True)
parser.add_argument('--maqi', type=Path, required=True)
parser.add_argument('--output', type=Path)
parser.add_argument('--scenario', type=Path, nargs='+', help='Run termharness .th scenarios against the iTerm engine')
parser.add_argument('--rows', type=int, default=12)
parser.add_argument('--cursor-row', type=int, help='1-based initial prompt row, after seeding history')
parser.add_argument('--cycles', type=int, default=3)
parser.add_argument('--modes', nargs='+', choices=['jump', 'burst', 'smooth', 'lag'], default=['jump', 'burst', 'smooth', 'lag'])
parser.add_argument('--shell', choices=['maqi', 'zsh', 'both'], default='both')
parser.add_argument('--zsh-config', type=Path, help='Isolated startup configuration to compare instead of the synthetic menu')
parser.add_argument('--zsh-input', default='probe a\t', help='Literal input, including tabs, sent to the comparison shell')
args = parser.parse_args()
if args.rows < 1 or args.cycles < 1:
    parser.error('--rows and --cycles must be positive')
if args.cursor_row is not None and not 1 <= args.cursor_row <= args.rows:
    parser.error('--cursor-row must be within the screen')
here = Path(__file__).resolve().parent
output = (args.output or Path(tempfile.mkdtemp(prefix='andiron-iterm-'))).resolve()
output.mkdir(parents=True, exist_ok=True)
if any(output.iterdir()):
    parser.error('--output must be empty (old RPC responses invalidate the test)')
print(f'Artifacts: {output}', flush=True)
scenarios = []
for path in args.scenario or []:
    exported = subprocess.check_output([
        'cargo', 'run', '--quiet', '-p', 'andiron', '--example',
        'export_iterm_scenario', '--', str(path.resolve()),
    ], cwd=here.parents[3])
    scenarios.append(json.loads(exported))
(output / 'config.json').write_text(json.dumps({
    'rows': args.rows, 'shell': args.shell, 'zsh_input': args.zsh_input,
    'cycles': args.cycles, 'modes': args.modes, 'cursor_row': args.cursor_row,
    'zsh_config': str(args.zsh_config.resolve()) if args.zsh_config else 'synthetic five-candidate menu',
    'scenario': scenarios,
}, indent=2))
config = output / 'zsh'
config.mkdir()
(config / '.zshrc').write_text('''PROMPT='probe> '
RPROMPT=''
unsetopt BEEP
setopt AUTO_LIST AUTO_MENU
autoload -Uz compinit
compinit -D -i
_probe() {
  local -a candidates
  candidates=(
    'alpha:Commands for features in alpha'
    'annotate:Update the annotations on a resource'
    'api-resources:Print the supported API resources on the server'
    'api-versions:Print the supported API versions on the server'
    'apply:Apply a configuration to a resource by filename or stdin'
  )
  _describe 'commands' candidates
}
compdef _probe probe
zmodload zsh/complist
zstyle ':completion:*' menu select
''')
if args.zsh_config:
    (config / '.zshrc').write_text(args.zsh_config.read_text())
test = args.iterm.resolve() / 'ModernTests/iTermLineAttributeTests.swift'
original = test.read_bytes()
marker = '    // MARK: - Helper Functions'
source = original.decode()
if marker not in source or 'testAndironScrollbackBridge' in source:
    parser.error('unsupported/already instrumented iTerm test source')
bridge = (here / 'bridge.swift').read_text().replace('@DIRECTORY@', json.dumps(str(output), ensure_ascii=False))
command = ['xcodebuild', 'test', '-project', 'iTerm2.xcodeproj', '-scheme', 'ModernTests',
           '-derivedDataPath', str(args.build_dir.resolve()),
           '-clonedSourcePackagesDirPath', str(args.packages.resolve()),
           '-destination', 'platform=macOS', '-jobs', '4',
           '-only-testing:ModernTests/iTermLineAttributeTests/testAndironScrollbackBridge',
           '-parallel-testing-enabled', 'NO', 'CODE_SIGN_IDENTITY=',
           'CODE_SIGNING_REQUIRED=NO', 'CODE_SIGNING_ALLOWED=NO']
engine = None
try:
    test.write_text(source.replace(marker, bridge + marker, 1))
    with (output / 'engine.log').open('w') as log:
        engine = subprocess.Popen(command, cwd=args.iterm, stdout=log, stderr=subprocess.STDOUT)
        with (output / 'driver.log').open('w') as driver_log:
            driver = subprocess.Popen([sys.executable, str(here / 'driver.py'), str(output), str(args.maqi.resolve())],
                                      stdout=driver_log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 360
                while driver.poll() is None:
                    if engine.poll() not in (None, 0):
                        raise RuntimeError(f'iTerm engine failed; see {output / "engine.log"}')
                    if time.monotonic() >= deadline:
                        raise TimeoutError('scenario driver timed out')
                    time.sleep(0.05)
            finally:
                if driver.poll() is None:
                    driver.terminate()
                    driver.wait(timeout=20)
        engine_status = engine.wait(timeout=40)
    print((output / 'driver.log').read_text())
    raise SystemExit(driver.returncode or engine_status)
finally:
    if engine is not None and engine.poll() is None:
        engine.terminate()
        try:
            engine.wait(timeout=10)
        except subprocess.TimeoutExpired:
            engine.kill()
            engine.wait()
    test.write_bytes(original)
