#!/usr/bin/env node

const { run } = require("../index.js");
const path = require("path");
const [_bin, script, ...args] = process.argv;

run([path.parse(script).name, ...args])
  .then(() => process.exit(0))
  .catch((e) => {
    console.error(e);
    process.exit(1);
  });
