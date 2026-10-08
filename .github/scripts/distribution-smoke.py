"""Build cargo-dist artifacts and exercise an isolated, model-free installation."""
import json
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from threading import Thread


def run(*args, **kwargs):
    try:
        return subprocess.run(args, check=True, timeout=60, **kwargs)
    except subprocess.CalledProcessError as error:
        if error.stdout:
            print(error.stdout, file=sys.stderr)
        if error.stderr:
            print(error.stderr, file=sys.stderr)
        raise


def main(app, target):
    run('dist', 'generate', '--check')
    subprocess.run(['dist', 'build', '--artifacts', 'local', '--target', target], check=True)
    run('dist', 'build', '--artifacts', 'global')
    archives = list(Path('target/distrib').glob(f'{app}-{target}.*'))
    archive = next(path for path in archives if path.suffix == '.zip' or path.name.endswith('.tar.xz'))
    with tempfile.TemporaryDirectory(prefix='isolated install ') as temporary:
        base = Path(temporary)
        shutil.unpack_archive(archive, base / 'install')
        suffix = '.exe' if sys.platform == 'win32' else ''
        programs = [app] if app == 'talechime' else ['trnovel', 'trn']
        installed = next((base / 'install').rglob(app + suffix)).parent
        files = list((base / 'install').rglob('*'))
        assert not any(path.name in ['novel-tts', 'novel-tts.exe'] for path in files)
        assert not any(path.suffix in ['.onnx', '.gguf', '.safetensors'] for path in files)
        if app == 'trnovel':
            assert not any('talechime' in path.name or 'onnxruntime' in path.name for path in files)
        else:
            assert (installed / 'THIRD_PARTY_LICENSES').is_dir()
        home = base / 'home'
        home.mkdir()
        env = dict(os.environ, HOME=str(home), USERPROFILE=str(home))
        for key in ['LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH', 'CUDA_PATH', 'CUDA_HOME']:
            env.pop(key, None)
        preserved = [home / '.novel/history.json', home / '.trnovel/config.toml',
                     home / '.trnovel/data/book_sources.json',
                     home / '.trnovel/data/source-state/login.json']
        if app == 'trnovel':
            for path in preserved:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('sentinel', encoding='utf-8')
        for program in programs:
            binary = str(installed / (program + suffix))
            run(binary, '--version', env=env, cwd=home)
            help_text = run(binary, '--help', env=env, cwd=home, capture_output=True, text=True).stdout
            if app == 'trnovel':
                assert '--tts-program' in help_text
                history = home / '.trnovel/data/history.json'
                history.write_text('clear this record', encoding='utf-8')
                run(binary, 'clear', env=env, cwd=home)
                assert not history.exists()
                assert all(path.read_text() == 'sentinel' for path in preserved)
            else:
                requests = ''.join(json.dumps(dict(protocol_version=5, request_id=kind,
                    session_id=None, type=kind)) + '\n' for kind in ['hello', 'shutdown'])
                result = run(binary, '--protocol', input=requests, capture_output=True,
                    text=True, encoding='utf-8', env=env, cwd=home)
                messages = [json.loads(line) for line in result.stdout.splitlines()]
                assert messages[0]['type'] == 'ready' and messages[0]['protocol_version'] == 5
                assert messages[-1]['type'] == 'accepted'
        assert not list(home.rglob('*.onnx')), 'Handshake downloaded weights'
        assert not list(home.rglob('config.json')), 'Handshake persisted configuration'
        # Exercise the generated platform installer against local build artifacts.
        # Its supported download override keeps this smoke independent of releases.
        server = ThreadingHTTPServer(('127.0.0.1', 0), partial(
            SimpleHTTPRequestHandler, directory=str(Path('target/distrib').resolve())))
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        prefix = app.upper()
        installer_env = dict(env)
        installer_env.update({
            f'{prefix}_DOWNLOAD_URL': f'http://127.0.0.1:{server.server_port}',
            f'{prefix}_UNMANAGED_INSTALL': str(base / 'installer-bin'),
            'XDG_CONFIG_HOME': str(home / '.config'),
            'LOCALAPPDATA': str(home / 'AppData/Local'),
        })
        try:
            if sys.platform == 'win32':
                run('pwsh', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                    str(Path(f'target/distrib/{app}-installer.ps1').resolve()), env=installer_env)
            else:
                run('sh', str(Path(f'target/distrib/{app}-installer.sh').resolve()), env=installer_env)
            for program in programs:
                run(str(base / 'installer-bin' / (program + suffix)), '--version', env=env, cwd=home)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
    print(f'{app}: archive and platform installer passed for {target}')


if __name__ == '__main__':
    main(*sys.argv[1:])
