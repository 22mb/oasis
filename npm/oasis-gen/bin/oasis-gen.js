#!/usr/bin/env node
// Runs the prebuilt oasis-gen binary for this platform, installed by npm as
// one of the optional `@oasis-gen/<platform>-<arch>` dependencies.
"use strict";

const { spawnSync } = require("node:child_process");
const { chmodSync } = require("node:fs");
const path = require("node:path");

function repository() {
  const { url } = require("../package.json").repository ?? {};
  return url ? url.replace(/^git\+/, "").replace(/\.git$/, "") : "the oasis-gen repository";
}

function binary() {
  if (process.env.OASIS_GEN_BINARY) return process.env.OASIS_GEN_BINARY;
  const pkg = `@oasis-gen/${process.platform}-${process.arch}`;
  const exe = process.platform === "win32" ? "oasis-gen.exe" : "oasis-gen";
  try {
    return path.join(path.dirname(require.resolve(`${pkg}/package.json`)), "bin", exe);
  } catch {
    console.error(
      `oasis-gen: no prebuilt binary for ${process.platform}-${process.arch} (${pkg} is not installed).\n` +
        "If your package manager skips optional dependencies, reinstall without that option.\n" +
        `Otherwise build it with \`cargo install --git ${repository()}\` and set OASIS_GEN_BINARY to its path.`,
    );
    process.exit(1);
  }
}

const bin = binary();
const run = () => spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
let result = run();
// Some package managers drop the executable bit when extracting.
if (result.error && result.error.code === "EACCES" && process.platform !== "win32") {
  chmodSync(bin, 0o755);
  result = run();
}
if (result.error) {
  console.error(`oasis-gen: cannot run ${bin}: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) process.kill(process.pid, result.signal);
process.exit(result.status ?? 1);
