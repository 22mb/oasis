// Assembles the npm packages for a release:
//
//   node npm/build.mjs <version> <artifacts> <out> [--partial]
//
// <artifacts>/<target>/oasis-gen[.exe] are the binaries built by CI. The output
// has one directory per platform package under <out>/platforms and the main
// package in <out>/oasis-gen, each ready for `npm publish`. `--partial` skips
// missing binaries (for local testing).

import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const targets = [
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { target: "aarch64-unknown-linux-musl", os: "linux", cpu: "arm64" },
  { target: "x86_64-unknown-linux-musl", os: "linux", cpu: "x64" },
  { target: "aarch64-pc-windows-msvc", os: "win32", cpu: "arm64" },
  { target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64" },
];

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const [version, artifacts, out] = process.argv.slice(2);
const partial = process.argv.includes("--partial");
if (!version || !artifacts || !out) {
  console.error("usage: node npm/build.mjs <version> <artifacts> <out> [--partial]");
  process.exit(2);
}
const cargo = readFileSync(path.join(root, "Cargo.toml"), "utf8");
const cargoVersion = cargo.match(/^version = "(.+)"$/m)?.[1];
// The single source of the repository URL; npm provenance checks it against the publishing repository.
const repositoryUrl = cargo.match(/^repository = "(.+)"$/m)?.[1];
if (version !== cargoVersion) {
  console.error(`version ${version} does not match Cargo.toml (${cargoVersion})`);
  process.exit(1);
}

const main = {
  ...JSON.parse(readFileSync(path.join(root, "npm/oasis-gen/package.json"), "utf8")),
  repository: { type: "git", url: `git+${repositoryUrl}.git` },
};
const docs = ["README.md", "LICENSE-MIT", "LICENSE-APACHE"];
const copyDocs = (dir) => docs.forEach((f) => copyFileSync(path.join(root, f), path.join(dir, f)));
rmSync(out, { recursive: true, force: true });

const optional = {};
for (const { target, os, cpu } of targets) {
  const exe = os === "win32" ? "oasis-gen.exe" : "oasis-gen";
  const binary = path.join(artifacts, target, exe);
  if (!existsSync(binary)) {
    if (partial) continue;
    console.error(`missing ${binary}`);
    process.exit(1);
  }
  const name = `@oasis-gen/${os}-${cpu}`;
  const dir = path.join(out, "platforms", `${os}-${cpu}`);
  mkdirSync(path.join(dir, "bin"), { recursive: true });
  copyFileSync(binary, path.join(dir, "bin", exe));
  chmodSync(path.join(dir, "bin", exe), 0o755);
  copyDocs(dir);
  const pkg = {
    name,
    version,
    description: `The oasis-gen binary for ${os}-${cpu}`,
    license: main.license,
    repository: main.repository,
    os: [os],
    cpu: [cpu],
    files: ["bin", "LICENSE-MIT", "LICENSE-APACHE"],
    preferUnplugged: true,
  };
  writeFileSync(path.join(dir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
  optional[name] = version;
}

const dir = path.join(out, "oasis-gen");
cpSync(path.join(root, "npm/oasis-gen"), dir, { recursive: true });
copyDocs(dir);
writeFileSync(
  path.join(dir, "package.json"),
  JSON.stringify({ ...main, version, optionalDependencies: optional }, null, 2) + "\n",
);
console.log(`built oasis-gen ${version} with ${Object.keys(optional).join(", ")}`);
