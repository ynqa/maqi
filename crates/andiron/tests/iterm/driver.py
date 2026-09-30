"""PTY driver for the iTerm XCTest bridge; invoked by run.py.

The XCTest only hosts the terminal protocol. The assertions below determine
whether the editor preserves all retained rows, including internal blank gaps.
"""
import os, pty, fcntl, termios, struct, select, time, json, base64, signal
from pathlib import Path
import sys
root = Path(sys.argv[1]).resolve()
maqi = Path(sys.argv[2]).resolve()
config = json.loads((root / 'config.json').read_text())
height = config['rows']
shells = ['maqi', 'zsh'] if config['shell'] == 'both' else [config['shell']]
trace = None
index = 0
state = {}
fd = None
frames = []
backend = bytearray()

def rpc(obj):
    global index, state
    i = index
    index += 1
    tmp = root / f'request-{i}.tmp'
    tmp.write_text(json.dumps(obj))
    tmp.rename(root / f'request-{i}.json')
    response = root / f'response-{i}.json'
    until = time.monotonic() + 15
    while not response.exists():
        if time.monotonic() > until:
            raise RuntimeError(f'engine timeout {i}')
        time.sleep(0.001)
    state = json.loads(response.read_text())
    reply = base64.b64decode(state['reply'])
    if fd is not None and reply:
        os.write(fd, reply)

def pump(duration):
    until = time.monotonic() + duration
    while time.monotonic() < until:
        (readable, _, _) = select.select([fd], [], [], max(0, min(0.01, until - time.monotonic())))
        if readable:
            try:
                data = os.read(fd, 65536)
            except OSError:
                return
            if not data:
                return
            backend.extend(data)
            if trace is not None:
                trace.write(json.dumps({'time': time.monotonic(), 'data': base64.b64encode(data).decode()}) + '\n')
                trace.flush()
            rpc({'data': base64.b64encode(data).decode()})

def send(s):
    os.write(fd, s.encode())
    pump(0.4)

def record(label):
    snap = dict(state, case=case, label=label)
    frames.append(snap)
    (root / 'frames.json').write_text(json.dumps(frames, indent=2))
    text = state['dump'].replace('.', '').replace('\n', '').replace(' ', '')
    print(case, label, 'history', state['history'], 'prompts', text.count('maqi>') or text.count('probe>'), 'H00', text.count('H00'), 'H29', text.count('H29'), flush=True)
    print('\n'.join(state['rows']), flush=True)
def retained_rows():
    rows = [row.rstrip() for row in state['all_rows']]
    while rows and not rows[-1]:
        rows.pop()
    return rows

def run_scenario(ast):
    global fd, case, trace, height
    assert ast['command'] in ('maqi', 'CARGO_BIN_EXE_maqi'), 'the iTerm fixture only supports maqi scenarios'
    height = ast['rows']
    width = ast['cols']
    case = ast['name']
    offset = 0
    trace = (root / (case + '-output.jsonl')).open('w')
    # Put committed output in native history before attaching the readline.
    seed = b'\x1bc\x1b[H\x1b[J\x1b[3J' + b''.join(f'H{i:02}\r\n'.encode() for i in range(30))
    seed += b'\r\n' * (height - 1)
    seed += f"\x1b[{ast['cursor_row']};{ast['cursor_col']}H".encode()
    rpc({'width': width, 'height': height, 'data': base64.b64encode(seed).decode()})
    (pid, fd) = pty.fork()
    if pid == 0:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        env = dict(os.environ, TERM='xterm-256color')
        env.update(ast['env'])
        env.pop('COLUMNS', None)
        env.pop('LINES', None)
        os.execve(str(maqi), [str(maqi), *ast['args']], env)
    def viewport():
        end = len(state['all_rows']) - offset
        return state['all_rows'][end - height:end]
    try:
        pump(0.1)
        for step in ast['steps']:
            before = retained_rows()
            before_size = (height, width)
            only_resizes = bool(step['actions']) and all('cols' in a for a in step['actions'])
            for action in step['actions']:
                if 'input' in action:
                    offset = 0
                    os.write(fd, action['input'].encode())
                elif 'cols' in action:
                    height, width = action['rows'], action['cols']
                    offset = 0
                    rpc({'width': width, 'height': height})
                    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
                    pump(0.004)
                elif 'scroll' in action:
                    change = action['lines'] * (1 if action['scroll'] == 'up' else -1)
                    offset = max(0, min(state['history'], offset + change))
                else:
                    deadline = time.monotonic() + action['timeout_ms'] / 1000
                    while True:
                        if 'wait_frontend' in action:
                            ready = any(row.startswith(action['wait_frontend']) for row in viewport())
                        else:
                            ready = any(row.startswith(action['wait_backend'].encode()) for row in backend.split(b'\n'))
                        if ready:
                            break
                        if time.monotonic() >= deadline:
                            raise AssertionError(f"{step['label']}: wait timed out: {viewport()}")
                        pump(0.01)
            pump(step['settle_ms'] / 1000)
            deadline = time.monotonic() + step['timeout_ms'] / 1000
            while viewport() != step['expect'] and time.monotonic() < deadline:
                pump(0.01)
            record(step['label'])
            assert viewport() == step['expect'], (step['label'], 'expected', step['expect'], 'actual', viewport())
            if only_resizes and before_size == (height, width):
                assert retained_rows() == before, (step['label'], 'retained history changed', before, retained_rows())
            print(case, step['label'], 'PASS', flush=True)
        expected_history = dict(ast['env']).get('ITERM_EXPECT_HISTORY')
        if expected_history is not None:
            assert state['history'] == int(expected_history), ('unexpected retained history length', state['history'])
    finally:
        os.kill(pid, signal.SIGTERM)
        os.close(fd)
        fd = None
        os.waitpid(pid, 0)
        trace.close()
        trace = None

