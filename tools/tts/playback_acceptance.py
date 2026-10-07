import subprocess, threading, queue, json, time, hashlib
from pathlib import Path
import argparse, os, atexit
parser = argparse.ArgumentParser(description='Development-only protocol playback acceptance; no Python is used by novel-tts.')
parser.add_argument('--worker', default='target/release/novel-tts.exe')
parser.add_argument('--backend', required=True)
parser.add_argument('--model', required=True)
parser.add_argument('--voice', required=True)
parser.add_argument('--device', default='cuda')
parser.add_argument('--model-root', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--seconds', type=int, default=1800)
args = parser.parse_args()
root = Path(args.output)
root.mkdir(exist_ok=True, parents=True)
from metrics import memory
text = Path('tools/tts/acceptance-corpus.txt').read_text(encoding='utf-8') * 160
(root / 'corpus.txt').write_text(text, encoding='utf-8')
os.environ['NOVEL_TTS_DIAGNOSTICS'] = '1'
stderr = (root / 'worker.log').open('w', encoding='utf-8')
p = subprocess.Popen([args.worker, '--protocol', '--backend', args.backend, '--model', args.model, '--voice', args.voice, '--tts-device', args.device, '--config', str(root / 'config.json'), '--model-dir', args.model_root, '--checkpoint-dir', str(root / 'positions')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, encoding='utf-8')

def cleanup():
    if p.poll() is None:
        p.kill()
        p.wait()

atexit.register(cleanup)
events = queue.Queue()
log = (root / 'events.jsonl').open('w', encoding='utf-8')

def read():
    for line in p.stdout:
        log.write(line)
        log.flush()
        events.put(json.loads(line))
    events.put(None)
threading.Thread(target=read, daemon=True).start()

def send(kind, request, payload=None, session=None):
    d = dict(protocol_version=5, request_id=request, session_id=session, type=kind)
    if payload is not None:
        d['payload'] = payload
    p.stdin.write(json.dumps(d, ensure_ascii=False) + '\n')
    p.stdin.flush()

def wait_for(kind, deadline):
    while time.monotonic() < deadline:
        e = events.get(timeout=max(0.1, deadline - time.monotonic()))
        assert e is not None, 'worker exited'
        if e['type'] == 'error':
            raise RuntimeError(e)
        if e['type'] == kind:
            return e
    raise TimeoutError(kind)
send('hello', 'hello')
wait_for('ready', time.monotonic() + 30)
send('get_config', 'config')
config = wait_for('config', time.monotonic() + 30)['payload']
send('update_config', 'mute', dict(expected_revision=config['revision'], volume=0.0))
wait_for('config_changed', time.monotonic() + 30)
send('prepare_model', 'prepare')
wait_for('model_ready', time.monotonic() + 180)
source = dict(namespace='acceptance', book=f'{args.backend}-clone-soak', chapter='1')

def start(value, id):
    send('start', id, dict(source=source, text=value, text_hash=hashlib.sha256(value.encode()).hexdigest(), resume_byte=0, restore_checkpoint=False), id)
start(text, 'soak')
started = time.monotonic()
samples = []
playing = False
segments = 0
underruns = 0
deadline = started + args.seconds
while time.monotonic() < deadline:
    try:
        e = events.get(timeout=min(5, max(0.1, deadline - time.monotonic())))
    except queue.Empty:
        e = 'timeout'
    if e is None:
        raise RuntimeError('worker exited')
    if isinstance(e, dict):
        if e['type'] == 'error':
            raise RuntimeError(e)
        if e['type'] == 'session_ended':
            raise RuntimeError(e)
        if e['type'] == 'session_state' and e['payload']['state'] == 'playing':
            playing = True
        if playing and e['type'] == 'session_state' and e['payload']['state'] == 'buffering':
            underruns += 1
        if e['type'] == 'segment_started':
            segments += 1
    if len(samples) == 0 or time.monotonic() - samples[-1]['at'] >= 10:
        try:
            gpu = subprocess.run(['nvidia-smi', '--query-gpu=memory.used', '--format=csv,noheader,nounits'], capture_output=True, text=True).stdout.strip()
        except FileNotFoundError:
            gpu = None
        samples.append(dict(at=time.monotonic(), elapsed=time.monotonic() - started, vram_mib=gpu, **memory(p.pid)))
        (root / 'progress.json').write_text(json.dumps(dict(elapsed=time.monotonic() - started, playing=playing, segments=segments, underruns=underruns, memory=samples), indent=2))
send('stop', 'cancel', session='soak')
cancel_at = time.monotonic()
wait_for('session_ended', time.monotonic() + 20)
cancel_ms = (time.monotonic() - cancel_at) * 1000
start('你好，取消后再次生成的请求应当正常完成。', 'after-cancel')
end = wait_for('session_ended', time.monotonic() + 90)
assert end['payload']['reason'] == 'completed', end
send('shutdown', 'shutdown')
p.stdin.close()
p.wait(timeout=30)
assert p.returncode == 0 and playing
(root / 'result.json').write_text(json.dumps(dict(seconds=time.monotonic() - started, playing=playing, segments=segments, underruns=underruns, stop_ack_ms=cancel_ms, after_cancel='completed', memory=samples), indent=2))
assert underruns == 0, f'{underruns} buffering underruns after playback began'
print(f'{args.backend}: {args.seconds}-second actual playback and subsequent request completed', flush=True)
