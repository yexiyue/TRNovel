"""Development-only real-worker playback and model release verification."""
import argparse
import atexit
import hashlib
import json
import queue
import shutil
import subprocess
import threading
import time
from pathlib import Path

from metrics import memory

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--worker', required=True)
parser.add_argument('--model-root', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--choices-file', help='Optional JSON array of [backend, model, voice, device] choices.')
args = parser.parse_args()
choices = [('qwen', '0.6b-customvoice', 'uncle_fu', 'cuda'),
           ('qwen', '1.7b-customvoice', 'uncle_fu', 'cuda'),
           ('qwen', '1.7b-base', 'custom:acceptance', 'cuda'),
           ('voxcpm', '2b-q8_0', 'custom:acceptance', 'cuda'),
           ('omnivoice', '0.6b', 'custom:acceptance', 'cuda'),
           ('moss', 'nano', 'Weiguo', 'cpu')]
if args.choices_file:
    choices = json.loads(Path(args.choices_file).read_text(encoding='utf-8'))
    assert choices and all(len(choice) == 4 for choice in choices)

root = Path(args.output)
root.mkdir(parents=True, exist_ok=True)
events = queue.Queue()
stderr = (root / 'worker.log').open('w', encoding='utf-8')
log = (root / 'events.jsonl').open('w', encoding='utf-8')
worker = subprocess.Popen(
    [args.worker, '--protocol', '--backend', choices[0][0], '--model', choices[0][1],
     '--voice', choices[0][2], '--tts-device', choices[0][3], '--config', str(root / 'config.json'),
     '--model-dir', args.model_root, '--checkpoint-dir', str(root / 'positions')],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, encoding='utf-8')


def cleanup():
    if worker.poll() is None:
        worker.kill()
        worker.wait()


atexit.register(cleanup)


def read():
    for line in worker.stdout:
        log.write(line)
        log.flush()
        events.put(json.loads(line))
    events.put(None)


threading.Thread(target=read, daemon=True).start()


def send(kind, request, payload=None, session=None):
    message = dict(protocol_version=5, request_id=request, session_id=session, type=kind)
    if payload is not None:
        message['payload'] = payload
    worker.stdin.write(json.dumps(message, ensure_ascii=False) + '\n')
    worker.stdin.flush()


def wait(kind, request=None):
    deadline = time.monotonic() + 180
    while True:
        event = events.get(timeout=max(0.1, deadline - time.monotonic()))
        assert event is not None, 'worker exited'
        if event['type'] == 'error':
            raise RuntimeError(event)
        if event['type'] == kind and (request is None or event['request_id'] == request):
            return event


def sample():
    result = memory(worker.pid)
    if shutil.which('nvidia-smi'):
        gpu = subprocess.run(['nvidia-smi', '--query-gpu=memory.used',
                              '--format=csv,noheader,nounits'], capture_output=True, text=True)
        if gpu.returncode == 0:
            result['vram_mib'] = int(gpu.stdout.strip())
    return result


send('hello', 'hello')
wait('ready')
results = []
for index, (backend, model, voice, device) in enumerate(choices):
    send('get_config', f'config-{index}')
    config = wait('config')['payload']
    send('update_config', f'change-{index}', dict(expected_revision=config['revision'],
         backend=backend, model=model, voice=voice, tts_device=device, style='', volume=0.0))
    wait('config_changed', f'change-{index}')
    released = sample()
    started = time.monotonic()
    send('prepare_model', f'prepare-{index}')
    wait('model_ready')
    prepared = sample()
    text = '你好，切换模型后，这次听书应当正常完成。'
    session = f'play-{index}'
    send('start', session, dict(source=dict(namespace='acceptance', book='model-switch',
         chapter=str(index)), text=text, text_hash=hashlib.sha256(text.encode()).hexdigest(),
         resume_byte=0, restore_checkpoint=False), session)
    ended = wait('session_ended')
    assert ended['payload']['reason'] == 'completed', ended
    results.append(dict(backend=backend, model=model, released=released, prepared=prepared,
                        seconds=time.monotonic() - started, completed=True))
    (root / 'progress.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
send('shutdown', 'shutdown')
worker.stdin.close()
worker.wait(timeout=30)
assert worker.returncode == 0
(root / 'result.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
print(f'{len(choices)} model switches completed with real playback and release-before-prepare samples.')
