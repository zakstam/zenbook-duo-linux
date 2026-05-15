#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'EOF'
bump-version.sh - update Zenbook Duo release metadata

Usage:
  ./bump-version.sh patch        # 0.3.3 -> 0.3.4
  ./bump-version.sh minor        # 0.3.3 -> 0.4.0
  ./bump-version.sh major        # 0.3.3 -> 1.0.0
  ./bump-version.sh 0.3.4       # set an explicit version

Updates:
  - ui-tauri-react/package.json
  - ui-tauri-react/package-lock.json
  - ui-tauri-react/src-tauri/Cargo.toml
  - ui-tauri-react/src-tauri/Cargo.lock
  - ui-tauri-react/src-tauri/tauri.conf.json
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || "${1:-}" == "help" ]]; then
  usage
  exit 0
fi

if [[ $# -ne 1 ]]; then
  echo "ERROR: expected exactly one version argument" >&2
  echo >&2
  usage >&2
  exit 1
fi

node - "${ROOT_DIR}" "$1" <<'NODE'
const fs = require('fs');
const path = require('path');

const [, , rootDir, requested] = process.argv;
const packageJsonPath = path.join(rootDir, 'ui-tauri-react/package.json');
const packageLockPath = path.join(rootDir, 'ui-tauri-react/package-lock.json');
const cargoTomlPath = path.join(rootDir, 'ui-tauri-react/src-tauri/Cargo.toml');
const cargoLockPath = path.join(rootDir, 'ui-tauri-react/src-tauri/Cargo.lock');
const tauriConfigPath = path.join(rootDir, 'ui-tauri-react/src-tauri/tauri.conf.json');

function read(file) {
  return fs.readFileSync(file, 'utf8');
}

function write(file, contents) {
  fs.writeFileSync(file, contents);
}

function parseVersion(version, label) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    throw new Error(`${label} must be a plain semver version like 0.3.4`);
  }
  return match.slice(1).map(Number);
}

function nextVersion(current, bump) {
  const [major, minor, patch] = parseVersion(current, 'Current version');
  switch (bump) {
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'major':
      return `${major + 1}.0.0`;
    default:
      parseVersion(bump, 'Requested version');
      return bump;
  }
}

function replaceOne(file, regex, replacement) {
  const original = read(file);
  let count = 0;
  const updated = original.replace(regex, (...args) => {
    count += 1;
    return typeof replacement === 'function' ? replacement(...args) : replacement;
  });
  if (count !== 1) {
    throw new Error(`Expected exactly one match in ${path.relative(rootDir, file)}, found ${count}`);
  }
  write(file, updated);
}

function verifyContains(file, regex, message) {
  if (!regex.test(read(file))) {
    throw new Error(message);
  }
}

const currentVersion = JSON.parse(read(packageJsonPath)).version;
const targetVersion = nextVersion(currentVersion, requested);

replaceOne(
  packageJsonPath,
  /("version"\s*:\s*")[^"]+(")/,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);
replaceOne(
  packageLockPath,
  /("name"\s*:\s*"ui-tauri-react",\n\s*"version"\s*:\s*")[^"]+(")/,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);
replaceOne(
  packageLockPath,
  /(""\s*:\s*\{\n\s*"name"\s*:\s*"ui-tauri-react",\n\s*"version"\s*:\s*")[^"]+(")/,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);
replaceOne(
  cargoTomlPath,
  /(^version\s*=\s*")[^"]+(")/m,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);
replaceOne(
  cargoLockPath,
  /(\[\[package\]\]\nname = "zenbook-duo-control"\nversion = ")[^"]+(")/,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);
replaceOne(
  tauriConfigPath,
  /("version"\s*:\s*")[^"]+(")/,
  (_, prefix, suffix) => `${prefix}${targetVersion}${suffix}`,
);

verifyContains(packageJsonPath, new RegExp(`"version"\\s*:\\s*"${targetVersion}"`), 'package.json version did not update');
verifyContains(packageLockPath, new RegExp(`"version"\\s*:\\s*"${targetVersion}"`), 'package-lock.json version did not update');
verifyContains(cargoTomlPath, new RegExp(`^version\\s*=\\s*"${targetVersion}"`, 'm'), 'Cargo.toml version did not update');
verifyContains(cargoLockPath, new RegExp(`name = "zenbook-duo-control"\\nversion = "${targetVersion}"`), 'Cargo.lock version did not update');
verifyContains(tauriConfigPath, new RegExp(`"version"\\s*:\\s*"${targetVersion}"`), 'tauri.conf.json version did not update');

console.log(`Bumped version: ${currentVersion} -> ${targetVersion}`);
NODE