until = time.monotonic() + 180
while not (root / 'ready').exists():
    if time.monotonic() > until:
        raise RuntimeError('engine not ready')
    time.sleep(0.2)
try:
    for scenario in config.get('scenario', []):
        run_scenario(scenario)
    for shell in ([] if config.get('scenario') else shells):
        for mode in config['modes']:
            case = shell + '-' + mode
            trace = (root / (case + '-output.jsonl')).open('w')
            seed = b'\x1bc\x1b[H\x1b[J\x1b[3J' + b''.join(f'H{i:02}\r\n'.encode() for i in range(30))
            if config['cursor_row'] is not None:
                seed += b'\r\n' * (height - 1)
                seed += f"\x1b[{config['cursor_row']};1H".encode()
            rpc({'width': 80, 'height': height, 'data': base64.b64encode(seed).decode()})
            (pid, fd) = pty.fork()
            if pid == 0:
                fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', height, 80, 0, 0))
                env = dict(os.environ, TERM='xterm-256color', TERM_PROGRAM='iTerm.app', ZDOTDIR=str(root / 'zsh'))
                env.pop('COLUMNS', None)
                env.pop('LINES', None)
                if shell == 'zsh':
                    os.execve('/bin/zsh', ['zsh', '-d', '-i'], env)
                env['PATH'] = '/nonexistent'
                os.execve(str(maqi), ['maqi'], env)
            try:
                pump(1)
                send('kubectl \t' if shell == 'maqi' else config['zsh_input'])
                deadline = time.monotonic() + 15
                while not any(('apply' in row for row in state['rows'])):
                    if time.monotonic() > deadline:
                        raise RuntimeError('completion menu did not appear')
                    pump(0.1)
                record('before')
                if mode == 'jump':
                    sizes = [1, 80]
                elif mode == 'burst':
                    sizes = [64, 48, 32, 16, 8, 1, 8, 16, 32, 48, 64, 80]
                else:
                    sizes = list(range(79, 0, -1)) + list(range(2, 81))
                sizes *= config['cycles']
                last_notice = time.monotonic()
                for cols in sizes:
                    rpc({'width': cols, 'height': height})
                    if mode == 'jump' and cols == 1:
                        record('narrow-before-repaint')
                    if mode != 'lag' or time.monotonic() - last_notice >= 0.2 or cols == 80:
                        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', height, cols, 0, 0))
                        last_notice = time.monotonic()
                    pump(0.35 if mode == 'jump' else 0.004)
                pump(0.7)
                record('after')
            finally:
                os.kill(pid, signal.SIGTERM)
                os.close(fd)
                fd = None
                os.waitpid(pid, 0)
                trace.close()
                trace = None
finally:
    tmp = root / 'stop.tmp'
    tmp.write_text(json.dumps({'stop': True}))
    tmp.rename(root / f'request-{index}.json')

if config.get('scenario'):
    raise SystemExit(0)

def normalized(frame):
    rows = [row.rstrip() for row in frame['all_rows']]
    while rows and (not rows[-1]):
        rows.pop()
    return rows
failures = []
for case in sorted({f['case'] for f in frames}):
    before = next((f for f in frames if f['case'] == case and f['label'] == 'before'))
    after = next((f for f in frames if f['case'] == case and f['label'] == 'after'))
    passed = normalized(before) == normalized(after)
    print(case, 'complete retained text:', 'PASS' if passed else 'FAIL', flush=True)
    if case.startswith('maqi-') and (not passed):
        failures.append(case)
if failures:
    raise SystemExit('Unresolved scrollback regressions: ' + ', '.join(failures))
