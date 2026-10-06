#!/usr/bin/env python3
"""Keep the generic cargo-dist app version aligned with the source package."""
from pathlib import Path
import re
import tomllib
root = Path(__file__).resolve().parents[2]
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
path = root / 'dist.toml'
text, count = re.subn(r'^version = "[^"]+"$', f'version = "{version}"', path.read_text(), count=1, flags=re.MULTILINE)
assert count == 1
path.write_text(text)
