#!/usr/bin/env python3
"""Global cargo-dist extra artifacts: base archives already came from custom CI."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

root = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--directory', type=Path, default=root / 'target/distrib')
parser.add_argument('--platform', action='append', help='Restrict local smoke generation; CI includes every target')
args = parser.parse_args()
output = args.directory.resolve()
output.mkdir(parents=True, exist_ok=True)
version = re.search(r'^version = "([^"]+)"', (root / 'Cargo.toml').read_text(), re.MULTILINE).group(1)
targets = args.platform or ['aarch64-apple-darwin', 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']
checksums = {}
for target in targets:
    extension = 'zip' if 'windows' in target else 'tar.xz'
    archive = output / f'trnovel-basic-{target}.{extension}'
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    expected = archive.with_name(archive.name + '.sha256').read_text().split()[0]
    assert actual == expected, f'Checksum mismatch: {archive}'
    checksums[target] = actual
for extension in ('sh', 'ps1'):
    source = (root / f'.github/templates/basic-installer.{extension}').read_text()
    (output / f'trnovel-basic-installer.{extension}').write_text(source.replace('@VERSION@', version))
with tempfile.TemporaryDirectory() as directory:
    package = Path(directory) / 'package'
    shutil.copytree(root / 'packaging/basic-npm', package)
    metadata = dict(name='@trnovel/trnovel-basic', version=version, description='TRNovel basic reader without listening', license='MIT', repository='git+https://github.com/yexiyue/TRNovel.git', bin={'trnovel':'bin/trnovel.js', 'trn':'bin/trn.js'}, scripts={'postinstall':'node install.js'}, engines={'node':'>=18'})
    (package / 'package.json').write_text(json.dumps(metadata, indent=2)+'\n')
    with tarfile.open(output / 'trnovel-basic-npm-package.tar.gz', 'w:gz') as archive:
        archive.add(package, arcname='package')
base = f'https://github.com/yexiyue/TRNovel/releases/download/trnovel-v{version}'
formula = ['class TrnovelBasic < Formula', '  desc "TRNovel basic reader without listening"', '  homepage "https://yexiyue.github.io/TRNovel"', f'  version "{version}"', '  license "MIT"', '  conflicts_with "trnovel", because: "both provide trnovel and trn"']
mac = [target for target in targets if 'apple' in target]
if mac:
    formula.append('  on_macos do')
    for target in mac:
        cpu = 'arm' if target.startswith('aarch64') else 'intel'
        formula += [f'    on_{cpu} do', f'      url "{base}/trnovel-basic-{target}.tar.xz"', f'      sha256 "{checksums[target]}"', '    end']
    formula.append('  end')
if 'x86_64-unknown-linux-gnu' in checksums:
    target = 'x86_64-unknown-linux-gnu'
    formula += ['  on_linux do', '    on_intel do', f'      url "{base}/trnovel-basic-{target}.tar.xz"', f'      sha256 "{checksums[target]}"', '    end', '  end']
formula += ['  def install', '    bin.install "trnovel", "trn"', '  end', '  test do', '    assert_match version.to_s, shell_output("#{bin}/trnovel --version")', '  end', 'end']
(output / 'trnovel-basic.formula').write_text('\n'.join(formula)+'\n')
print(f'Generated basic installers for {version}: {", ".join(targets)}')
