const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const https = require('node:https');
const crypto = require('node:crypto');
const os = require('node:os');
const { pipeline } = require('node:stream/promises');
const { execFileSync } = require('node:child_process');
const metadata = require('./package.json');
const targets = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
};
const target = targets[`${process.platform}-${process.arch}`];
async function download(url, destination, redirects = 0) {
  if (redirects > 5) throw new Error('Too many download redirects');
  const client = new URL(url).protocol === 'https:' ? https : http;
  const response = await new Promise((resolve, reject) => {
    const request = client.get(url, resolve);
    request.setTimeout(30_000, () => request.destroy(new Error('Download timed out')));
    request.on('error', reject);
  });
  if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
    response.resume();
    return download(new URL(response.headers.location, url).href, destination, redirects + 1);
  }
  if (response.statusCode !== 200) { response.resume(); throw new Error(`Download failed: HTTP ${response.statusCode}`); }
  await pipeline(response, fs.createWriteStream(destination));
}
async function install() {
  if (!target) throw new Error('Unsupported platform; see manual installation');
  if (process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error('This package needs GNU libc; use the musl archive on Alpine');
  }
  const extension = process.platform === 'win32' ? 'zip' : 'tar.xz';
  const asset = `trnovel-basic-${target}.${extension}`;
  const base = process.env.TRNOVEL_RELEASE_BASE_URL || `https://github.com/yexiyue/TRNovel/releases/download/trnovel-v${metadata.version}`;
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'trnovel-basic-'));
  try {
    const archive = path.join(temporary, asset);
    const checksum = `${archive}.sha256`;
    await download(`${base}/${asset}`, archive);
    await download(`${base}/${asset}.sha256`, checksum);
    const expected = fs.readFileSync(checksum, 'utf8').trim().split(/\s+/)[0];
    const hasher = crypto.createHash('sha256');
    for await (const chunk of fs.createReadStream(archive)) hasher.update(chunk);
    if (hasher.digest('hex') !== expected) throw new Error('Archive checksum mismatch');
    execFileSync('tar', ['-xf', archive, '-C', temporary]);
    const native = path.join(__dirname, 'native');
    fs.mkdirSync(native, { recursive: true });
    for (const name of ['trnovel', 'trn']) {
      const binary = name + (process.platform === 'win32' ? '.exe' : '');
      const destination = path.join(native, binary);
      fs.copyFileSync(path.join(temporary, `trnovel-basic-${target}`, binary), destination);
      fs.chmodSync(destination, 0o755);
    }
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
}
install().catch(error => { console.error(error.message); process.exitCode = 1; });
