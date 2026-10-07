"""Run a native WAV probe with CPU RSS / whole-device VRAM measurements."""
import argparse, json, subprocess, time
from pathlib import Path
from metrics import memory
p = argparse.ArgumentParser()
p.add_argument('--output', required=True)
p.add_argument('command', nargs=argparse.REMAINDER)
a = p.parse_args()
if a.command and a.command[0] == '--':
    a.command = a.command[1:]
if not a.command:
    p.error('provide a native command after --')
root = Path(a.output)
root.mkdir(exist_ok=True, parents=True)
with (root / 'stdout.log').open('w', encoding='utf-8') as stdout, (root / 'stderr.log').open('w', encoding='utf-8') as stderr:
    started = time.monotonic()
    child = subprocess.Popen(a.command, stdout=stdout, stderr=stderr)
    samples = []
    while child.poll() is None:
        sample = dict(seconds=time.monotonic() - started, **memory(child.pid))
        try:
            gpu = subprocess.run(['nvidia-smi', '--query-gpu=memory.used', '--format=csv,noheader,nounits'], capture_output=True, text=True)
            if gpu.returncode == 0:
                sample['device_vram_mib'] = gpu.stdout.strip()
        except FileNotFoundError:
            pass
        samples.append(sample)
        time.sleep(0.5)
    result = dict(exit_code=child.returncode, wall_seconds=time.monotonic() - started, samples=samples, peak_rss_bytes=max((s.get('peak_rss_bytes', s.get('rss_bytes', 0)) for s in samples), default=0))
    (root / 'metrics.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'samples'}), flush=True)
    raise SystemExit(child.returncode)
