#!/usr/bin/env python3
"""Opt-in native protocol smoke using explicitly supplied MOSS Nano models/device."""
import argparse
import hashlib
import json
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--program', type=Path, required=True)
parser.add_argument('--models', type=Path, required=True)
args = parser.parse_args()
text = '第一句测试。\n第二句验证暂停与恢复。\n第三句完成。'
with tempfile.TemporaryDirectory() as directory:
    for finish in ('shutdown', 'eof'):
        worker = subprocess.Popen([str(args.program.resolve()), '--protocol', '--model-dir', str(args.models.resolve()), '--config', f'{directory}/config.json', '--checkpoint-dir', f'{directory}/positions'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        messages = queue.Queue()
        logs = []
        def collect():
            try:
                for line in worker.stdout:
                    messages.put(json.loads(line))
            except Exception as error:
                messages.put(error)
        def collect_logs():
            for line in worker.stderr:
                logs.append(line)
        collector = threading.Thread(target=collect, daemon=True)
        collector.start()
        threading.Thread(target=collect_logs, daemon=True).start()
        sequence = 0
        instance = None
        counter = 0
        def send(kind, session=None, **payload):
            global counter
            counter += 1
            request = dict(protocol_version=1, request_id=str(counter), session_id=session, type=kind)
            if payload: request['payload'] = payload
            worker.stdin.write(json.dumps(request, ensure_ascii=False)+'\n')
            worker.stdin.flush()
            return str(counter)
        def until(predicate, timeout=30):
            global sequence, instance
            deadline = time.monotonic()+timeout
            while time.monotonic() < deadline:
                message = messages.get(timeout=max(.01, deadline-time.monotonic()))
                if isinstance(message, Exception): raise message
                assert message['protocol_version'] == 1
                assert message['sequence'] > sequence
                sequence = message['sequence']
                if instance is None: instance = message['instance_id']
                assert instance == message['instance_id']
                assert message['type'] != 'error', message
                if predicate(message): return message
            raise TimeoutError('worker event timed out')
        try:
            send('hello')
            until(lambda event: event['type'] == 'ready')
            send('get_config')
            config = until(lambda event: event['type'] == 'config')['payload']
            send('update_config', expected_revision=config['revision'], speed=2.0, volume=0.4)
            until(lambda event: event['type'] == 'config_changed')
            send('prepare_model')
            until(lambda event: event['type'] == 'model_ready')
            payload = dict(source=dict(namespace='smoke', book='self-authored', chapter=finish), text=text, text_hash=hashlib.sha256(text.encode()).hexdigest(), resume_byte=0, restore_checkpoint=False)
            send('start', 'first', **payload)
            first = until(lambda event: event['type'] == 'segment_started')
            assert first['payload']['range']['start'] == 0
            request = send('pause', 'first')
            until(lambda event: event['request_id'] == request)
            time.sleep(.05)
            request = send('resume', 'first')
            until(lambda event: event['request_id'] == request)
            byte = len('第一句测试。\n'.encode())
            send('seek', 'first', byte=byte, new_session_id='seeked')
            event = until(lambda event: event['type'] == 'segment_started' and event['session_id'] == 'seeked')
            assert event['payload']['range']['start'] == byte
            if finish == 'shutdown':
                request = send('stop', 'seeked')
                until(lambda event: event['request_id'] == request)
                send('start', 'complete', **payload)
                event = until(lambda event: event['type'] == 'session_ended' and event['session_id'] == 'complete')
                assert event['payload']['reason'] == 'completed'
                send('start', 'shutdown-active', **payload)
                until(lambda event: event['type'] == 'segment_started' and event['session_id'] == 'shutdown-active')
                send('shutdown')
            else:
                worker.stdin.close()
            worker.wait(timeout=5)
            assert worker.returncode == 0, ''.join(logs)
            collector.join(timeout=2)
            assert not collector.is_alive()
            while not messages.empty():
                event = messages.get_nowait()
                if isinstance(event, Exception): raise event
                assert event["instance_id"] == instance
                assert event["sequence"] > sequence
                sequence = event["sequence"]
            # Every stdout line has been parsed as JSON.
            print(f'{finish}: native handshake, prepare, pause/resume, seek and exit passed')
        finally:
            if worker.poll() is None: worker.kill()
            worker.wait()
