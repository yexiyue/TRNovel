#!/usr/bin/env node
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const binary = path.basename(__filename, '.js') + (process.platform === 'win32' ? '.exe' : '');
const result = spawnSync(path.join(__dirname, '..', 'native', binary), process.argv.slice(2), { stdio: 'inherit' });
if (result.error) { console.error(result.error.message); process.exitCode = 1; }
else { process.exitCode = result.status === null ? 1 : result.status; }
